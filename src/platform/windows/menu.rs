//! App-grouped native tray menu: PowerKit actions at the root, tools in status-bearing submenus.
use super::{
    DISPLAY, EXIT, FOREVER, HALF_HOUR, HOUR, MEASURE, OFF, OPACITY_DOWN, OPACITY_UP, OPEN, PIN,
    RESIZE, SOUND, TWO_HOURS, UPDATE, theme, wide,
};
use crate::always_on::Mode;
use crate::always_top::Pin;
use std::{mem::size_of, ptr::null};
use windows_sys::Win32::{
    Foundation::{HWND, POINT},
    UI::WindowsAndMessaging::{
        AppendMenuW, CheckMenuRadioItem, CreatePopupMenu, DestroyMenu, GetCursorPos,
        GetMenuItemCount, InsertMenuItemW, MENUITEMINFOW, MF_BYCOMMAND, MF_SEPARATOR, MFS_CHECKED,
        MFS_GRAYED, MFT_RADIOCHECK, MFT_STRING, MIIM_FTYPE, MIIM_ID, MIIM_STATE, MIIM_STRING,
        MIIM_SUBMENU, PostMessageW, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON,
        TrackPopupMenu, WM_NULL,
    },
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TrayEntry {
    Item {
        id: usize,
        label: String,
        mark: TrayMark,
        enabled: bool,
    },
    Separator,
    Submenu {
        label: String,
        entries: Vec<TrayEntry>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrayMark {
    None,
    Check(bool),
    Radio(bool),
}
pub(super) fn remaining_minutes(now_ms: u64, end: u64) -> u64 {
    end.saturating_sub(now_ms).div_ceil(60_000)
}
fn always_on_root_label(mode: Mode, remaining_minutes: u64, t: &crate::locale::Strings) -> String {
    match mode {
        Mode::Off => t.tray_always_on_off.into(),
        Mode::Indefinite => t.tray_always_on_on.into(),
        Mode::Until(_) => {
            format!(
                "{} ({remaining_minutes} {})",
                t.tray_always_on_on, t.tray_always_on_left
            )
        }
    }
}
fn always_top_root_label(pin: &Pin, t: &crate::locale::Strings) -> String {
    match pin.title() {
        Some(title) => format!("{} ({title})", t.tray_always_top_on),
        None => t.tray_always_top_off.into(),
    }
}
fn checked_radio(mode: Mode, timed_choice: Option<usize>) -> Option<usize> {
    match mode {
        Mode::Off => Some(OFF),
        Mode::Indefinite => Some(FOREVER),
        Mode::Until(_) => timed_choice.filter(|id| matches!(*id, HALF_HOUR | HOUR | TWO_HOURS)),
    }
}
fn item(id: usize, label: impl Into<String>, mark: TrayMark, enabled: bool) -> TrayEntry {
    TrayEntry::Item {
        id,
        label: label.into(),
        mark,
        enabled,
    }
}
fn radio_item(id: usize, label: &str, selected: Option<usize>) -> TrayEntry {
    item(id, label, TrayMark::Radio(selected == Some(id)), true)
}
fn shortcut_label(label: &str, hotkey: Option<&str>) -> String {
    match hotkey {
        Some(key) => format!("{label}\t{key}"),
        None => label.into(),
    }
}
pub(super) struct AlwaysTopMenu<'a> {
    pub pin: &'a Pin,
    pub hotkey: Option<&'a str>,
    pub sound: bool,
    pub opacity_up: Option<&'a str>,
    pub opacity_down: Option<&'a str>,
}
pub(super) struct ScreenRulerMenu<'a> {
    pub active: bool,
    pub hotkey: Option<&'a str>,
}
pub(super) fn tray_entries(
    mode: Mode,
    display: bool,
    remaining_minutes: u64,
    timed_choice: Option<usize>,
    always_top: AlwaysTopMenu<'_>,
    ruler: ScreenRulerMenu<'_>,
    lang: crate::locale::UiLanguage,
) -> Vec<TrayEntry> {
    let t = lang.strings();
    let radio = checked_radio(mode, timed_choice);
    let pinned = always_top.pin.active();
    vec![
        TrayEntry::Submenu {
            label: always_on_root_label(mode, remaining_minutes, t),
            entries: vec![
                radio_item(OFF, t.tray_off_radio, radio),
                radio_item(HALF_HOUR, t.tray_half, radio),
                radio_item(HOUR, t.tray_hour, radio),
                radio_item(TWO_HOURS, t.tray_two, radio),
                radio_item(FOREVER, t.tray_forever, radio),
                TrayEntry::Separator,
                item(DISPLAY, t.tray_display, TrayMark::Check(display), true),
            ],
        },
        TrayEntry::Submenu {
            label: always_top_root_label(always_top.pin, t),
            entries: vec![
                item(
                    PIN,
                    shortcut_label(t.tray_pin, always_top.hotkey),
                    TrayMark::Check(pinned),
                    true,
                ),
                item(SOUND, t.sound, TrayMark::Check(always_top.sound), true),
                TrayEntry::Separator,
                item(
                    OPACITY_UP,
                    shortcut_label(t.hotkey_op_up, always_top.opacity_up),
                    TrayMark::None,
                    pinned,
                ),
                item(
                    OPACITY_DOWN,
                    shortcut_label(t.hotkey_op_down, always_top.opacity_down),
                    TrayMark::None,
                    pinned,
                ),
            ],
        },
        TrayEntry::Submenu {
            label: if ruler.active {
                t.ruler_on.into()
            } else {
                t.ruler_off.into()
            },
            entries: vec![item(
                MEASURE,
                shortcut_label(t.tray_measure, ruler.hotkey),
                TrayMark::Check(ruler.active),
                true,
            )],
        },
        TrayEntry::Submenu {
            label: t.nav_image_resizer.into(),
            entries: vec![item(RESIZE, t.tray_resize, TrayMark::None, true)],
        },
        TrayEntry::Separator,
        item(OPEN, t.open_window, TrayMark::None, true),
        item(UPDATE, t.update_menu, TrayMark::None, true),
        item(EXIT, t.exit, TrayMark::None, true),
    ]
}
fn append(menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU, entry: &TrayEntry) -> bool {
    match entry {
        TrayEntry::Separator => unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) != 0 },
        TrayEntry::Item {
            id,
            label,
            mark,
            enabled,
        } => {
            let mut text = wide(label);
            let checked = matches!(mark, TrayMark::Check(true) | TrayMark::Radio(true));
            let info = MENUITEMINFOW {
                cbSize: size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_FTYPE | MIIM_STRING | MIIM_ID | MIIM_STATE,
                fType: MFT_STRING
                    | if matches!(mark, TrayMark::Radio(_)) {
                        MFT_RADIOCHECK
                    } else {
                        0
                    },
                fState: (if checked { MFS_CHECKED } else { 0 })
                    | if *enabled { 0 } else { MFS_GRAYED },
                wID: *id as u32,
                dwTypeData: text.as_mut_ptr(),
                cch: text.len().saturating_sub(1) as u32,
                ..Default::default()
            };
            // SAFETY: text is a live NUL-terminated buffer; menu is the popup being built.
            unsafe { InsertMenuItemW(menu, GetMenuItemCount(menu) as u32, 1, &info) != 0 }
        }
        TrayEntry::Submenu { label, entries } => {
            let sub = unsafe { CreatePopupMenu() };
            if sub.is_null() {
                return false;
            }
            for child in entries {
                if !append(sub, child) {
                    unsafe {
                        DestroyMenu(sub);
                    }
                    return false;
                }
            }
            if let Some(selected) = entries.iter().find_map(|child| match child {
                TrayEntry::Item {
                    id,
                    mark: TrayMark::Radio(true),
                    ..
                } => Some(*id as u32),
                _ => None,
            }) {
                unsafe {
                    CheckMenuRadioItem(sub, OFF as u32, TWO_HOURS as u32, selected, MF_BYCOMMAND);
                }
            }
            let mut text = wide(label);
            let info = MENUITEMINFOW {
                cbSize: size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_FTYPE | MIIM_STRING | MIIM_SUBMENU,
                fType: MFT_STRING,
                hSubMenu: sub,
                dwTypeData: text.as_mut_ptr(),
                cch: text.len().saturating_sub(1) as u32,
                ..Default::default()
            };
            // SAFETY: submenu is owned by this item after a successful insert.
            let ok = unsafe { InsertMenuItemW(menu, GetMenuItemCount(menu) as u32, 1, &info) != 0 };
            if !ok {
                unsafe {
                    DestroyMenu(sub);
                }
            }
            ok
        }
    }
}
pub(super) fn track(hwnd: HWND, entries: &[TrayEntry]) -> Option<usize> {
    // SAFETY: popup and any submenu live until TrackPopupMenu returns; then DestroyMenu.
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() || !entries.iter().all(|entry| append(menu, entry)) {
            if !menu.is_null() {
                DestroyMenu(menu);
            }
            super::report("Menü oluşturulamadı");
            return None;
        }
        let mut point = POINT::default();
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let _chrome = theme::prepare_popup();
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        ) as usize;
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
        (command != 0).then_some(command)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn root_commands(entries: &[TrayEntry]) -> Vec<usize> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                TrayEntry::Item { id, .. } => Some(*id),
                _ => None,
            })
            .collect()
    }
    fn always_on(entries: &[TrayEntry]) -> &[TrayEntry] {
        match &entries[0] {
            TrayEntry::Submenu { entries, .. } => entries,
            other => panic!("{other:?}"),
        }
    }
    fn always_top(entries: &[TrayEntry]) -> &TrayEntry {
        &entries[1]
    }
    fn marks(entries: &[TrayEntry]) -> Vec<(usize, TrayMark)> {
        always_on(entries)
            .iter()
            .filter_map(|entry| match entry {
                TrayEntry::Item { id, mark, .. } => Some((*id, *mark)),
                _ => None,
            })
            .collect()
    }
    fn entries(
        mode: Mode,
        display: bool,
        remaining: u64,
        timed: Option<usize>,
        pin: &Pin,
        hotkey: Option<&str>,
    ) -> Vec<TrayEntry> {
        tray_entries(
            mode,
            display,
            remaining,
            timed,
            AlwaysTopMenu {
                pin,
                hotkey,
                sound: true,
                opacity_up: Some("Win+Ctrl++"),
                opacity_down: Some("Win+Ctrl+-"),
            },
            ScreenRulerMenu {
                active: false,
                hotkey: None,
            },
            crate::locale::UiLanguage::Turkish,
        )
    }
    #[test]
    fn remaining_minutes_round_up_without_wrapping() {
        assert_eq!(remaining_minutes(0, 23 * 60_000), 23);
        assert_eq!(remaining_minutes(0, 23 * 60_000 - 1), 23);
        assert_eq!(remaining_minutes(1_000, 1_000), 0);
        assert_eq!(remaining_minutes(u64::MAX, 1), 0);
    }
    #[test]
    fn root_keeps_powerkit_actions_and_always_on_status() {
        let off = entries(Mode::Off, false, 0, None, &Pin::Off, None);
        let timed = entries(Mode::Until(1), true, 23, Some(HALF_HOUR), &Pin::Off, None);
        let t = crate::locale::UiLanguage::Turkish.strings();
        assert!(matches!(
            &off[0],
            TrayEntry::Submenu { label, .. } if label == t.tray_always_on_off
        ));
        assert!(matches!(
            &timed[0],
            TrayEntry::Submenu { label, .. } if label == "AlwaysOn açık (23 dk kaldı)"
        ));
        assert!(matches!(
            always_top(&off),
            TrayEntry::Submenu { label, .. } if label == t.tray_always_top_off
        ));
        assert!(matches!(
            &off[3],
            TrayEntry::Submenu { label, .. } if label == t.nav_image_resizer
        ));
        assert!(matches!(off[4], TrayEntry::Separator));
        assert_eq!(root_commands(&off), [OPEN, UPDATE, EXIT]);
        assert_eq!(root_commands(&timed), [OPEN, UPDATE, EXIT]);
    }
    #[test]
    fn always_top_uses_status_label_and_pin_check() {
        let off = entries(Mode::Off, false, 0, None, &Pin::Off, Some("Win+Ctrl+T"));
        let on = tray_entries(
            Mode::Off,
            false,
            0,
            None,
            AlwaysTopMenu {
                pin: &Pin::on("Notepad"),
                hotkey: None,
                sound: false,
                opacity_up: Some("Win+Ctrl++"),
                opacity_down: Some("Win+Ctrl+-"),
            },
            ScreenRulerMenu {
                active: false,
                hotkey: None,
            },
            crate::locale::UiLanguage::Turkish,
        );
        let t = crate::locale::UiLanguage::Turkish.strings();
        assert!(matches!(
            always_top(&off),
            TrayEntry::Submenu {
                label,
                entries
            } if label == t.tray_always_top_off
                && matches!(
                    entries.as_slice(),
                    [
                        TrayEntry::Item { id: PIN, label, mark: TrayMark::Check(false), enabled: true, .. },
                        TrayEntry::Item { id: SOUND, mark: TrayMark::Check(true), enabled: true, .. },
                        TrayEntry::Separator,
                        TrayEntry::Item { id: OPACITY_UP, enabled: false, .. },
                        TrayEntry::Item { id: OPACITY_DOWN, enabled: false, .. },
                    ]
                        if label == "Pencereyi üstte tut\tWin+Ctrl+T"
                )
        ));
        assert!(matches!(
            always_top(&on),
            TrayEntry::Submenu {
                label,
                entries
            } if label == "AlwaysTop açık (Notepad)"
                && matches!(
                    entries.as_slice(),
                    [
                        TrayEntry::Item { id: PIN, mark: TrayMark::Check(true), enabled: true, .. },
                        TrayEntry::Item { id: SOUND, mark: TrayMark::Check(false), enabled: true, .. },
                        TrayEntry::Separator,
                        TrayEntry::Item { id: OPACITY_UP, enabled: true, .. },
                        TrayEntry::Item { id: OPACITY_DOWN, enabled: true, .. },
                    ]
                )
        ));
    }
    #[test]
    fn always_on_uses_radio_group_and_display_check() {
        let off = entries(Mode::Off, false, 0, None, &Pin::Off, None);
        assert_eq!(
            marks(&off),
            [
                (OFF, TrayMark::Radio(true)),
                (HALF_HOUR, TrayMark::Radio(false)),
                (HOUR, TrayMark::Radio(false)),
                (TWO_HOURS, TrayMark::Radio(false)),
                (FOREVER, TrayMark::Radio(false)),
                (DISPLAY, TrayMark::Check(false)),
            ]
        );
        let running = entries(Mode::Until(1), true, 12, Some(HOUR), &Pin::Off, None);
        assert_eq!(
            marks(&running)
                .into_iter()
                .filter(|(id, _)| *id != DISPLAY)
                .collect::<Vec<_>>(),
            [
                (OFF, TrayMark::Radio(false)),
                (HALF_HOUR, TrayMark::Radio(false)),
                (HOUR, TrayMark::Radio(true)),
                (TWO_HOURS, TrayMark::Radio(false)),
                (FOREVER, TrayMark::Radio(false)),
            ]
        );
        assert_eq!(
            marks(&running).into_iter().find(|(id, _)| *id == DISPLAY),
            Some((DISPLAY, TrayMark::Check(true)))
        );
        let unknown = entries(Mode::Until(1), false, 5, Some(OPEN), &Pin::Off, None);
        assert!(
            marks(&unknown)
                .into_iter()
                .filter(|(id, mark)| *id != DISPLAY && matches!(mark, TrayMark::Radio(true)))
                .count()
                == 0
        );
    }
}
