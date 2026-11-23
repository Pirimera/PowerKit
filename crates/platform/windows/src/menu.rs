//! The native tray popup. `powerkit_app::view::tray` decides the entries; this
//! module builds them as a Win32 menu, maps each command to the `WM_COMMAND`
//! id the app window dispatches and each icon to its Segoe Fluent glyph.
use super::{
    command::{Command, MenuId, RESIZE},
    menu_hint::{self, Hints},
    menu_icon::MenuIcons,
    theme, wide,
};
use powerkit_app::controller::settings::Tool;
use powerkit_app::view::tray::{Command as TrayCommand, Entry, Icon, Mark};
use std::{
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{HWND, POINT},
    UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetMenuItemCount, HMENU,
        InsertMenuItemW, MENUINFO, MENUITEMINFOW, MF_SEPARATOR, MFS_CHECKED, MFS_GRAYED,
        MFT_RADIOCHECK, MFT_STRING, MIIM_BITMAP, MIIM_FTYPE, MIIM_ID, MIIM_STATE, MIIM_STRING,
        MIIM_SUBMENU, MIM_STYLE, MNS_CHECKORBMP, PostMessageW, SetForegroundWindow, SetMenuInfo,
        TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
    },
};

/// Keyboard glyph for the Shortcuts submenu.
/// The icon of the tray menu's shortcuts submenu.
pub const GLYPH_KEYBOARD: &str = "\u{E765}";

