//! The tray menu as data: which entries exist, in what order, with which
//! labels, marks and commands. A platform turns [`Entry`] values into its
//! native menu and maps a chosen [`Command`] back to an action.
//!
//! The root holds one entry per tool the user lists, then the registered
//! shortcuts, "Open PowerKit" and Exit. A tool with several commands is a
//! submenu whose label carries its status; a tool with one command is that
//! command at the root.
use crate::controller::settings::Tool;
use powerkit_core::always_on::{Choice, PRESET_MINUTES, Session};
use powerkit_core::always_top::Pin;
use powerkit_core::locale::{RenameStrings, Strings, UiLanguage};
use powerkit_core::settings::Modules;
use powerkit_core::tray_layout::{TrayLayout, TrayTool};

/// What choosing a tray entry asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Start (or stop) AlwaysOn with this choice.
    AlwaysOn(Choice),
    /// Keep awake while the last eligible foreground window's process runs.
    BindProcess,
    Extend,
    ToggleDisplay,
    TogglePin,
    ToggleSound,
    OpacityUp,
    OpacityDown,
    ToggleMeasure,
    ResizeImages,
    PowerStoreUpdateAll,
    PowerStoreCheckNow,
    OpenPowerStore,
    OpenMediaRenamer,
    ToggleMouse,
    /// Open the settings page of this tool, where its shortcut is set.
    OpenPage(Tool),
    OpenApp,
    Exit,
}

/// A picture a platform draws before a root entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Tool(TrayTool),
    Shortcuts,
}

/// What MediaRenamer reports in its tray entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenamerStatus {
    Idle,
    Scanning,
    /// Items ready to rename.
    Ready(usize),
    Applying,
}

