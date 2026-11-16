//! App-grouped native tray menu: PowerKit actions at the root, tools in status-bearing submenus.
use super::{
    command::{
        AppCommand, BIND, CUSTOM, DISPLAY, EXIT, EXPIRY, EXTEND, FOREVER, HALF_HOUR, HOUR, MEASURE,
        OFF, OPACITY_DOWN, OPACITY_UP, OPEN, PIN, POWER_STORE_CHECK_NOW, POWER_STORE_OPEN,
        POWER_STORE_UPDATE_ALL, RESIZE, SOUND, TWO_HOURS, UPDATE,
    },
    theme, wide,
};
use crate::always_on::Session;
use crate::always_top::Pin;
use std::{mem::size_of, ptr::null};
use windows_sys::Win32::{
    Foundation::{HWND, POINT},
    UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetMenuItemCount, InsertMenuItemW,
        MENUITEMINFOW, MF_SEPARATOR, MFS_CHECKED, MFS_GRAYED, MFT_RADIOCHECK, MFT_STRING,
        MIIM_FTYPE, MIIM_ID, MIIM_STATE, MIIM_STRING, MIIM_SUBMENU, PostMessageW,
        SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
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
fn always_top_root_label(pin: &Pin, t: &crate::locale::Strings) -> String {
    match pin.title() {
        Some(title) => format!("{} ({title})", t.tray_always_top_on),
        None => t.tray_always_top_off.into(),
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
pub(super) struct AlwaysOnMenu<'a> {
    pub session: Session,
    pub display: bool,
    pub interval_minutes: u32,
    /// Submenu label from `Strings::always_on_status`.
    pub status: &'a str,
    /// Local end time while an expiry session runs.
    pub expiry: Option<&'a str>,
    /// Process name while a process-bound session runs.
    pub bound_app: Option<&'a str>,
}
fn always_on_entries(menu: &AlwaysOnMenu<'_>, t: &crate::locale::Strings) -> Vec<TrayEntry> {
    let radio = Some(AppCommand::for_session(menu.session).id());
    let custom = format!(
        "{} ({})",
        t.tray_custom,
        t.duration(u64::from(menu.interval_minutes))
    );
    let mut entries = vec![
        radio_item(OFF, t.tray_off_radio, radio),
        radio_item(HALF_HOUR, t.tray_half, radio),
        radio_item(HOUR, t.tray_hour, radio),
        radio_item(TWO_HOURS, t.tray_two, radio),
        radio_item(CUSTOM, &custom, radio),
        radio_item(FOREVER, t.tray_forever, radio),
    ];
    // Binding targets the last eligible foreground window, like AlwaysTop pinning.
    let bind = match menu.bound_app {
        Some(app) => format!("{} ({app})", t.tray_bind),
        None => t.tray_bind.into(),
    };
    entries.push(item(BIND, bind, TrayMark::Radio(radio == Some(BIND)), true));
    // An expiry needs a picked time, so the tray only reports the running one.
    if let Some(end) = menu.expiry {
        entries.push(item(
            EXPIRY,
            format!("{} {end}", t.expiry_until),
            TrayMark::Radio(radio == Some(EXPIRY)),
            false,
        ));
    }
    entries.push(TrayEntry::Separator);
    if menu.session.deadline().is_some() {
        entries.push(item(EXTEND, t.tray_extend, TrayMark::None, true));
    }
    entries.push(item(
        DISPLAY,
        t.tray_display,
        TrayMark::Check(menu.display),
        true,
    ));
    entries
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
pub(super) struct PowerStoreMenu {
    /// Updates after ignore/skip rules; `None` before the first check.
    pub updates: Option<usize>,
    pub checking: bool,
}
fn power_store_entry(menu: &PowerStoreMenu, t: &crate::locale::Strings) -> TrayEntry {
    let (label, status) = match menu.updates {
        Some(count) if count > 0 => (
            format!("{} ({count})", t.nav_power_store),
            format!("{}: {count}", t.power_store_updates_count),
        ),
        Some(_) => (t.nav_power_store.into(), t.power_store_up_to_date.into()),
        None => (t.nav_power_store.into(), t.power_store_not_checked.into()),
    };
    let status = if menu.checking {
        t.power_store_checking.into()
    } else {
        status
    };
    TrayEntry::Submenu {
        label,
        entries: vec![
            item(POWER_STORE_STATUS, status, TrayMark::None, false),
            TrayEntry::Separator,
            item(
                POWER_STORE_UPDATE_ALL,
                t.power_store_update_all,
                TrayMark::None,
                menu.updates.is_some_and(|count| count > 0),
            ),
            item(
                POWER_STORE_CHECK_NOW,
                t.power_store_check_now,
                TrayMark::None,
                !menu.checking,
            ),
            item(POWER_STORE_OPEN, t.power_store_open, TrayMark::None, true),
        ],
    }
}
/// Disabled status rows need an id that never maps to a command.
const POWER_STORE_STATUS: usize = 0;
pub(super) fn tray_entries(
    always_on: AlwaysOnMenu<'_>,
    always_top: AlwaysTopMenu<'_>,
    ruler: ScreenRulerMenu<'_>,
    power_store: PowerStoreMenu,
    lang: crate::locale::UiLanguage,
) -> Vec<TrayEntry> {
    let t = lang.strings();
    let pinned = always_top.pin.active();
    vec![
        TrayEntry::Submenu {
            label: always_on.status.into(),
            entries: always_on_entries(&always_on, t),
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
        power_store_entry(&power_store, t),
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
    fn radios(entries: &[TrayEntry]) -> Vec<(usize, bool)> {
        marks(entries)
            .into_iter()
            .filter_map(|(id, mark)| match mark {
                TrayMark::Radio(on) => Some((id, on)),
                _ => None,
            })
            .collect()
    }
    fn menu_for(
        session: Session,
        display: bool,
        status: &str,
        expiry: Option<&str>,
    ) -> Vec<TrayEntry> {
        tray_entries(
            AlwaysOnMenu {
                session,
                display,
                interval_minutes: 195,
                status,
                expiry,
                bound_app: match session {
                    Session::Process { .. } => Some("vlc"),
                    _ => None,
                },
            },
            AlwaysTopMenu {
                pin: &Pin::Off,
                hotkey: None,
                sound: true,
                opacity_up: Some("Win+Ctrl++"),
                opacity_down: Some("Win+Ctrl+-"),
            },
            ScreenRulerMenu {
                active: false,
                hotkey: None,
            },
            PowerStoreMenu {
                updates: None,
                checking: false,
            },
            crate::locale::UiLanguage::Turkish,
        )
    }
    fn always_top_menu(pin: &Pin, hotkey: Option<&str>, sound: bool) -> Vec<TrayEntry> {
        tray_entries(
            AlwaysOnMenu {
                session: Session::Off,
                display: false,
                interval_minutes: 60,
                status: "",
                expiry: None,
                bound_app: None,
            },
            AlwaysTopMenu {
                pin,
                hotkey,
                sound,
                opacity_up: Some("Win+Ctrl++"),
                opacity_down: Some("Win+Ctrl+-"),
            },
            ScreenRulerMenu {
                active: false,
                hotkey: None,
            },
            PowerStoreMenu {
                updates: None,
                checking: false,
            },
            crate::locale::UiLanguage::Turkish,
        )
    }
    #[test]
    fn root_keeps_powerkit_actions_and_always_on_status() {
        let t = crate::locale::UiLanguage::Turkish.strings();
        let off = menu_for(Session::Off, false, t.tray_always_on_off, None);
        let timed = menu_for(
            Session::interval(0, 30, false),
            true,
            "AlwaysOn açık (23 dk kaldı)",
            None,
        );
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
        assert!(matches!(
            &off[4],
            TrayEntry::Submenu { label, .. } if label == t.nav_power_store
        ));
        assert!(matches!(off[5], TrayEntry::Separator));
        assert_eq!(root_commands(&off), [OPEN, UPDATE, EXIT]);
        assert_eq!(root_commands(&timed), [OPEN, UPDATE, EXIT]);
    }
    #[test]
    fn power_store_shows_update_count_and_enables_update_all_only_with_updates() {
        let t = crate::locale::UiLanguage::Turkish.strings();
        let entry = |updates, checking| power_store_entry(&PowerStoreMenu { updates, checking }, t);
        let enabled = |entry: &TrayEntry, wanted: usize| match entry {
            TrayEntry::Submenu { entries, .. } => entries.iter().find_map(|entry| match entry {
                TrayEntry::Item { id, enabled, .. } if *id == wanted => Some(*enabled),
                _ => None,
            }),
            _ => None,
        };
        let pending = entry(Some(3), false);
        assert!(matches!(&pending, TrayEntry::Submenu { label, .. } if label == "Power Store (3)"));
        assert_eq!(enabled(&pending, POWER_STORE_UPDATE_ALL), Some(true));
        assert_eq!(enabled(&pending, POWER_STORE_OPEN), Some(true));
        let current = entry(Some(0), true);
        assert!(matches!(&current, TrayEntry::Submenu { label, .. } if label == t.nav_power_store));
        assert_eq!(enabled(&current, POWER_STORE_UPDATE_ALL), Some(false));
        assert_eq!(enabled(&current, POWER_STORE_CHECK_NOW), Some(false));
        assert_eq!(
            enabled(&entry(None, false), POWER_STORE_UPDATE_ALL),
            Some(false)
        );
    }
    #[test]
    fn always_top_uses_status_label_and_pin_check() {
        let off = always_top_menu(&Pin::Off, Some("Win+Ctrl+T"), true);
        let on = always_top_menu(&Pin::on("Notepad"), None, false);
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
        let off = menu_for(Session::Off, false, "", None);
        assert_eq!(
            marks(&off),
            [
                (OFF, TrayMark::Radio(true)),
                (HALF_HOUR, TrayMark::Radio(false)),
                (HOUR, TrayMark::Radio(false)),
                (TWO_HOURS, TrayMark::Radio(false)),
                (CUSTOM, TrayMark::Radio(false)),
                (FOREVER, TrayMark::Radio(false)),
                (BIND, TrayMark::Radio(false)),
                (DISPLAY, TrayMark::Check(false)),
            ]
        );
        let running = menu_for(Session::interval(0, 60, false), true, "", None);
        assert_eq!(
            radios(&running),
            [
                (OFF, false),
                (HALF_HOUR, false),
                (HOUR, true),
                (TWO_HOURS, false),
                (CUSTOM, false),
                (FOREVER, false),
                (BIND, false),
            ]
        );
        assert_eq!(
            marks(&running).into_iter().find(|(id, _)| *id == DISPLAY),
            Some((DISPLAY, TrayMark::Check(true)))
        );
        let custom = menu_for(Session::interval(0, 60, true), false, "", None);
        assert_eq!(
            radios(&custom)
                .into_iter()
                .filter(|(_, on)| *on)
                .collect::<Vec<_>>(),
            [(CUSTOM, true)]
        );
    }
    #[test]
    fn custom_item_shows_its_duration_and_expiry_is_reported_only_while_running() {
        let off = menu_for(Session::Off, false, "", None);
        assert!(always_on(&off).iter().any(|entry| matches!(
            entry,
            TrayEntry::Item { id: CUSTOM, label, .. } if label == "Özel süre açık tut (3 sa 15 dk)"
        )));
        assert!(!marks(&off).iter().any(|(id, _)| *id == EXPIRY));
        let expiry = menu_for(
            Session::Expiry {
                end: 10,
                target: 10,
            },
            false,
            "",
            Some("18:30"),
        );
        assert!(always_on(&expiry).iter().any(|entry| matches!(
            entry,
            TrayEntry::Item { id: EXPIRY, label, mark: TrayMark::Radio(true), enabled: false }
                if label == "bitiş 18:30"
        )));
        assert_eq!(radios(&expiry).into_iter().filter(|(_, on)| *on).count(), 1);
    }
    #[test]
    fn extend_appears_only_for_deadlines_and_binding_names_the_app() {
        let has_extend = |entries: &[TrayEntry]| marks(entries).iter().any(|(id, _)| *id == EXTEND);
        assert!(!has_extend(&menu_for(Session::Off, false, "", None)));
        assert!(!has_extend(&menu_for(Session::Indefinite, false, "", None)));
        assert!(has_extend(&menu_for(
            Session::interval(0, 30, false),
            false,
            "",
            None
        )));
        assert!(has_extend(&menu_for(
            Session::Expiry { end: 9, target: 9 },
            false,
            "",
            Some("18:30")
        )));
        let bound = menu_for(Session::Process { pid: 7 }, false, "", None);
        assert!(!has_extend(&bound));
        assert!(always_on(&bound).iter().any(|entry| matches!(
            entry,
            TrayEntry::Item { id: BIND, label, mark: TrayMark::Radio(true), enabled: true }
                if label == "Uygulama çalıştıkça açık tut (vlc)"
        )));
    }
}
