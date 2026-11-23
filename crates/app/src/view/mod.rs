//! What a user interface renders: the pages, the snapshot of everything the
//! settings shell shows and the tray menu as data ([`tray`]).
//!
//! The platform shell rebuilds the [`Snapshot`] from the application state
//! after each change. A workspace's own view model (PowerStore, MediaRenamer)
//! is built by its session from state only the session sees, so it sits
//! beside that session under `controller` and is carried here by value.
pub mod tray;

use crate::command::{Command, MODE_ORDER};
use crate::intent::MouseSlot;
use powerkit_core::always_on::Session;
use powerkit_core::image_resizer::ResizeSettings;
use powerkit_core::locale::UiLanguage;
use powerkit_core::primitives::Rgb;
use powerkit_core::settings::Modules;
use powerkit_core::tray_layout::{TrayLayout, TrayTool};
/// A page of the settings shell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    General,
    AlwaysOn,
    AlwaysTop,
    ScreenRuler,
    ImageResizer,
    PowerMouse,
    PowerStore,
    MediaRenamer,
    Settings,
}
impl Page {
    pub const ALL: [Page; 9] = [
        Page::General,
        Page::PowerStore,
        Page::MediaRenamer,
        Page::AlwaysOn,
        Page::AlwaysTop,
        Page::ScreenRuler,
        Page::ImageResizer,
        Page::PowerMouse,
        Page::Settings,
    ];
    /// The tool a page belongs to; `None` for Home and Settings.
    pub fn tool(self) -> Option<crate::controller::settings::Tool> {
        use crate::controller::settings::Tool;
        Some(match self {
            Page::General | Page::Settings => return None,
            Page::AlwaysOn => Tool::AlwaysOn,
            Page::AlwaysTop => Tool::AlwaysTop,
            Page::ScreenRuler => Tool::ScreenRuler,
            Page::ImageResizer => Tool::ImageResizer,
            Page::PowerStore => Tool::PowerStore,
            Page::MediaRenamer => Tool::MediaRenamer,
            Page::PowerMouse => Tool::PowerMouse,
        })
    }
    /// Whether this rail/tool page is enabled in preferences.
    pub fn module_enabled(self, modules: Modules, mouse_on: bool) -> bool {
        match self {
            Page::General | Page::Settings => true,
            Page::AlwaysOn => modules.always_on,
            Page::AlwaysTop => modules.always_top,
            Page::ScreenRuler => modules.screen_ruler,
            Page::ImageResizer => modules.image_resizer,
            Page::PowerStore => modules.power_store,
            Page::MediaRenamer => modules.media_renamer,
            Page::PowerMouse => mouse_on,
        }
    }
    pub fn tag(self) -> &'static str {
        match self {
            Page::AlwaysOn => "always-on",
            Page::AlwaysTop => "always-top",
            Page::ScreenRuler => "screen-ruler",
            Page::ImageResizer => "image-resizer",
            Page::PowerMouse => "power-mouse",
            Page::PowerStore => "power-store",
            Page::MediaRenamer => "media-renamer",
            Page::General => "general",
            Page::Settings => "settings",
        }
    }
    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|page| page.tag() == tag)
    }
    pub fn label(self, lang: UiLanguage) -> &'static str {
        let t = lang.strings();
        match self {
            Page::AlwaysOn => t.nav_always_on,
            Page::AlwaysTop => t.nav_always_top,
            Page::ScreenRuler => t.nav_screen_ruler,
            Page::ImageResizer => t.nav_image_resizer,
            Page::PowerMouse => t.mouse.nav,
            Page::PowerStore => t.nav_power_store,
            Page::MediaRenamer => t.rename.nav,
            Page::General => t.nav_general,
            Page::Settings => t.nav_settings,
        }
    }
}
/// Settings page of a tray tool; the tray menu and the Settings list share its
/// icon and name.
pub fn tray_page(tool: TrayTool) -> Page {
    match tool {
        TrayTool::AlwaysOn => Page::AlwaysOn,
        TrayTool::AlwaysTop => Page::AlwaysTop,
        TrayTool::ScreenRuler => Page::ScreenRuler,
        TrayTool::ImageResizer => Page::ImageResizer,
        TrayTool::PowerStore => Page::PowerStore,
        TrayTool::MediaRenamer => Page::MediaRenamer,
        TrayTool::PowerMouse => Page::PowerMouse,
    }
}
/// Everything a settings UI renders, rebuilt by the platform shell from the
/// application state on each change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub status: String,
    pub mode: Option<usize>,
    pub display: bool,
    pub interval_minutes: u32,
    /// Problem text under a shortcut card, keyed by its capture command.
    pub hotkey_notes: Vec<(Command, String)>,
    pub always_on_hotkey: String,
    pub always_on_warn: bool,
    pub ac_only: bool,
    pub battery_floor: u8,
    /// Process name while a process-bound session runs.
    pub bound_app: Option<String>,
    pub top_status: String,
    pub sound: bool,
    pub capturing: Option<CaptureTarget>,
    pub pin_hotkey: String,
    pub opacity_up_hotkey: String,
    pub opacity_down_hotkey: String,
    pub frame_accent: bool,
    pub frame_color: Rgb,
    pub frame_opacity: i32,
    pub frame_thickness: i32,
    pub ruler_active: bool,
    pub ruler_hotkey: String,
    pub ruler_mode: powerkit_core::screen_ruler::Mode,
    pub ruler_unit: powerkit_core::screen_ruler::Unit,
    pub ruler_continuous: bool,
    pub ruler_per_channel: bool,
    pub ruler_feet: bool,
    pub ruler_tolerance: u8,
    pub ruler_color: Rgb,
    pub resize: ResizeSettings,
    pub startup: bool,
    pub theme: powerkit_core::settings::AppTheme,
    pub language: powerkit_core::settings::AppLanguage,
    pub show_tray: bool,
    pub shell_opacity: i32,
    /// Color for the selected rail icon; `None` keeps WinUI theme colors.
    pub rail_accent: Option<Rgb>,
    pub ui_language: UiLanguage,
    pub hint: String,
    pub power_store: crate::controller::power_store::Snapshot,
    pub media_renamer: crate::controller::media_renamer::Snapshot,
    pub mouse: powerkit_core::power_mouse::Preferences,
    pub mouse_elevated: bool,
    /// Connected devices a vendor adapter exists for; empty when none.
    pub mouse_devices: Vec<powerkit_core::power_mouse::device::Device>,
    /// What each running adapter reports for its device, by device key.
    pub mouse_adapters:
        std::collections::BTreeMap<String, powerkit_core::power_mouse::device::AdapterStatus>,
    /// Bumped whenever a background-fetched device render finishes loading.
    pub mouse_assets_rev: u64,
    pub modules: Modules,
    /// Tools this platform cannot offer. A UI leaves their pages, switches
    /// and settings out; empty where every tool exists.
    pub unavailable: Vec<crate::controller::settings::Tool>,
    /// Installed from the MSIX package rather than run from a build folder.
    pub packaged: bool,
    /// The application version, as shown on the Home page.
    pub version: &'static str,
    /// What the platform calls the meta key in a shortcut (Win, Command,
    /// Super); pass it to `Hotkey::label` and the PowerMouse summaries.
    pub meta_key: &'static str,
    pub tray: TrayLayout,
    /// Phase for the Home hero opacity cascade (advanced on the poll timer).
    pub hero_phase: u8,
}
impl Snapshot {
    /// Whether the platform offers the tool `page` belongs to.
    pub fn offers(&self, page: Page) -> bool {
        page.tool()
            .is_none_or(|tool| !self.unavailable.contains(&tool))
    }
    /// Whether `page` can be opened: its tool exists here and is switched on.
    pub fn shows(&self, page: Page) -> bool {
        self.offers(page) && page.module_enabled(self.modules, self.mouse.enabled)
    }
}
/// What the settings shell is waiting to record from the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureTarget {
    Hotkey(Command),
    MouseCustom(MouseSlot),
    MouseAddFKey,
}
/// Index of the AlwaysOn session in `MODE_ORDER`; `None` for an expiry, which
/// has its own card rather than a radio choice.
pub fn mode_index(session: Session) -> Option<usize> {
    let command = Command::for_session(session);
    MODE_ORDER.iter().position(|item| *item == command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_shown_when_its_tool_exists_here_and_is_switched_on() {
        use crate::controller::settings::Tool;
        let mut view = Snapshot::default();
        view.modules.always_on = true;
        view.modules.screen_ruler = false;
        assert!(view.shows(Page::General));
        assert!(view.shows(Page::AlwaysOn));
        // Switched off: offered, but not shown.
        assert!(view.offers(Page::ScreenRuler));
        assert!(!view.shows(Page::ScreenRuler));
        // A platform without the tool hides it whatever the switch says.
        view.unavailable = vec![Tool::AlwaysOn];
        assert!(!view.offers(Page::AlwaysOn));
        assert!(!view.shows(Page::AlwaysOn));
        assert!(view.shows(Page::Settings));
        for page in Page::ALL {
            assert_eq!(
                page.tool().is_none(),
                matches!(page, Page::General | Page::Settings)
            );
        }
    }

    #[test]
    fn mode_index_follows_the_running_session() {
        use Session;
        assert_eq!(mode_index(Session::Off), Some(0));
        assert_eq!(mode_index(Session::interval(0, 60, false)), Some(2));
        assert_eq!(mode_index(Session::interval(0, 60, true)), Some(4));
        assert_eq!(mode_index(Session::Indefinite), Some(5));
        assert_eq!(
            mode_index(Session::Expiry {
                end: 5,
                target: powerkit_core::primitives::UtcTicks(5)
            }),
            None
        );
        assert_eq!(mode_index(Session::Process { pid: 5 }), None);
    }
}