const OFF: Option<Command> = Some(Command::AlwaysOn(Choice::Off));
const HALF_HOUR: Option<Command> = Some(Command::AlwaysOn(Choice::Preset(PRESET_MINUTES[0])));
const HOUR: Option<Command> = Some(Command::AlwaysOn(Choice::Preset(PRESET_MINUTES[1])));
const TWO_HOURS: Option<Command> = Some(Command::AlwaysOn(Choice::Preset(PRESET_MINUTES[2])));
const CUSTOM: Option<Command> = Some(Command::AlwaysOn(Choice::CustomInterval));
const FOREVER: Option<Command> = Some(Command::AlwaysOn(Choice::Indefinite));
const BIND: Option<Command> = Some(Command::BindProcess);
const EXTEND: Option<Command> = Some(Command::Extend);
const DISPLAY: Option<Command> = Some(Command::ToggleDisplay);
const PIN: Option<Command> = Some(Command::TogglePin);
const SOUND: Option<Command> = Some(Command::ToggleSound);
const OPACITY_UP: Option<Command> = Some(Command::OpacityUp);
const OPACITY_DOWN: Option<Command> = Some(Command::OpacityDown);
const MEASURE: Option<Command> = Some(Command::ToggleMeasure);
const RESIZE: Option<Command> = Some(Command::ResizeImages);
const POWER_STORE_UPDATE_ALL: Option<Command> = Some(Command::PowerStoreUpdateAll);
const POWER_STORE_CHECK_NOW: Option<Command> = Some(Command::PowerStoreCheckNow);
const POWER_STORE_OPEN: Option<Command> = Some(Command::OpenPowerStore);
const MEDIA_RENAMER_OPEN: Option<Command> = Some(Command::OpenMediaRenamer);
const TOGGLE_MOUSE: Option<Command> = Some(Command::ToggleMouse);
const OPEN: Option<Command> = Some(Command::OpenApp);
const EXIT: Option<Command> = Some(Command::Exit);
/// A row that only reports something and never runs a command.
const REPORT: Option<Command> = None;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Item {
        /// None for a row that only reports something.
        command: Option<Command>,
        label: String,
        mark: Mark,
        enabled: bool,
        /// Drawn before the label; root tool entries only.
        icon: Option<Icon>,
        /// Shortcut shown in a tooltip after the pointer rests on the entry.
        hint: Option<String>,
    },
    Separator,
    Submenu {
        label: String,
        entries: Vec<Entry>,
        icon: Option<Icon>,
        hint: Option<String>,
    },
}
impl Entry {
    fn with_icon(mut self, picture: Icon) -> Self {
        if let Self::Item { icon, .. } | Self::Submenu { icon, .. } = &mut self {
            *icon = Some(picture);
        }
        self
    }
    fn with_hint(mut self, key: Option<&str>) -> Self {
        if let Self::Item { hint, .. } | Self::Submenu { hint, .. } = &mut self {
            *hint = key.map(str::to_owned);
        }
        self
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    None,
    Check(bool),
    Radio(bool),
}
fn always_top_root_label(pin: &Pin, t: &Strings) -> String {
    match t.always_top_status(pin) {
        Some(status) => format!("{} ({status})", t.tray_always_top_on),
        None => t.tray_always_top_off.into(),
    }
}
fn item(command: Option<Command>, label: impl Into<String>, mark: Mark, enabled: bool) -> Entry {
    Entry::Item {
        command,
        label: label.into(),
        mark,
        enabled,
        icon: None,
        hint: None,
    }
}
fn submenu(label: impl Into<String>, entries: Vec<Entry>) -> Entry {
    Entry::Submenu {
        label: label.into(),
        entries,
        icon: None,
        hint: None,
    }
}
fn radio_item(command: Option<Command>, label: &str, selected: Choice) -> Entry {
    let on = command == Some(Command::AlwaysOn(selected));
    item(command, label, Mark::Radio(on), true)
}
pub struct AlwaysOnMenu<'a> {
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
fn always_on_entries(menu: &AlwaysOnMenu<'_>, t: &Strings) -> Vec<Entry> {
    let radio = Choice::of(menu.session);
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
    entries.push(item(
        BIND,
        bind,
        Mark::Radio(radio == Choice::Process),
        true,
    ));
    // An expiry needs a picked time, so the tray only reports the running one.
    if let Some(end) = menu.expiry {
        entries.push(item(
            REPORT,
            format!("{} {end}", t.expiry_until),
            Mark::Radio(radio == Choice::Expiry),
            false,
        ));
    }
    entries.push(Entry::Separator);
    if menu.session.deadline().is_some() {
        entries.push(item(EXTEND, t.tray_extend, Mark::None, true));
    }
    entries.push(item(
        DISPLAY,
        t.tray_display,
        Mark::Check(menu.display),
        true,
    ));
    entries
}
pub struct AlwaysTopMenu<'a> {
    pub pin: &'a Pin,
    pub sound: bool,
}
#[derive(Clone, Copy)]
pub struct ScreenRulerMenu {
    pub active: bool,
}
pub struct PowerStoreMenu {
    /// Updates after ignore/skip rules; `None` before the first check.
    pub updates: Option<usize>,
    pub checking: bool,
}
fn power_store_entry(menu: &PowerStoreMenu, t: &Strings) -> Entry {
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
            item(REPORT, status, Mark::None, false),
            Entry::Separator,
            item(
                POWER_STORE_UPDATE_ALL,
                t.power_store_update_all,
                Mark::None,
                menu.updates.is_some_and(|count| count > 0),
            ),
            item(
                POWER_STORE_CHECK_NOW,
                t.power_store_check_now,
                Mark::None,
                !menu.checking,
            ),
            item(POWER_STORE_OPEN, t.power_store_open, Mark::None, true),
        ],
    )
}
fn media_renamer_entry(status: RenamerStatus, t: &Strings) -> Entry {
    let r = t.rename;
    let (label, status) = match status {
        RenamerStatus::Idle => (r.nav.to_string(), r.tray_idle.to_string()),
        RenamerStatus::Scanning => (r.nav.to_string(), r.tray_scanning.to_string()),
        RenamerStatus::Ready(count) => (
            format!("{} ({count})", r.nav),
            RenameStrings::count(r.tray_ready, count),
        ),
        RenamerStatus::Applying => (r.nav.to_string(), r.tray_applying.to_string()),
    };
    submenu(
        label,
        vec![
            item(REPORT, status, Mark::None, false),
            Entry::Separator,
            item(MEDIA_RENAMER_OPEN, r.tray_open, Mark::None, true),
        ],
    )
}
/// Registered global shortcuts; `None` when a shortcut is not registered.
#[derive(Clone, Copy, Default)]
pub struct Shortcuts<'a> {
    pub always_on: Option<&'a str>,
    pub pin: Option<&'a str>,
    pub opacity_up: Option<&'a str>,
    pub opacity_down: Option<&'a str>,
    pub ruler: Option<&'a str>,
}
/// Lists each registered shortcut of an enabled tool as "Tool: action⇥keys";
/// choosing one opens the page where it is set. `None` when Settings hides the
/// submenu or there is nothing to list.
fn shortcuts_entry(menus: &Menus<'_>, t: &Strings) -> Option<Entry> {
    if !menus.layout.shortcuts() {
        return None;
    }
    let keys = &menus.shortcuts;
    let modules = menus.modules;
    let rows = [
        (
            modules.always_on,
            Command::OpenPage(Tool::AlwaysOn),
            t.nav_always_on,
            t.shortcut_always_on,
            keys.always_on,
        ),
        (
            modules.always_top,
            Command::OpenPage(Tool::AlwaysTop),
            t.nav_always_top,
            t.shortcut_pin,
            keys.pin,
        ),
        (
            modules.always_top,
            Command::OpenPage(Tool::AlwaysTop),
            t.nav_always_top,
            t.hotkey_op_up,
            keys.opacity_up,
        ),
        (
            modules.always_top,
            Command::OpenPage(Tool::AlwaysTop),
            t.nav_always_top,
            t.hotkey_op_down,
            keys.opacity_down,
        ),
        (
            modules.screen_ruler,
            Command::OpenPage(Tool::ScreenRuler),
            t.nav_screen_ruler,
            t.shortcut_ruler,
            keys.ruler,
        ),
    ];
    let entries: Vec<Entry> = rows
        .into_iter()
        .filter(|(enabled, ..)| *enabled)
        .filter_map(|(_, command, tool, action, key)| {
            key.map(|key| {
                item(
                    Some(command),
                    format!("{tool}: {action}\t{key}"),
                    Mark::None,
                    true,
                )
            })
        })
        .collect();
    (!entries.is_empty()).then(|| submenu(t.section_shortcuts, entries).with_icon(Icon::Shortcuts))
}
pub struct Menus<'a> {
    pub always_on: AlwaysOnMenu<'a>,
    pub always_top: AlwaysTopMenu<'a>,
    pub ruler: ScreenRulerMenu,
    pub power_store: PowerStoreMenu,
    pub media_renamer: RenamerStatus,
    /// Whether PowerMouse is remapping; its tray entry is the on/off switch.
    pub mouse: bool,
    pub shortcuts: Shortcuts<'a>,
    pub modules: Modules,
    /// User-chosen order and visibility of the tool entries.
    pub layout: TrayLayout,
    pub lang: UiLanguage,
}
fn module_enabled(tool: TrayTool, modules: Modules) -> bool {
    match tool {
        TrayTool::AlwaysOn => modules.always_on,
        TrayTool::AlwaysTop => modules.always_top,
        TrayTool::ScreenRuler => modules.screen_ruler,
        TrayTool::ImageResizer => modules.image_resizer,
        TrayTool::PowerStore => modules.power_store,
        TrayTool::MediaRenamer => modules.media_renamer,
        // PowerMouse has no separate Home switch: its tray entry is the
        // switch itself, so it stays listed while PowerMouse is off.
        TrayTool::PowerMouse => true,
    }
}
/// A tool with several commands gets a status-bearing submenu; a tool with one
/// command (ScreenRuler, ImageResizer) is that command, directly at the root.
/// Entries a global shortcut also triggers carry its keys as a hover hint.
fn tool_entry(tool: TrayTool, menus: &Menus<'_>, t: &Strings) -> Entry {
    let keys = &menus.shortcuts;
    match tool {
        TrayTool::AlwaysOn => submenu(
            menus.always_on.status,
            always_on_entries(&menus.always_on, t),
        )
        .with_hint(keys.always_on),
        TrayTool::AlwaysTop => {
            let always_top = &menus.always_top;
            let pinned = always_top.pin.active();
            submenu(
                always_top_root_label(always_top.pin, t),
                vec![
                    item(PIN, t.tray_pin, Mark::Check(pinned), true).with_hint(keys.pin),
                    item(SOUND, t.sound, Mark::Check(always_top.sound), true),
                    Entry::Separator,
                    item(OPACITY_UP, t.hotkey_op_up, Mark::None, pinned).with_hint(keys.opacity_up),
                    item(OPACITY_DOWN, t.hotkey_op_down, Mark::None, pinned)
                        .with_hint(keys.opacity_down),
                ],
            )
        }
        TrayTool::ScreenRuler => {
            let status = if menus.ruler.active {
                t.ruler_on
            } else {
                t.ruler_off
            };
            item(MEASURE, status, Mark::Check(menus.ruler.active), true).with_hint(keys.ruler)
        }
        TrayTool::ImageResizer => item(RESIZE, t.tray_resize, Mark::None, true),
        TrayTool::PowerStore => power_store_entry(&menus.power_store, t),
        TrayTool::MediaRenamer => media_renamer_entry(menus.media_renamer, t),
        TrayTool::PowerMouse => item(TOGGLE_MOUSE, t.mouse.title, Mark::Check(menus.mouse), true),
    }
}
/// The whole tray menu, root first.
pub fn entries(menus: Menus<'_>) -> Vec<Entry> {
    let t = menus.lang.strings();
    let mut entries: Vec<Entry> = menus
        .layout
        .items()
        .iter()
        .filter(|item| item.shown && module_enabled(item.tool, menus.modules))
        .map(|item| tool_entry(item.tool, &menus, t).with_icon(Icon::Tool(item.tool)))
        .collect();
    if !entries.is_empty() {
        entries.push(Entry::Separator);
    }
    entries.extend(shortcuts_entry(&menus, t));
    entries.push(item(OPEN, t.open_app, Mark::None, true));
    entries.push(item(EXIT, t.exit, Mark::None, true));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root_commands(entries: &[Entry]) -> Vec<Option<Command>> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Item { command, .. } => Some(*command),
                _ => None,
            })
            .collect()
    }
    /// The default layout with every tool listed, so tests see all six entries.
    fn all_shown() -> TrayLayout {
        let mut layout = TrayLayout::default();
        for tool in TrayTool::ALL {
            if !layout.shown(tool) {
                layout.apply(powerkit_core::tray_layout::TrayEdit::Toggle(tool));
            }
        }
        layout
    }
    fn always_on(entries: &[Entry]) -> &[Entry] {
        match &entries[1] {
            Entry::Submenu { entries, .. } => entries,
            other => panic!("{other:?}"),
        }
    }
    fn always_top(entries: &[Entry]) -> &Entry {
        &entries[2]
    }
    fn marks(entries: &[Entry]) -> Vec<(Option<Command>, Mark)> {
        always_on(entries)
            .iter()
            .filter_map(|entry| match entry {
                Entry::Item { command, mark, .. } => Some((*command, *mark)),
                _ => None,
            })
            .collect()
    }
    fn radios(entries: &[Entry]) -> Vec<(Option<Command>, bool)> {
        marks(entries)
            .into_iter()
            .filter_map(|(id, mark)| match mark {
                Mark::Radio(on) => Some((id, on)),
                _ => None,
            })
            .collect()
    }
    fn menu_for(session: Session, display: bool, status: &str, expiry: Option<&str>) -> Vec<Entry> {
        entries(Menus {
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
                sound: true,
            },
            ruler: ScreenRulerMenu { active: false },
            power_store: PowerStoreMenu {
                updates: None,
                checking: false,
            },
            media_renamer: RenamerStatus::Idle,
            mouse: false,
            shortcuts: Shortcuts::default(),
            modules: Modules::default(),
            layout: all_shown(),
            lang: UiLanguage::Turkish,
        })
    }
    fn always_top_menu(pin: &Pin, hotkey: Option<&str>, sound: bool) -> Vec<Entry> {
        entries(Menus {
            always_on: AlwaysOnMenu {
                session: Session::Off,
                display: false,
                interval_minutes: 60,
                status: "",
                expiry: None,
                bound_app: None,
            },
            always_top: AlwaysTopMenu { pin, sound },
            ruler: ScreenRulerMenu { active: false },
            power_store: PowerStoreMenu {
                updates: None,
                checking: false,
            },
            media_renamer: RenamerStatus::Ready(2),
            mouse: true,
            shortcuts: Shortcuts {
                pin: hotkey,
                ..Shortcuts::default()
            },
            modules: Modules::default(),
            layout: all_shown(),
            lang: UiLanguage::Turkish,
        })
    }
    #[test]
    fn root_keeps_powerkit_actions_and_always_on_status() {
        let t = UiLanguage::Turkish.strings();
        let off = menu_for(Session::Off, false, t.tray_always_on_off, None);
        let timed = menu_for(
            Session::interval(0, 30, false),
            true,
            "AlwaysOn açık (23 dk kaldı)",
            None,
        );
        assert!(matches!(
            &off[0],
            Entry::Submenu { label, .. } if label == t.nav_power_store
        ));
        assert!(matches!(
            &off[1],
            Entry::Submenu { label, .. } if label == t.tray_always_on_off
        ));
        assert!(matches!(
            &timed[1],
            Entry::Submenu { label, .. } if label == "AlwaysOn açık (23 dk kaldı)"
        ));
        assert!(matches!(
            always_top(&off),
            Entry::Submenu { label, .. } if label == t.tray_always_top_off
        ));
        // One-command tools are that command at the root, not a submenu.
        assert!(matches!(
            &off[3],
            Entry::Item { command: MEASURE, label, mark: Mark::Check(false), enabled: true, .. }
                if label == t.ruler_off
        ));
        assert!(matches!(
            &off[4],
            Entry::Submenu { label, .. } if label == t.rename.nav
        ));
        assert!(matches!(
            &off[5],
            Entry::Item { command: RESIZE, label, mark: Mark::None, enabled: true, .. }
                if label == t.tray_resize
        ));
        // PowerMouse is its own on/off switch and stays listed while off.
        assert!(matches!(
            &off[6],
            Entry::Item { command: TOGGLE_MOUSE, label, mark: Mark::Check(false), enabled: true, .. }
                if label == "PowerMouse"
        ));
        assert!(matches!(off[7], Entry::Separator));
        assert!(matches!(
            &off[8],
            Entry::Item { command: OPEN, label, icon: None, .. } if label == "PowerKit’i aç"
        ));
        assert_eq!(
            root_commands(&off),
            [MEASURE, RESIZE, TOGGLE_MOUSE, OPEN, EXIT]
        );
        assert_eq!(
            root_commands(&timed),
            [MEASURE, RESIZE, TOGGLE_MOUSE, OPEN, EXIT]
        );
    }
    fn laid_out(layout: TrayLayout, modules: Modules, ruler: ScreenRulerMenu) -> Vec<Entry> {
        entries(Menus {
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
                sound: true,
            },
            ruler,
            power_store: PowerStoreMenu {
                updates: None,
                checking: false,
            },
            media_renamer: RenamerStatus::Idle,
            mouse: false,
            shortcuts: Shortcuts::default(),
            modules,
            layout,
            lang: UiLanguage::English,
        })
    }
    fn root_glyphs(entries: &[Entry]) -> Vec<Option<Icon>> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Item { icon, .. } | Entry::Submenu { icon, .. } => Some(*icon),
                Entry::Separator => None,
            })
            .collect()
    }
    #[test]
    fn layout_orders_and_hides_tools_and_every_tool_carries_its_icon() {
        use powerkit_core::tray_layout::TrayEdit;
        let idle = ScreenRulerMenu { active: false };
        let modules = Modules::default();
        // A fresh layout leaves out the file tools.
        assert_eq!(
            root_commands(&laid_out(TrayLayout::default(), modules, idle)),
            [MEASURE, TOGGLE_MOUSE, OPEN, EXIT]
        );
        let default = laid_out(all_shown(), modules, idle);
        let glyphs = root_glyphs(&default);
        // Seven tools with distinct icons, then Open and Exit without one.
        assert!(glyphs[..7].iter().all(Option::is_some));
        for (index, icon) in glyphs[..7].iter().enumerate() {
            assert!(!glyphs[..index].contains(icon), "{icon:?}");
        }
        assert_eq!(glyphs[7..], [None, None]);
        // Submenu children never carry icons.
        assert!(default.iter().all(|entry| match entry {
            Entry::Submenu { entries, .. } => root_glyphs(entries).iter().all(Option::is_none),
            _ => true,
        }));

        let mut layout = all_shown();
        for _ in 0..5 {
            layout.apply(TrayEdit::MoveUp(TrayTool::ImageResizer));
        }
        layout.apply(TrayEdit::Toggle(TrayTool::AlwaysTop));
        let active = ScreenRulerMenu { active: true };
        // PowerStore is hidden by its Home switch, AlwaysTop by the tray setting.
        let custom = laid_out(
            layout,
            Modules {
                power_store: false,
                ..modules
            },
            active,
        );
        assert!(matches!(
            custom.as_slice(),
            [
                Entry::Item { command: RESIZE, .. },
                Entry::Submenu { label: always_on, .. },
                Entry::Item { command: MEASURE, label: ruler, mark: Mark::Check(true), .. },
                Entry::Submenu { label: renamer, .. },
                Entry::Item { command: TOGGLE_MOUSE, mark: Mark::Check(false), .. },
                Entry::Separator,
                Entry::Item { command: OPEN, label: open, .. },
                Entry::Item { command: EXIT, .. },
            ] if always_on == "AlwaysOn"
                && ruler == "ScreenRuler on"
                && renamer == "Media Renamer"
                && open == "Open PowerKit"
        ));

        // Nothing to list: no leading separator before the PowerKit actions.
        let mut hidden = all_shown();
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
    fn shortcuts_submenu_lists_registered_keys_of_enabled_tools() {
        let t = UiLanguage::English.strings();
        let menus = |modules, shortcuts| Menus {
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
                sound: true,
            },
            ruler: ScreenRulerMenu { active: false },
            power_store: PowerStoreMenu {
                updates: None,
                checking: false,
            },
            media_renamer: RenamerStatus::Idle,
            mouse: false,
            shortcuts,
            modules,
            layout: TrayLayout::default(),
            lang: UiLanguage::English,
        };
        let keys = Shortcuts {
            always_on: Some("Win+Ctrl+Shift+A"),
            pin: Some("Win+Ctrl+T"),
            opacity_up: None,
            opacity_down: Some("Win+Ctrl+-"),
            ruler: Some("Win+Ctrl+Shift+M"),
        };
        let modules = Modules::default();
        let rows = |entry: Option<Entry>| match entry {
            Some(Entry::Submenu {
                label,
                entries,
                icon,
                ..
            }) => {
                assert_eq!(label, t.section_shortcuts);
                assert_eq!(icon, Some(Icon::Shortcuts));
                entries
                    .into_iter()
                    .map(|entry| match entry {
                        Entry::Item { command, label, .. } => (command, label),
                        other => panic!("{other:?}"),
                    })
                    .collect::<Vec<_>>()
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(
            rows(shortcuts_entry(&menus(modules, keys), t)),
            [
                (
                    Some(Command::OpenPage(Tool::AlwaysOn)),
                    format!("{}: Turn on/off\tWin+Ctrl+Shift+A", t.nav_always_on)
                ),
                (
                    Some(Command::OpenPage(Tool::AlwaysTop)),
                    format!("{}: Pin/unpin window\tWin+Ctrl+T", t.nav_always_top)
                ),
                (
                    Some(Command::OpenPage(Tool::AlwaysTop)),
                    format!("{}: {}\tWin+Ctrl+-", t.nav_always_top, t.hotkey_op_down)
                ),
                (
                    Some(Command::OpenPage(Tool::ScreenRuler)),
                    format!(
                        "{}: Start/stop measuring\tWin+Ctrl+Shift+M",
                        t.nav_screen_ruler
                    )
                ),
            ]
        );
        // A tool switched off on Home drops its shortcuts.
        let only_ruler = Modules {
            always_on: false,
            always_top: false,
            ..modules
        };
        assert_eq!(rows(shortcuts_entry(&menus(only_ruler, keys), t)).len(), 1);
        // Nothing registered: no submenu, and the root ends with Open and Exit.
        assert!(shortcuts_entry(&menus(modules, Shortcuts::default()), t).is_none());
        // With keys, the submenu sits between the tools and Open.
        let root = entries(menus(modules, keys));
        let open = root
            .iter()
            .position(|entry| matches!(entry, Entry::Item { command: OPEN, .. }))
            .unwrap();
        assert!(
            matches!(&root[open - 1], Entry::Submenu { label, .. } if label == t.section_shortcuts)
        );
        assert!(matches!(root[open - 2], Entry::Separator));
        // Tool entries a shortcut also triggers carry it as a hover hint.
        let hint_of = |wanted: &str| {
            root.iter().find_map(|entry| match entry {
                Entry::Item { label, hint, .. } | Entry::Submenu { label, hint, .. }
                    if label == wanted =>
                {
                    Some(hint.clone())
                }
                _ => None,
            })
        };
        assert_eq!(hint_of("AlwaysOn"), Some(Some("Win+Ctrl+Shift+A".into())));
        assert_eq!(hint_of(t.ruler_off), Some(Some("Win+Ctrl+Shift+M".into())));
        // Settings can hide the submenu; the shortcuts themselves stay.
        let mut hidden = menus(modules, keys);
        hidden
            .layout
            .apply(powerkit_core::tray_layout::TrayEdit::ToggleShortcuts);
        assert!(shortcuts_entry(&hidden, t).is_none());
    }
    #[test]
    fn power_store_shows_update_count_and_enables_update_all_only_with_updates() {
        let t = UiLanguage::Turkish.strings();
        let entry = |updates, checking| power_store_entry(&PowerStoreMenu { updates, checking }, t);
        let enabled = |entry: &Entry, wanted: Option<Command>| match entry {
            Entry::Submenu { entries, .. } => entries.iter().find_map(|entry| match entry {
                Entry::Item {
                    command, enabled, ..
                } if *command == wanted => Some(*enabled),
                _ => None,
            }),
            _ => None,
        };
        let pending = entry(Some(3), false);
        assert!(matches!(&pending, Entry::Submenu { label, .. } if label == "Power Store (3)"));
        assert_eq!(enabled(&pending, POWER_STORE_UPDATE_ALL), Some(true));
        assert_eq!(enabled(&pending, POWER_STORE_OPEN), Some(true));
        let current = entry(Some(0), true);
        assert!(matches!(&current, Entry::Submenu { label, .. } if label == t.nav_power_store));
        assert_eq!(enabled(&current, POWER_STORE_UPDATE_ALL), Some(false));
        assert_eq!(enabled(&current, POWER_STORE_CHECK_NOW), Some(false));
        assert_eq!(
            enabled(&entry(None, false), POWER_STORE_UPDATE_ALL),
            Some(false)
        );
    }
    #[test]
    fn media_renamer_reports_ready_items_and_opens_the_workspace() {
        let t = UiLanguage::English.strings();
        let ready = media_renamer_entry(RenamerStatus::Ready(4), t);
        assert!(matches!(
            &ready,
            Entry::Submenu { label, entries, .. } if label == "Media Renamer (4)"
                && matches!(
                    entries.as_slice(),
                    [
                        Entry::Item { command: REPORT, label, enabled: false, .. },
                        Entry::Separator,
                        Entry::Item { command: MEDIA_RENAMER_OPEN, enabled: true, .. },
                    ] if label == "4 items ready to rename"
                )
        ));
        assert!(matches!(
            media_renamer_entry(RenamerStatus::Applying, t),
            Entry::Submenu { label, .. } if label == "Media Renamer"
        ));
        let always_top = always_top_menu(&Pin::Off, None, true);
        assert!(matches!(
            &always_top[4],
            Entry::Submenu { label, .. } if label == "Media Renamer (2)"
        ));
    }
    #[test]
    fn always_top_uses_status_label_and_pin_check() {
        let off = always_top_menu(&Pin::Off, Some("Win+Ctrl+T"), true);
        let on = always_top_menu(&Pin::on("Notepad"), None, false);
        let t = UiLanguage::Turkish.strings();
        assert!(matches!(
            always_top(&off),
            Entry::Submenu {
                label,
                entries,
                ..
            } if label == t.tray_always_top_off
                && matches!(
                    entries.as_slice(),
                    [
                        Entry::Item { command: PIN, label, mark: Mark::Check(false), enabled: true, hint: Some(hint), .. },
                        Entry::Item { command: SOUND, mark: Mark::Check(true), enabled: true, hint: None, .. },
                        Entry::Separator,
                        Entry::Item { command: OPACITY_UP, enabled: false, .. },
                        Entry::Item { command: OPACITY_DOWN, enabled: false, .. },
                    ]
                        // The key moved from the label into the hover hint.
                        if label == "Pencereyi üstte tut" && hint == "Win+Ctrl+T"
                )
        ));
        assert!(matches!(
            always_top(&on),
            Entry::Submenu {
                label,
                entries,
                ..
            } if label == "AlwaysTop açık (Notepad)"
                && matches!(
                    entries.as_slice(),
                    [
                        Entry::Item { command: PIN, mark: Mark::Check(true), enabled: true, hint: None, .. },
                        Entry::Item { command: SOUND, mark: Mark::Check(false), enabled: true, .. },
                        Entry::Separator,
                        Entry::Item { command: OPACITY_UP, enabled: true, .. },
                        Entry::Item { command: OPACITY_DOWN, enabled: true, .. },
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
                (OFF, Mark::Radio(true)),
                (HALF_HOUR, Mark::Radio(false)),
                (HOUR, Mark::Radio(false)),
                (TWO_HOURS, Mark::Radio(false)),
                (CUSTOM, Mark::Radio(false)),
                (FOREVER, Mark::Radio(false)),
                (BIND, Mark::Radio(false)),
                (DISPLAY, Mark::Check(false)),
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
            Some((DISPLAY, Mark::Check(true)))
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
            Entry::Item { command: CUSTOM, label, .. } if label == "Özel süre açık tut (3 sa 15 dk)"
        )));
        assert!(!marks(&off).iter().any(|(id, _)| *id == REPORT));
        let expiry = menu_for(
            Session::Expiry {
                end: 10,
                target: powerkit_core::primitives::UtcTicks(10),
            },
            false,
            "",
            Some("18:30"),
        );
        assert!(always_on(&expiry).iter().any(|entry| matches!(
            entry,
            Entry::Item { command: REPORT, label, mark: Mark::Radio(true), enabled: false, .. }
                if label == "bitiş 18:30"
        )));
        assert_eq!(radios(&expiry).into_iter().filter(|(_, on)| *on).count(), 1);
    }
    #[test]
    fn extend_appears_only_for_deadlines_and_binding_names_the_app() {
        let has_extend = |entries: &[Entry]| marks(entries).iter().any(|(id, _)| *id == EXTEND);
        assert!(!has_extend(&menu_for(Session::Off, false, "", None)));
        assert!(!has_extend(&menu_for(Session::Indefinite, false, "", None)));
        assert!(has_extend(&menu_for(
            Session::interval(0, 30, false),
            false,
            "",
            None
        )));
        assert!(has_extend(&menu_for(
            Session::Expiry {
                end: 9,
                target: powerkit_core::primitives::UtcTicks(9),
            },
            false,
            "",
            Some("18:30")
        )));
        let bound = menu_for(Session::Process { pid: 7 }, false, "", None);
        assert!(!has_extend(&bound));
        assert!(always_on(&bound).iter().any(|entry| matches!(
            entry,
            Entry::Item { command: BIND, label, mark: Mark::Radio(true), enabled: true, .. }
                if label == "Uygulama çalıştıkça açık tut (vlc)"
        )));
    }
}
