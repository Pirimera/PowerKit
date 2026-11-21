//! App-grouped native tray menu: PowerKit actions at the root, tools in status-bearing submenus.
use super::{
    command::{
        AppCommand, BIND, CUSTOM, DISPLAY, EXIT, EXPIRY, EXTEND, FOREVER, HALF_HOUR, HOUR, MEASURE,
        MEDIA_RENAMER_OPEN, OFF, OPACITY_DOWN, OPACITY_UP, OPEN, PIN, POWER_STORE_CHECK_NOW,
        POWER_STORE_OPEN, POWER_STORE_UPDATE_ALL, RESIZE, SOUND, TWO_HOURS,
    },
    media_renamer::TrayStatus,
    menu_icon::MenuIcons,
    theme, wide,
};
use crate::always_on::Session;
use crate::always_top::Pin;
use crate::locale::RenameStrings;
use crate::tray_layout::{TrayLayout, TrayTool};
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TrayEntry {
    Item {
        id: usize,
        label: String,
        mark: TrayMark,
        enabled: bool,
        /// Segoe Fluent glyph drawn before the label; root tool entries only.
        glyph: Option<&'static str>,
    },
    Separator,
    Submenu {
        label: String,
        entries: Vec<TrayEntry>,
        glyph: Option<&'static str>,
    },
}
impl TrayEntry {
    fn with_glyph(mut self, icon: &'static str) -> Self {
        if let Self::Item { glyph, .. } | Self::Submenu { glyph, .. } = &mut self {
            *glyph = Some(icon);
        }
        self
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrayMark {
    None,
    Check(bool),
    Radio(bool),
}
fn always_top_root_label(pin: &Pin, t: &crate::locale::Strings) -> String {
    match t.always_top_status(pin) {
        Some(status) => format!("{} ({status})", t.tray_always_top_on),
        None => t.tray_always_top_off.into(),
    }
}
fn item(id: usize, label: impl Into<String>, mark: TrayMark, enabled: bool) -> TrayEntry {
    TrayEntry::Item {
        id,
        label: label.into(),
        mark,
        enabled,
        glyph: None,
    }
}
fn submenu(label: impl Into<String>, entries: Vec<TrayEntry>) -> TrayEntry {
    TrayEntry::Submenu {
        label: label.into(),
        entries,
        glyph: None,
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
#[derive(Clone, Copy)]
pub(super) struct ScreenRulerMenu {
    pub active: bool,
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
    submenu(
        label,
        vec![
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
    )
}
fn media_renamer_entry(status: TrayStatus, t: &crate::locale::Strings) -> TrayEntry {
    let r = t.rename;
    let (label, status) = match status {
        TrayStatus::Idle => (r.nav.to_string(), r.tray_idle.to_string()),
        TrayStatus::Scanning => (r.nav.to_string(), r.tray_scanning.to_string()),
        TrayStatus::Ready(count) => (
            format!("{} ({count})", r.nav),
            RenameStrings::count(r.tray_ready, count),
        ),
        TrayStatus::Applying => (r.nav.to_string(), r.tray_applying.to_string()),
    };
    submenu(
        label,
        vec![
            item(POWER_STORE_STATUS, status, TrayMark::None, false),
            TrayEntry::Separator,
            item(MEDIA_RENAMER_OPEN, r.tray_open, TrayMark::None, true),
        ],
    )
}
/// Disabled status rows need an id that never maps to a command.
const POWER_STORE_STATUS: usize = 0;
pub(super) struct TrayMenus<'a> {
    pub always_on: AlwaysOnMenu<'a>,
    pub always_top: AlwaysTopMenu<'a>,
    pub ruler: ScreenRulerMenu,
    pub power_store: PowerStoreMenu,
    pub media_renamer: TrayStatus,
    pub modules: crate::settings::Modules,
    /// User-chosen order and visibility of the tool entries.
    pub layout: TrayLayout,
    pub lang: crate::locale::UiLanguage,
}
fn module_enabled(tool: TrayTool, modules: crate::settings::Modules) -> bool {
    match tool {
        TrayTool::AlwaysOn => modules.always_on,
        TrayTool::AlwaysTop => modules.always_top,
        TrayTool::ScreenRuler => modules.screen_ruler,
        TrayTool::ImageResizer => modules.image_resizer,
        TrayTool::PowerStore => modules.power_store,
        TrayTool::MediaRenamer => modules.media_renamer,
    }
}
/// A tool with several commands gets a status-bearing submenu; a tool with one
/// command (ScreenRuler, ImageResizer) is that command, directly at the root.
fn tool_entry(tool: TrayTool, menus: &TrayMenus<'_>, t: &crate::locale::Strings) -> TrayEntry {
    match tool {
        TrayTool::AlwaysOn => submenu(
            menus.always_on.status,
            always_on_entries(&menus.always_on, t),
        ),
        TrayTool::AlwaysTop => {
            let always_top = &menus.always_top;
            let pinned = always_top.pin.active();
            submenu(
                always_top_root_label(always_top.pin, t),
                vec![
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
            )
        }
        TrayTool::ScreenRuler => {
            let status = if menus.ruler.active {
                t.ruler_on
            } else {
                t.ruler_off
            };
            item(MEASURE, status, TrayMark::Check(menus.ruler.active), true)
        }
        TrayTool::ImageResizer => item(RESIZE, t.tray_resize, TrayMark::None, true),
        TrayTool::PowerStore => power_store_entry(&menus.power_store, t),
        TrayTool::MediaRenamer => media_renamer_entry(menus.media_renamer, t),
    }
}
pub(super) fn tray_entries(menus: TrayMenus<'_>) -> Vec<TrayEntry> {
    let t = menus.lang.strings();
    let mut entries: Vec<TrayEntry> = menus
        .layout
        .items()
        .iter()
        .filter(|item| item.shown && module_enabled(item.tool, menus.modules))
        .map(|item| {
            tool_entry(item.tool, &menus, t).with_glyph(super::window::tray_page(item.tool).glyph())
        })
        .collect();
    if !entries.is_empty() {
        entries.push(TrayEntry::Separator);
    }
    entries.push(item(OPEN, t.open_app, TrayMark::None, true));
    entries.push(item(EXIT, t.exit, TrayMark::None, true));
    entries
}
fn append(menu: HMENU, entry: &TrayEntry, icons: &mut MenuIcons) -> bool {
    match entry {
        TrayEntry::Separator => unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) != 0 },
        TrayEntry::Item {
            id,
            label,
            mark,
            enabled,
            glyph,
        } => {
            let mut text = wide(label);
            let checked = matches!(mark, TrayMark::Check(true) | TrayMark::Radio(true));
            let bitmap = icons.bitmap(*glyph);
            let info = MENUITEMINFOW {
                cbSize: size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_FTYPE
                    | MIIM_STRING
                    | MIIM_ID
                    | MIIM_STATE
                    | if bitmap.is_some() { MIIM_BITMAP } else { 0 },
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
                hbmpItem: bitmap.unwrap_or(null_mut()),
                ..Default::default()
            };
            // SAFETY: text is a live NUL-terminated buffer; menu is the popup being built.
            // The bitmap stays owned by `icons`, which outlives the popup.
            unsafe { InsertMenuItemW(menu, GetMenuItemCount(menu) as u32, 1, &info) != 0 }
        }
        TrayEntry::Submenu {
            label,
            entries,
            glyph,
        } => {
            let sub = new_popup();
            if sub.is_null() {
                return false;
            }
            for child in entries {
                if !append(sub, child, icons) {
                    unsafe {
                        DestroyMenu(sub);
                    }
                    return false;
                }
            }
            let mut text = wide(label);
            let bitmap = icons.bitmap(*glyph);
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
pub(super) fn track(hwnd: HWND, entries: &[TrayEntry]) -> Option<usize> {
    // Declared before the menu so the item bitmaps are freed after DestroyMenu.
    let mut icons = MenuIcons::new(hwnd, theme::popup_text_color());
    // SAFETY: popup and any submenu live until TrackPopupMenu returns; then DestroyMenu.
    unsafe {
        let menu = new_popup();
        if menu.is_null() || !entries.iter().all(|entry| append(menu, entry, &mut icons)) {
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
        tray_entries(TrayMenus {
            always_on: AlwaysOnMenu {
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
            always_top: AlwaysTopMenu {
                pin: &Pin::Off,
                hotkey: None,
                sound: true,
                opacity_up: Some("Win+Ctrl++"),
                opacity_down: Some("Win+Ctrl+-"),
            },
            ruler: ScreenRulerMenu { active: false },
            power_store: PowerStoreMenu {
                updates: None,
                checking: false,
            },
            media_renamer: TrayStatus::Idle,
            modules: crate::settings::Modules::default(),
            layout: TrayLayout::default(),
            lang: crate::locale::UiLanguage::Turkish,
        })
    }
    fn always_top_menu(pin: &Pin, hotkey: Option<&str>, sound: bool) -> Vec<TrayEntry> {
        tray_entries(TrayMenus {
            always_on: AlwaysOnMenu {
                session: Session::Off,
                display: false,
                interval_minutes: 60,
                status: "",
                expiry: None,
                bound_app: None,
            },
            always_top: AlwaysTopMenu {
                pin,
                hotkey,
                sound,
                opacity_up: Some("Win+Ctrl++"),
                opacity_down: Some("Win+Ctrl+-"),
            },
            ruler: ScreenRulerMenu { active: false },
            power_store: PowerStoreMenu {
                updates: None,
                checking: false,
            },
            media_renamer: TrayStatus::Ready(2),
            modules: crate::settings::Modules::default(),
            layout: TrayLayout::default(),
            lang: crate::locale::UiLanguage::Turkish,
        })
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
        // One-command tools are that command at the root, not a submenu.
        assert!(matches!(
            &off[2],
            TrayEntry::Item { id: MEASURE, label, mark: TrayMark::Check(false), enabled: true, .. }
                if label == t.ruler_off
        ));
        assert!(matches!(
            &off[3],
            TrayEntry::Item { id: RESIZE, label, mark: TrayMark::None, enabled: true, .. }
                if label == t.tray_resize
        ));
        assert!(matches!(
            &off[4],
            TrayEntry::Submenu { label, .. } if label == t.nav_power_store
        ));
        assert!(matches!(
            &off[5],
            TrayEntry::Submenu { label, .. } if label == t.rename.nav
        ));
        assert!(matches!(off[6], TrayEntry::Separator));
        assert!(matches!(
            &off[7],
            TrayEntry::Item { id: OPEN, label, glyph: None, .. } if label == "PowerKit’i aç"
        ));
        assert_eq!(root_commands(&off), [MEASURE, RESIZE, OPEN, EXIT]);
        assert_eq!(root_commands(&timed), [MEASURE, RESIZE, OPEN, EXIT]);
    }
    fn laid_out(
        layout: TrayLayout,
        modules: crate::settings::Modules,
        ruler: ScreenRulerMenu,
    ) -> Vec<TrayEntry> {
        tray_entries(TrayMenus {
            always_on: AlwaysOnMenu {
                session: Session::Off,
                display: false,
                interval_minutes: 60,
                status: "AlwaysOn",
                expiry: None,
                bound_app: None,
            },
            always_top: AlwaysTopMenu {
                pin: &Pin::Off,
                hotkey: None,
                sound: true,
                opacity_up: None,
                opacity_down: None,
            },
            ruler,
            power_store: PowerStoreMenu {
                updates: None,
                checking: false,
            },
            media_renamer: TrayStatus::Idle,
            modules,
            layout,
            lang: crate::locale::UiLanguage::English,
        })
    }
    fn root_glyphs(entries: &[TrayEntry]) -> Vec<Option<&'static str>> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                TrayEntry::Item { glyph, .. } | TrayEntry::Submenu { glyph, .. } => Some(*glyph),
                TrayEntry::Separator => None,
            })
            .collect()
    }
    #[test]
    fn layout_orders_and_hides_tools_and_every_tool_carries_its_icon() {
        use crate::tray_layout::TrayEdit;
        let idle = ScreenRulerMenu { active: false };
        let modules = crate::settings::Modules::default();
        let default = laid_out(TrayLayout::default(), modules, idle);
        let glyphs = root_glyphs(&default);
        // Six tools with distinct icons, then Open and Exit without one.
        assert!(glyphs[..6].iter().all(Option::is_some));
        let mut distinct = glyphs[..6].to_vec();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), 6);
        assert_eq!(glyphs[6..], [None, None]);
        // Submenu children never carry icons.
        assert!(default.iter().all(|entry| match entry {
            TrayEntry::Submenu { entries, .. } => root_glyphs(entries).iter().all(Option::is_none),
            _ => true,
        }));

        let mut layout = TrayLayout::default();
        layout.apply(TrayEdit::MoveUp(TrayTool::ImageResizer));
        layout.apply(TrayEdit::MoveUp(TrayTool::ImageResizer));
        layout.apply(TrayEdit::MoveUp(TrayTool::ImageResizer));
        layout.apply(TrayEdit::Toggle(TrayTool::AlwaysTop));
        let active = ScreenRulerMenu { active: true };
        // PowerStore is hidden by its Home switch, AlwaysTop by the tray setting.
        let custom = laid_out(
            layout,
            crate::settings::Modules {
                power_store: false,
                ..modules
            },
            active,
        );
        assert!(matches!(
            custom.as_slice(),
            [
                TrayEntry::Item { id: RESIZE, .. },
                TrayEntry::Submenu { label: always_on, .. },
                TrayEntry::Item { id: MEASURE, label: ruler, mark: TrayMark::Check(true), .. },
                TrayEntry::Submenu { label: renamer, .. },
                TrayEntry::Separator,
                TrayEntry::Item { id: OPEN, label: open, .. },
                TrayEntry::Item { id: EXIT, .. },
            ] if always_on == "AlwaysOn"
                && ruler == "ScreenRuler on"
                && renamer == "Media Renamer"
                && open == "Open PowerKit"
        ));

        // Nothing to list: no leading separator before the PowerKit actions.
        let mut hidden = TrayLayout::default();
        for tool in TrayTool::ALL {
            hidden.apply(TrayEdit::Toggle(tool));
        }
        assert_eq!(
            root_commands(&laid_out(hidden, modules, idle)),
            [OPEN, EXIT]
        );
        assert_eq!(laid_out(hidden, modules, idle).len(), 2);
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
    fn media_renamer_reports_ready_items_and_opens_the_workspace() {
        let t = crate::locale::UiLanguage::English.strings();
        let ready = media_renamer_entry(TrayStatus::Ready(4), t);
        assert!(matches!(
            &ready,
            TrayEntry::Submenu { label, entries, .. } if label == "Media Renamer (4)"
                && matches!(
                    entries.as_slice(),
                    [
                        TrayEntry::Item { id: POWER_STORE_STATUS, label, enabled: false, .. },
                        TrayEntry::Separator,
                        TrayEntry::Item { id: MEDIA_RENAMER_OPEN, enabled: true, .. },
                    ] if label == "4 items ready to rename"
                )
        ));
        assert!(matches!(
            media_renamer_entry(TrayStatus::Applying, t),
            TrayEntry::Submenu { label, .. } if label == "Media Renamer"
        ));
        let always_top = always_top_menu(&Pin::Off, None, true);
        assert!(matches!(
            &always_top[5],
            TrayEntry::Submenu { label, .. } if label == "Media Renamer (2)"
        ));
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
                entries,
                ..
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
                entries,
                ..
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
            TrayEntry::Item { id: EXPIRY, label, mark: TrayMark::Radio(true), enabled: false, .. }
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
            TrayEntry::Item { id: BIND, label, mark: TrayMark::Radio(true), enabled: true, .. }
                if label == "Uygulama çalıştıkça açık tut (vlc)"
        )));
    }
}
