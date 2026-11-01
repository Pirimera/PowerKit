//! App-grouped native tray menu: PowerKit actions at the root, AlwaysOn in a submenu.
use super::{
    DISPLAY, EXIT, FOREVER, HALF_HOUR, HOUR, LABEL_DISPLAY, LABEL_EXIT, LABEL_FOREVER,
    LABEL_HALF_HOUR, LABEL_HOUR, LABEL_OPEN, LABEL_TWO_HOURS, LABEL_UPDATE_MENU, OFF, OPEN,
    TWO_HOURS, UPDATE, theme, wide,
};
use crate::always_on::Mode;
use std::{mem::size_of, ptr::null};
use windows_sys::Win32::{
    Foundation::{HWND, POINT},
    UI::WindowsAndMessaging::{
        AppendMenuW, CheckMenuRadioItem, CreatePopupMenu, DestroyMenu, GetCursorPos,
        GetMenuItemCount, InsertMenuItemW, MENUITEMINFOW, MF_BYCOMMAND, MF_SEPARATOR, MFS_CHECKED,
        MFT_RADIOCHECK, MFT_STRING, MIIM_FTYPE, MIIM_ID, MIIM_STATE, MIIM_STRING, MIIM_SUBMENU,
        PostMessageW, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
    },
};
const LABEL_ALWAYS_ON_OFF: &str = "AlwaysOn kapalı";
const LABEL_ALWAYS_ON_ON: &str = "AlwaysOn açık";
const LABEL_OFF_RADIO: &str = "Kapalı";
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TrayEntry {
    Item {
        id: usize,
        label: String,
        mark: TrayMark,
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
fn always_on_root_label(mode: Mode, remaining_minutes: u64) -> String {
    match mode {
        Mode::Off => LABEL_ALWAYS_ON_OFF.into(),
        Mode::Indefinite => LABEL_ALWAYS_ON_ON.into(),
        Mode::Until(_) => format!("{LABEL_ALWAYS_ON_ON} ({remaining_minutes} dk kaldı)"),
    }
}
fn checked_radio(mode: Mode, timed_choice: Option<usize>) -> Option<usize> {
    match mode {
        Mode::Off => Some(OFF),
        Mode::Indefinite => Some(FOREVER),
        Mode::Until(_) => timed_choice.filter(|id| matches!(*id, HALF_HOUR | HOUR | TWO_HOURS)),
    }
}
fn radio_item(id: usize, label: &str, selected: Option<usize>) -> TrayEntry {
    TrayEntry::Item {
        id,
        label: label.into(),
        mark: TrayMark::Radio(selected == Some(id)),
    }
}
pub(super) fn tray_entries(
    mode: Mode,
    display: bool,
    remaining_minutes: u64,
    timed_choice: Option<usize>,
) -> Vec<TrayEntry> {
    let radio = checked_radio(mode, timed_choice);
    vec![
        TrayEntry::Submenu {
            label: always_on_root_label(mode, remaining_minutes),
            entries: vec![
                radio_item(OFF, LABEL_OFF_RADIO, radio),
                radio_item(FOREVER, LABEL_FOREVER, radio),
                radio_item(HALF_HOUR, LABEL_HALF_HOUR, radio),
                radio_item(HOUR, LABEL_HOUR, radio),
                radio_item(TWO_HOURS, LABEL_TWO_HOURS, radio),
                TrayEntry::Separator,
                TrayEntry::Item {
                    id: DISPLAY,
                    label: LABEL_DISPLAY.into(),
                    mark: TrayMark::Check(display),
                },
            ],
        },
        TrayEntry::Separator,
        TrayEntry::Item {
            id: OPEN,
            label: LABEL_OPEN.into(),
            mark: TrayMark::None,
        },
        TrayEntry::Item {
            id: UPDATE,
            label: LABEL_UPDATE_MENU.into(),
            mark: TrayMark::None,
        },
        TrayEntry::Item {
            id: EXIT,
            label: LABEL_EXIT.into(),
            mark: TrayMark::None,
        },
    ]
}
fn append(menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU, entry: &TrayEntry) -> bool {
    match entry {
        TrayEntry::Separator => unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) != 0 },
        TrayEntry::Item { id, label, mark } => {
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
                fState: if checked { MFS_CHECKED } else { 0 },
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
    fn marks(entries: &[TrayEntry]) -> Vec<(usize, TrayMark)> {
        always_on(entries)
            .iter()
            .filter_map(|entry| match entry {
                TrayEntry::Item { id, mark, .. } => Some((*id, *mark)),
                _ => None,
            })
            .collect()
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
        let off = tray_entries(Mode::Off, false, 0, None);
        let timed = tray_entries(Mode::Until(1), true, 23, Some(HALF_HOUR));
        assert!(matches!(
            &off[0],
            TrayEntry::Submenu { label, .. } if label == LABEL_ALWAYS_ON_OFF
        ));
        assert!(matches!(
            &timed[0],
            TrayEntry::Submenu { label, .. } if label == "AlwaysOn açık (23 dk kaldı)"
        ));
        assert!(matches!(off[1], TrayEntry::Separator));
        assert_eq!(root_commands(&off), [OPEN, UPDATE, EXIT]);
        assert_eq!(root_commands(&timed), [OPEN, UPDATE, EXIT]);
    }
    #[test]
    fn always_on_uses_radio_group_and_display_check() {
        let off = tray_entries(Mode::Off, false, 0, None);
        assert_eq!(
            marks(&off),
            [
                (OFF, TrayMark::Radio(true)),
                (FOREVER, TrayMark::Radio(false)),
                (HALF_HOUR, TrayMark::Radio(false)),
                (HOUR, TrayMark::Radio(false)),
                (TWO_HOURS, TrayMark::Radio(false)),
                (DISPLAY, TrayMark::Check(false)),
            ]
        );
        let running = tray_entries(Mode::Until(1), true, 12, Some(HOUR));
        assert_eq!(
            marks(&running)
                .into_iter()
                .filter(|(id, _)| *id != DISPLAY)
                .collect::<Vec<_>>(),
            [
                (OFF, TrayMark::Radio(false)),
                (FOREVER, TrayMark::Radio(false)),
                (HALF_HOUR, TrayMark::Radio(false)),
                (HOUR, TrayMark::Radio(true)),
                (TWO_HOURS, TrayMark::Radio(false)),
            ]
        );
        assert_eq!(
            marks(&running).into_iter().find(|(id, _)| *id == DISPLAY),
            Some((DISPLAY, TrayMark::Check(true)))
        );
        let unknown = tray_entries(Mode::Until(1), false, 5, Some(OPEN));
        assert!(
            marks(&unknown)
                .into_iter()
                .filter(|(id, mark)| *id != DISPLAY && matches!(mark, TrayMark::Radio(true)))
                .count()
                == 0
        );
    }
}