/// The `WM_COMMAND` id of a tray command. A row that only reports something
/// gets 0, which never maps to a command.
fn id(command: Option<TrayCommand>) -> usize {
    let Some(command) = command else {
        return 0;
    };
    match command {
        TrayCommand::AlwaysOn(choice) => Command::for_choice(choice).id(),
        TrayCommand::BindProcess => Command::AlwaysOnBindProcess.id(),
        TrayCommand::Extend => Command::AlwaysOnExtend.id(),
        TrayCommand::ToggleDisplay => Command::ToggleDisplay.id(),
        TrayCommand::TogglePin => Command::TogglePin.id(),
        TrayCommand::ToggleSound => Command::ToggleSound.id(),
        TrayCommand::OpacityUp => Command::OpacityUp.id(),
        TrayCommand::OpacityDown => Command::OpacityDown.id(),
        TrayCommand::ToggleMeasure => Command::ToggleMeasure.id(),
        // The tray's own id: it opens the dialog like the settings page does.
        TrayCommand::ResizeImages => RESIZE,
        TrayCommand::PowerStoreUpdateAll => Command::PowerStoreUpdateAll.id(),
        TrayCommand::PowerStoreCheckNow => Command::PowerStoreCheckNow.id(),
        TrayCommand::OpenPowerStore => Command::PowerStoreOpen.id(),
        TrayCommand::OpenMediaRenamer => Command::MediaRenamerOpen.id(),
        TrayCommand::ToggleMouse => Command::ToggleMouse.id(),
        TrayCommand::OpenPage(Tool::AlwaysOn) => Command::OpenAlwaysOnPage.id(),
        TrayCommand::OpenPage(Tool::AlwaysTop) => Command::OpenAlwaysTopPage.id(),
        TrayCommand::OpenPage(Tool::ScreenRuler) => Command::OpenScreenRulerPage.id(),
        // Only tools with a global shortcut are listed; any other opens the app.
        TrayCommand::OpenPage(_) | TrayCommand::OpenApp => Command::OpenSettings.id(),
        TrayCommand::Exit => Command::Exit.id(),
    }
}
fn glyph(icon: Icon) -> &'static str {
    match icon {
        Icon::Tool(tool) => super::tool_glyph(tool),
        Icon::Shortcuts => GLYPH_KEYBOARD,
    }
}
fn append(menu: HMENU, entry: &Entry, icons: &mut MenuIcons, hints: &mut Hints) -> bool {
    match entry {
        Entry::Separator => unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) != 0 },
        Entry::Item {
            command,
            label,
            mark,
            enabled,
            icon,
            hint,
        } => {
            let id = id(*command);
            if let Some(hint) = hint {
                hints.items.push((id, hint.clone()));
            }
            let mut text = wide(label);
            let checked = matches!(mark, Mark::Check(true) | Mark::Radio(true));
            let bitmap = icons.bitmap(icon.map(glyph));
            let info = MENUITEMINFOW {
                cbSize: size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_FTYPE
                    | MIIM_STRING
                    | MIIM_ID
                    | MIIM_STATE
                    | if bitmap.is_some() { MIIM_BITMAP } else { 0 },
                fType: MFT_STRING
                    | if matches!(mark, Mark::Radio(_)) {
                        MFT_RADIOCHECK
                    } else {
                        0
                    },
                fState: (if checked { MFS_CHECKED } else { 0 })
                    | if *enabled { 0 } else { MFS_GRAYED },
                wID: id as u32,
                dwTypeData: text.as_mut_ptr(),
                cch: text.len().saturating_sub(1) as u32,
                hbmpItem: bitmap.unwrap_or(null_mut()),
                ..Default::default()
            };
            // SAFETY: text is a live NUL-terminated buffer; menu is the popup being built.
            // The bitmap stays owned by `icons`, which outlives the popup.
            unsafe { InsertMenuItemW(menu, GetMenuItemCount(menu) as u32, 1, &info) != 0 }
        }
        Entry::Submenu {
            label,
            entries,
            icon,
            hint,
        } => {
            let sub = new_popup();
            if sub.is_null() {
                return false;
            }
            if let Some(hint) = hint {
                hints.popups.push((sub, hint.clone()));
            }
            for child in entries {
                if !append(sub, child, icons, hints) {
                    unsafe {
                        DestroyMenu(sub);
                    }
                    return false;
                }
            }
            let mut text = wide(label);
            let bitmap = icons.bitmap(icon.map(glyph));
            let info = MENUITEMINFOW {
                cbSize: size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_FTYPE
                    | MIIM_STRING
                    | MIIM_SUBMENU
                    | if bitmap.is_some() { MIIM_BITMAP } else { 0 },
                fType: MFT_STRING,
                hSubMenu: sub,
                dwTypeData: text.as_mut_ptr(),
                cch: text.len().saturating_sub(1) as u32,
                hbmpItem: bitmap.unwrap_or(null_mut()),
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
/// A popup whose items share one gutter for check mark and bitmap: an item shows
/// its bitmap, or its check/radio mark while checked. Null when creation fails.
fn new_popup() -> HMENU {
    // SAFETY: the new menu is owned by the caller (or by the item it is attached
    // to); MENUINFO is a stack value with its size set.
    unsafe {
        let menu = CreatePopupMenu();
        if !menu.is_null() {
            let style = MENUINFO {
                cbSize: size_of::<MENUINFO>() as u32,
                fMask: MIM_STYLE,
                dwStyle: MNS_CHECKORBMP,
                ..Default::default()
            };
            SetMenuInfo(menu, &style);
        }
        menu
    }
}
pub fn track(hwnd: HWND, entries: &[Entry]) -> Option<usize> {
    // Declared before the menu so the item bitmaps are freed after DestroyMenu.
    let mut icons = MenuIcons::new(hwnd, theme::popup_text_color());
    let mut hints = Hints::default();
    // SAFETY: popup and any submenu live until TrackPopupMenu returns; then DestroyMenu.
    unsafe {
        let menu = new_popup();
        if menu.is_null()
            || !entries
                .iter()
                .all(|entry| append(menu, entry, &mut icons, &mut hints))
        {
            if !menu.is_null() {
                DestroyMenu(menu);
            }
            super::report(super::fault::Fault::TrayMenu.text(super::ui_strings()));
            return None;
        }
        let mut point = POINT::default();
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let _chrome = theme::prepare_popup();
        // The owner window receives WM_MENUSELECT and the hint timer while the
        // menu loop runs; the guard hides the tooltip and forgets the HMENUs.
        let _hints = menu_hint::begin(hwnd, hints);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        ) as usize;
        drop(_hints);
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
        (command != 0).then_some(command)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use powerkit_core::always_on::Choice;

    #[test]
    fn every_tray_command_has_an_id_the_app_window_dispatches() {
        let [half, hour, two] = powerkit_core::always_on::PRESET_MINUTES;
        let commands = [
            (TrayCommand::AlwaysOn(Choice::Off), Command::AlwaysOnOff),
            (
                TrayCommand::AlwaysOn(Choice::Preset(half)),
                Command::AlwaysOnHalfHour,
            ),
            (
                TrayCommand::AlwaysOn(Choice::Preset(hour)),
                Command::AlwaysOnHour,
            ),
            (
                TrayCommand::AlwaysOn(Choice::Preset(two)),
                Command::AlwaysOnTwoHours,
            ),
            (
                TrayCommand::AlwaysOn(Choice::CustomInterval),
                Command::AlwaysOnInterval,
            ),
            (
                TrayCommand::AlwaysOn(Choice::Indefinite),
                Command::AlwaysOnForever,
            ),
            (TrayCommand::BindProcess, Command::AlwaysOnBindProcess),
            (TrayCommand::Extend, Command::AlwaysOnExtend),
            (TrayCommand::ToggleDisplay, Command::ToggleDisplay),
            (TrayCommand::TogglePin, Command::TogglePin),
            (TrayCommand::ToggleSound, Command::ToggleSound),
            (TrayCommand::OpacityUp, Command::OpacityUp),
            (TrayCommand::OpacityDown, Command::OpacityDown),
            (TrayCommand::ToggleMeasure, Command::ToggleMeasure),
            (TrayCommand::ResizeImages, Command::ResizeOpen),
            (
                TrayCommand::PowerStoreUpdateAll,
                Command::PowerStoreUpdateAll,
            ),
            (TrayCommand::PowerStoreCheckNow, Command::PowerStoreCheckNow),
            (TrayCommand::OpenPowerStore, Command::PowerStoreOpen),
            (TrayCommand::OpenMediaRenamer, Command::MediaRenamerOpen),
            (TrayCommand::ToggleMouse, Command::ToggleMouse),
            (
                TrayCommand::OpenPage(Tool::AlwaysOn),
                Command::OpenAlwaysOnPage,
            ),
            (
                TrayCommand::OpenPage(Tool::AlwaysTop),
                Command::OpenAlwaysTopPage,
            ),
            (
                TrayCommand::OpenPage(Tool::ScreenRuler),
                Command::OpenScreenRulerPage,
            ),
            (
                TrayCommand::OpenPage(Tool::PowerStore),
                Command::OpenSettings,
            ),
            (TrayCommand::OpenApp, Command::OpenSettings),
            (TrayCommand::Exit, Command::Exit),
        ];
        for (command, expected) in commands {
            let id = id(Some(command));
            assert_ne!(id, 0, "{command:?}");
            assert_eq!(Command::from_id(id), Some(expected), "{command:?}");
        }
        // A report row can never be chosen as a command.
        assert_eq!(id(None), 0);
        assert_eq!(Command::from_id(0), None);
    }
}
