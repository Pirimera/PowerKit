//! Settings shell: a WinUI 3 window rendered by windows-reactor.
//!
//! The shell only renders snapshots and reports user intent. Every edit is posted to
//! the hidden Win32 app window as the same WM_COMMAND the tray menu uses, so
//! preference writes, power requests and hotkey ownership stay in `mod.rs`.
//! Shared Fluent surfaces live in `super::settings_ui`; per-tool pages in `pages`.
mod manager_icon;
mod pages;
mod rename_drawer;
mod rename_page;
mod store_drawer;
mod store_page;
mod view_helpers;

use super::command::{
    self, LANG_AR, LANG_EN, LANG_SYSTEM, LANG_TR, RESIZE_ENC_BMP, RESIZE_ENC_GIF, RESIZE_ENC_JPEG,
    RESIZE_ENC_PNG, RESIZE_ENC_TIFF, RESIZE_PNG_DEFAULT, RESIZE_PNG_OFF, RESIZE_PNG_ON,
    RULER_MODE_BOUNDS, RULER_MODE_HORIZONTAL, RULER_MODE_NONE, RULER_MODE_SPACING,
    RULER_MODE_VERTICAL, RULER_UNIT_CM, RULER_UNIT_DIP, RULER_UNIT_IN, RULER_UNIT_MM,
    RULER_UNIT_PX, THEME_DARK, THEME_LIGHT, THEME_SYSTEM,
};
#[cfg(test)]
use super::command::{CUSTOM, FOREVER, HALF_HOUR, HOUR, OFF, TWO_HOURS};
use super::media_renamer::Action as MediaRenamerAction;
use super::power_store::{Action as PowerStoreAction, DrawerView};
use super::{resize_dialog, settings_ui};
use crate::image_resizer::{FallbackEncoder, PngInterlace, ResizeSettings};
pub(super) use command::{
    AppCommand, LANG_ORDER, MODE_ORDER, RESIZE_ENCODER_ORDER, RESIZE_PNG_ORDER, RULER_MODE_ORDER,
    RULER_UNIT_ORDER, THEME_ORDER,
};
use std::{cell::RefCell, io, path::PathBuf};
use windows_reactor::*;
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{PostMessageW, WM_COMMAND},
};

const CLIENT_WIDTH: f64 = 980.0;
const CLIENT_HEIGHT: f64 = 720.0;
const MIN_WIDTH: f64 = 640.0;
const MIN_HEIGHT: f64 = 480.0;
const PACKAGE_ID: &str = "PiriMedya.PowerKit";
const LABEL_WINDOW_TITLE: &str = "PowerKit";
const TITLE_LOGO: &[u8] = include_bytes!("../../../../brand/windows/png/powerkit-32.png");
const RELEASE_NOTES_URI: &str = "https://github.com/Pirimera/PowerKit/blob/main/CHANGELOG.md";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Page {
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
/// Primary rail pages (Settings sits in the rail footer, not here).
/// Workspaces follow General; utilities come next.
const RAIL_PAGES: [Page; 8] = [
    Page::General,
    Page::PowerStore,
    Page::MediaRenamer,
    Page::AlwaysOn,
    Page::AlwaysTop,
    Page::ScreenRuler,
    Page::ImageResizer,
    Page::PowerMouse,
];
#[cfg(test)]
const ALL_PAGES: [Page; 9] = [
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
impl Page {
    /// Whether this rail/tool page is enabled in preferences.
    pub(super) fn module_enabled(self, modules: crate::settings::Modules, mouse_on: bool) -> bool {
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
    pub(super) fn tag(self) -> &'static str {
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
    #[cfg(test)]
    fn from_tag(tag: &str) -> Option<Self> {
        ALL_PAGES.into_iter().find(|page| page.tag() == tag)
    }
    fn label(self, lang: crate::locale::UiLanguage) -> &'static str {
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
    pub(super) fn glyph(self) -> &'static str {
        match self {
            Page::AlwaysOn => "\u{E7E8}",
            Page::AlwaysTop => "\u{E718}",
            Page::ScreenRuler => "\u{ED5E}",
            Page::ImageResizer => "\u{E91B}",
            Page::PowerMouse => "\u{E962}",
            Page::PowerStore => "\u{E71B}",
            Page::MediaRenamer => "\u{E8AC}",
            Page::General => "\u{E80F}",
            Page::Settings => "\u{E713}",
        }
    }
    /// Segoe Fluent filled variant where one exists (HomeSolid, PinFill).
    pub(super) fn selected_glyph(self) -> &'static str {
        match self {
            Page::General => "\u{EA8A}",
            Page::AlwaysTop => "\u{E842}",
            _ => self.glyph(),
        }
    }
}
/// Everything the shell renders, rebuilt by `mod.rs` from its state on each change.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Snapshot {
    pub status: String,
    pub mode: Option<usize>,
    pub display: bool,
    pub interval_minutes: u32,
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
    pub frame_color: u32,
    pub frame_opacity: i32,
    pub frame_thickness: i32,
    pub ruler_active: bool,
    pub ruler_hotkey: String,
    pub ruler_mode: usize,
    pub ruler_unit: usize,
    pub ruler_continuous: bool,
    pub ruler_per_channel: bool,
    pub ruler_feet: bool,
    pub ruler_tolerance: u8,
    pub ruler_color: u32,
    pub resize: ResizeSettings,
    pub startup: bool,
    pub theme: usize,
    pub language: usize,
    pub show_tray: bool,
    pub shell_opacity: i32,
    /// COLORREF for the selected rail icon; `None` keeps WinUI theme colors.
    pub rail_accent: Option<u32>,
    pub ui_language: crate::locale::UiLanguage,
    pub hint: String,
    pub power_store: super::power_store::Snapshot,
    pub media_renamer: super::media_renamer::Snapshot,
    pub mouse: crate::power_mouse::Preferences,
    pub mouse_elevated: bool,
    pub modules: crate::settings::Modules,
    /// Phase for the Home hero opacity cascade (advanced on the poll timer).
    pub hero_phase: u8,
}
/// PowerStore install option edited in the options card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OptionsField {
    Scope,
    Architecture,
    Version,
    ExtraArgs,
}
/// Which settings color the shell ColorPicker is applying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ColorTarget {
    Frame,
    Ruler,
}

#[derive(Default)]
struct Bridge {
    sender: Option<LocalSender<Message>>,
    latest: Snapshot,
    page: Page,
    filename: Option<String>,
    power_store_actions: Vec<PowerStoreAction>,
    media_renamer_actions: Vec<MediaRenamerAction>,
    power_mouse_actions: Vec<PowerMouseAction>,
    color: Option<u32>,
    shell_opacity: Option<i32>,
    interval: Option<u32>,
    expiry: Option<ExpiryPick>,
    hero_phase: u8,
}
/// Date and time picked for an AlwaysOn expiry, in WinRT 100 ns ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExpiryPick {
    /// Any instant on the chosen local day; `None` means the next occurrence.
    pub date: Option<i64>,
    /// Time since local midnight.
    pub time_of_day: i64,
}
/// Opens the WinUI Image Resizer dialog when the settings shell is running.
/// Returns false when WinUI is unavailable (smoke test or missing runtime); there is
/// no other resize UI.
pub(super) fn open_resize(
    app: windows_sys::Win32::Foundation::HWND,
    files: Vec<PathBuf>,
    settings: ResizeSettings,
) -> bool {
    let seed = resize_dialog::Seed {
        app: app as isize,
        files,
        settings,
    };
    BRIDGE.with(|bridge| {
        let bridge = bridge.borrow();
        let Some(sender) = bridge.sender.clone() else {
            return false;
        };
        drop(bridge);
        if resize_dialog::reload(seed.clone()) {
            let _ = super::winui_host::present_dialog();
            return true;
        }
        resize_dialog::queue(seed);
        sender.send(Message::OpenResize);
        true
    })
}
thread_local! {
    static BRIDGE: RefCell<Bridge> = RefCell::new(Bridge::default());
}
/// Hands the latest app state to the shell. Safe before the window exists.
pub(super) fn publish(snapshot: Snapshot) {
    let sender = BRIDGE.with(|bridge| {
        let mut bridge = bridge.borrow_mut();
        if bridge.latest == snapshot {
            return None;
        }
        bridge.latest = snapshot.clone();
        bridge.sender.clone()
    });
    if let Some(sender) = sender {
        sender.send(Message::Refresh(Box::new(snapshot)));
    }
}
/// Re-sends the current snapshot, discarding an applied draft the app rejected.
pub(super) fn refresh() {
    let pending = BRIDGE.with(|bridge| {
        let bridge = bridge.borrow();
        bridge
            .sender
            .clone()
            .map(|sender| (sender, bridge.latest.clone()))
    });
    if let Some((sender, snapshot)) = pending {
        sender.send(Message::Refresh(Box::new(snapshot)));
    }
}
pub(super) fn show_page(page: Page) {
    let sender = BRIDGE.with(|bridge| {
        let mut bridge = bridge.borrow_mut();
        bridge.page = page;
        bridge.sender.clone()
    });
    if let Some(sender) = sender {
        sender.send(Message::Navigate(page));
    }
}
pub(super) fn current_page() -> Page {
    BRIDGE.with(|bridge| bridge.borrow().page)
}
pub(super) fn hero_phase() -> u8 {
    BRIDGE.with(|bridge| bridge.borrow().hero_phase)
}
/// Advances the Home hero animation when the shell shows General. A hidden or
/// minimized window skips the tick, so the tray app does not re-render every second.
pub(super) fn tick_hero() {
    if !super::winui_host::settings_visible() {
        return;
    }
    let pending = BRIDGE.with(|bridge| {
        let mut bridge = bridge.borrow_mut();
        if bridge.page != Page::General {
            return None;
        }
        bridge.hero_phase = bridge.hero_phase.wrapping_add(1);
        bridge.latest.hero_phase = bridge.hero_phase;
        bridge
            .sender
            .clone()
            .map(|sender| (sender, bridge.latest.clone()))
    });
    if let Some((sender, snapshot)) = pending {
        sender.send(Message::Refresh(Box::new(snapshot)));
    }
}
/// The filename draft the user applied, consumed by the RESIZE_FILENAME command.
pub(super) fn take_filename() -> Option<String> {
    BRIDGE.with(|bridge| bridge.borrow_mut().filename.take())
}
/// PowerStore page requests, consumed by the PowerStoreAction command.
pub(super) fn take_power_store_actions() -> Vec<PowerStoreAction> {
    BRIDGE.with(|bridge| std::mem::take(&mut bridge.borrow_mut().power_store_actions))
}
/// MediaRenamer page requests, consumed by the MediaRenamerAction command.
pub(super) fn take_media_renamer_actions() -> Vec<MediaRenamerAction> {
    BRIDGE.with(|bridge| std::mem::take(&mut bridge.borrow_mut().media_renamer_actions))
}
/// PowerMouse page requests, consumed by the PowerMouseAction command.
pub(super) fn take_power_mouse_actions() -> Vec<PowerMouseAction> {
    BRIDGE.with(|bridge| std::mem::take(&mut bridge.borrow_mut().power_mouse_actions))
}
/// Binding change queued from the PowerMouse page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PowerMouseAction {
    SetBinding {
        trigger: crate::power_mouse::Trigger,
        binding: crate::power_mouse::Binding,
    },
    ResetDefaults,
    BeginCustomCapture {
        trigger: crate::power_mouse::Trigger,
        slot: crate::power_mouse::ActionSlot,
    },
    BeginAddFKey,
    RemoveFKey {
        trigger: crate::power_mouse::Trigger,
    },
}
/// What the settings shell is waiting to record from the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptureTarget {
    Hotkey(AppCommand),
    MouseCustom {
        trigger: crate::power_mouse::Trigger,
        slot: crate::power_mouse::ActionSlot,
    },
    MouseAddFKey,
}
/// COLORREF chosen in the shell ColorPicker, consumed by PickFrame/RulerColor.
pub(super) fn take_color() -> Option<u32> {
    BRIDGE.with(|bridge| bridge.borrow_mut().color.take())
}
/// Settings-window opacity percent from the shell slider.
pub(super) fn take_shell_opacity() -> Option<i32> {
    BRIDGE.with(|bridge| bridge.borrow_mut().shell_opacity.take())
}
/// Custom AlwaysOn interval in minutes, consumed by ApplyInterval.
pub(super) fn take_interval() -> Option<u32> {
    BRIDGE.with(|bridge| bridge.borrow_mut().interval.take())
}
/// Expiry picked in the shell, consumed by AlwaysOnExpiry.
pub(super) fn take_expiry() -> Option<ExpiryPick> {
    BRIDGE.with(|bridge| bridge.borrow_mut().expiry.take())
}
/// Runs the WinUI application on this thread until its last window closes.
pub(super) fn run(app: HWND) -> io::Result<()> {
    super::winui_host::expect(super::winui_host::WindowRole::Settings);
    App::run_component::<Shell>(AppWindow(app as isize))
        .map_err(|error| io::Error::other(error.to_string()))
}
fn open_resize_window(context: &ComponentContext<Shell>, seed: resize_dialog::Seed) {
    super::winui_host::expect(super::winui_host::WindowRole::Dialog);
    let _ = context.open_window(View::component::<resize_dialog::Dialog>(seed));
}
#[derive(Clone, Copy, PartialEq)]
struct AppWindow(isize);
impl AppWindow {
    fn post(self, command: AppCommand) {
        // SAFETY: the app window lives on this UI thread; posting to a destroyed
        // window fails without side effects.
        unsafe {
            PostMessageW(self.0 as HWND, WM_COMMAND, command.id(), 0);
        }
    }
}
#[derive(Clone)]
enum Message {
    Noop,
    Refresh(Box<Snapshot>),
    Navigate(Page),
    Command(AppCommand),
    Toggle(AppCommand, bool),
    Choose(&'static [AppCommand], Option<usize>),
    EditFilename(String),
    ApplyFilename,
    PowerStore(PowerStoreAction),
    PowerStoreFilterText(String),
    MediaRenamer(MediaRenamerAction),
    MediaRenamerTemplate(String),
    MediaRenamerFilter(String),
    PowerMouse(PowerMouseAction),
    OpenDiscoverDialog,
    EditDiscoverQuery(String),
    PickDiscoverSource(Option<usize>),
    SubmitDiscover,
    DiscoverDialogClosed(ContentDialogResult),
    EditPowerStoreOptions(OptionsField, String),
    SavePowerStoreOptions,
    ApplyColor(ColorTarget, u32),
    ApplyShellOpacity(i32),
    CaptureHotkey(AppCommand),
    OpenResize,
    IntervalHours(Option<f64>),
    IntervalMinutes(Option<f64>),
    ExpiryDate(Option<DateTime>),
    ExpiryTime(Option<TimeSpan>),
    StartExpiry,
}
struct Shell {
    app: AppWindow,
    page: Page,
    snapshot: Snapshot,
    filename_draft: Option<String>,
    applying_filename: bool,
    discover_dialog_open: bool,
    discover_query_draft: String,
    discover_source_index: usize,
    /// Unsaved options for the package open in the inspector drawer.
    power_store_options_draft: Option<crate::power_store::InstallOptions>,
    /// Uncontrolled picker values; cleared when the page remounts the pickers.
    expiry_date: Option<i64>,
    expiry_time: Option<i64>,
}
impl Component for Shell {
    type Input = AppWindow;
    type Message = Message;
    fn create(input: &AppWindow, context: &ComponentContext<Self>) -> Self {
        let (snapshot, page) = BRIDGE.with(|bridge| {
            let mut bridge = bridge.borrow_mut();
            bridge.sender = Some(context.sender());
            (bridge.latest.clone(), bridge.page)
        });
        if let Some(seed) = resize_dialog::take_pending() {
            open_resize_window(context, seed);
        }
        Self {
            app: *input,
            page,
            snapshot,
            filename_draft: None,
            applying_filename: false,
            discover_dialog_open: false,
            discover_query_draft: String::new(),
            discover_source_index: 0,
            power_store_options_draft: None,
            expiry_date: None,
            expiry_time: None,
        }
    }
    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            Message::Noop => {}
            Message::Refresh(snapshot) => {
                let inspected = |snapshot: &Snapshot| match &snapshot.power_store.drawer {
                    DrawerView::Inspector(inspector) => Some(inspector.key.clone()),
                    _ => None,
                };
                if inspected(&snapshot) != inspected(&self.snapshot) {
                    self.power_store_options_draft = None;
                }
                self.snapshot = *snapshot;
                if self.applying_filename {
                    self.applying_filename = false;
                    self.filename_draft = None;
                }
            }
            Message::Navigate(page) => {
                let page =
                    if page.module_enabled(self.snapshot.modules, self.snapshot.mouse.enabled) {
                        page
                    } else {
                        Page::General
                    };
                let entered = page != self.page;
                if entered {
                    self.expiry_date = None;
                    self.expiry_time = None;
                    self.discover_dialog_open = false;
                }
                self.page = page;
                BRIDGE.with(|bridge| bridge.borrow_mut().page = page);
                if entered && page == Page::PowerStore {
                    self.app.post(AppCommand::PowerStoreEnsureLoaded);
                }
            }
            Message::IntervalHours(hours) => {
                let minutes = self.snapshot.interval_minutes % 60;
                self.apply_interval(hours, |hours| hours * 60 + i64::from(minutes));
            }
            Message::IntervalMinutes(minutes) => {
                let hours = self.snapshot.interval_minutes / 60;
                self.apply_interval(minutes, |minutes| i64::from(hours) * 60 + minutes);
            }
            Message::ExpiryDate(date) => self.expiry_date = date.map(|date| date.universal_time),
            Message::ExpiryTime(time) => self.expiry_time = time.map(|time| time.duration),
            Message::StartExpiry => {
                if let Some(time_of_day) = self.expiry_time {
                    let pick = ExpiryPick {
                        date: self.expiry_date,
                        time_of_day,
                    };
                    BRIDGE.with(|bridge| bridge.borrow_mut().expiry = Some(pick));
                    self.app.post(AppCommand::AlwaysOnExpiry);
                }
            }
            Message::Command(command) => self.app.post(command),
            Message::Toggle(command, on) => {
                if self.toggled(command) != on {
                    self.app.post(command);
                }
            }
            Message::Choose(order, index) => {
                if let Some(&command) = index.and_then(|index| order.get(index))
                    && self.chosen(order) != index
                {
                    self.app.post(command);
                }
            }
            Message::EditFilename(text) => self.filename_draft = Some(text),
            Message::PowerStore(action) => self.post_power_store(action),
            Message::PowerStoreFilterText(text) => {
                if text != self.snapshot.power_store.query.filter_text {
                    self.post_power_store(PowerStoreAction::SetFilterText(text));
                }
            }
            Message::MediaRenamer(action) => self.post_media_renamer(action),
            Message::MediaRenamerTemplate(text) => {
                if text != self.snapshot.media_renamer.template {
                    self.post_media_renamer(MediaRenamerAction::SetTemplate(text));
                }
            }
            Message::MediaRenamerFilter(text) => {
                if text != self.snapshot.media_renamer.filter_text {
                    self.post_media_renamer(MediaRenamerAction::SetFilterText(text));
                }
            }
            Message::PowerMouse(action) => self.post_power_mouse(action),
            Message::OpenDiscoverDialog => {
                if self.discover_query_draft.is_empty()
                    && !self.snapshot.power_store.discover_query.is_empty()
                {
                    self.discover_query_draft = self.snapshot.power_store.discover_query.clone();
                }
                self.discover_source_index = self.power_store_discover_source_index(
                    self.snapshot.power_store.discover_source.as_deref(),
                );
                self.discover_dialog_open = true;
            }
            Message::EditDiscoverQuery(text) => self.discover_query_draft = text,
            Message::PickDiscoverSource(index) => {
                if let Some(index) = index {
                    self.discover_source_index = index;
                }
            }
            Message::SubmitDiscover => {
                let query = self.discover_query_draft.trim().to_string();
                if !query.is_empty() {
                    self.discover_dialog_open = false;
                    let source = self.power_store_discover_source_value(self.discover_source_index);
                    self.post_power_store(PowerStoreAction::StartDiscover { query, source });
                }
            }
            Message::DiscoverDialogClosed(result) => {
                self.discover_dialog_open = false;
                if result == ContentDialogResult::Primary {
                    let query = self.discover_query_draft.trim().to_string();
                    if !query.is_empty() {
                        let source =
                            self.power_store_discover_source_value(self.discover_source_index);
                        self.post_power_store(PowerStoreAction::StartDiscover { query, source });
                    }
                }
            }
            Message::EditPowerStoreOptions(field, text) => {
                if let DrawerView::Inspector(inspector) = &self.snapshot.power_store.drawer {
                    let draft = self
                        .power_store_options_draft
                        .get_or_insert_with(|| inspector.options.clone());
                    let value = Some(text).filter(|text| !text.is_empty());
                    match field {
                        OptionsField::Scope => draft.scope = value,
                        OptionsField::Architecture => draft.architecture = value,
                        OptionsField::Version => draft.version = value,
                        OptionsField::ExtraArgs => draft.extra_args = value,
                    }
                }
            }
            Message::SavePowerStoreOptions => {
                if let DrawerView::Inspector(inspector) = &self.snapshot.power_store.drawer {
                    let options = self
                        .power_store_options_draft
                        .take()
                        .unwrap_or_else(|| inspector.options.clone());
                    self.post_power_store(PowerStoreAction::SaveOptions(
                        inspector.key.clone(),
                        options,
                    ));
                }
            }
            Message::ApplyFilename => {
                if let Some(text) = self.filename_draft.clone() {
                    BRIDGE.with(|bridge| bridge.borrow_mut().filename = Some(text));
                    self.applying_filename = true;
                    self.app.post(AppCommand::ApplyResizeFilename);
                }
            }
            Message::ApplyColor(target, color) => {
                BRIDGE.with(|bridge| bridge.borrow_mut().color = Some(color));
                self.app.post(match target {
                    ColorTarget::Frame => AppCommand::PickFrameColor,
                    ColorTarget::Ruler => AppCommand::PickRulerColor,
                });
            }
            Message::ApplyShellOpacity(percent) => {
                BRIDGE.with(|bridge| bridge.borrow_mut().shell_opacity = Some(percent));
                self.app.post(AppCommand::ApplyShellOpacity);
            }
            Message::CaptureHotkey(command) => self.app.post(command),
            Message::OpenResize => {
                if let Some(seed) = resize_dialog::take_pending() {
                    open_resize_window(context, seed);
                }
            }
        }
    }
    fn view(&self, _: &AppWindow, context: &mut ViewContext<Self>) -> View {
        context.window_title(LABEL_WINDOW_TITLE);
        let theme = match AppCommand::from_id(self.snapshot.theme) {
            Some(AppCommand::ThemeLight) => WindowTheme::Light,
            Some(AppCommand::ThemeDark) => WindowTheme::Dark,
            _ => WindowTheme::System,
        };
        context.window_visuals(
            WindowVisuals::new()
                .backdrop(WindowBackdrop::Acrylic)
                .theme(theme)
                .client_size(CLIENT_WIDTH, CLIENT_HEIGHT)
                .constraints(WindowConstraints {
                    min_width: Some(MIN_WIDTH),
                    min_height: Some(MIN_HEIGHT),
                    max_width: None,
                    max_height: None,
                }),
        );
        let rail_item = |page: Page| {
            settings_ui::nav_rail_item(
                settings_ui::RailItem {
                    glyph: page.glyph(),
                    selected_glyph: page.selected_glyph(),
                    label: page.label(self.snapshot.ui_language),
                    selected: page == self.page,
                    accent: self.snapshot.rail_accent,
                },
                Button::new().on_click(context.message(Message::Navigate(page))),
            )
        };
        let rail_items: Vec<KeyedView> = RAIL_PAGES
            .into_iter()
            .filter(|page| page.module_enabled(self.snapshot.modules, self.snapshot.mouse.enabled))
            .map(|page| KeyedView::new(page.tag(), rail_item(page)))
            .collect();
        let rail = Grid::new()
            .width(settings_ui::RAIL_ITEM_WIDTH)
            .margin(Thickness::new(4.0, 0.0, 4.0, 4.0))
            .rows([GridLength::STAR, GridLength::Auto])
            .grid_column(0)
            .children((
                StackPanel::new()
                    .grid_row(0)
                    .spacing(2.0)
                    .keyed_children(rail_items),
                StackPanel::new()
                    .grid_row(1)
                    .children((rail_item(Page::Settings),)),
            ));
        // Store-style main pane: acrylic still shows through, but an extra 50%
        // SolidBackground over the shared shell tint makes it less transparent
        // than the rail/title chrome. Opacity is on a sibling so cards stay solid.
        let pane_body: View = if self.page == Page::PowerStore {
            self.power_store_pane(context)
        } else if self.page == Page::MediaRenamer {
            self.media_renamer_pane(context)
        } else {
            // Stretch across the STAR column with horizontal scroll disabled; setting
            // MaxWidth on a stretched WinUI 3 child shifts its arrange offset right once
            // the pane exceeds MaxWidth and clips controls past the viewport's right edge.
            ScrollViewer::new()
                .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                .content(
                    StackPanel::new()
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .margin(Thickness::new(28.0, 16.0, 36.0, 36.0))
                        .keyed_children([KeyedView::new(self.page.tag(), self.page_view(context))]),
                )
        };
        let content = Grid::new().grid_column(1).keyed_children([
            KeyedView::new(
                "bg",
                Border::new()
                    .background(ThemeBrush::SolidBackground)
                    .opacity(settings_ui::CONTENT_PANE_EXTRA_OPACITY)
                    .corner_radius(CornerRadius::new(
                        settings_ui::CONTENT_TOP_LEFT_RADIUS,
                        0.0,
                        0.0,
                        0.0,
                    )),
            ),
            KeyedView::new(
                match self.page {
                    Page::PowerStore => "power-store-pane",
                    Page::MediaRenamer => "media-renamer-pane",
                    _ => "scroll-pane",
                },
                pane_body,
            ),
        ]);
        let body = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR])
            .grid_row(1)
            .children((rail, content));
        let title_bar = TitleBar::new()
            .preferred_height(WindowTitleBarHeight::Standard)
            .is_back_button_visible(false)
            .is_pane_toggle_button_visible(false)
            .grid_row(0);
        // Drawn over the TitleBar rather than in its Content slot, which centers content.
        // WinUI TitleBar metrics: 16 left inset, 16×16 icon, 16 gap, 12px caption title.
        // The caption region is non-client, so this overlay does not block window drag.
        let title_brand = StackPanel::new()
            .grid_row(0)
            .orientation(Orientation::Horizontal)
            .spacing(16.0)
            .margin(Thickness::new(16.0, 0.0, 0.0, 0.0))
            .horizontal_alignment(HorizontalAlignment::Left)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                Image::new()
                    .source_data(EncodedImage::from_static(TITLE_LOGO))
                    .width(16.0)
                    .height(16.0)
                    .vertical_alignment(VerticalAlignment::Center),
                TextBlock::new()
                    .text(LABEL_WINDOW_TITLE)
                    .font_size(12.0)
                    .vertical_alignment(VerticalAlignment::Center),
            ));
        let tint = Border::new()
            .background(ThemeBrush::SolidBackground)
            .opacity(settings_ui::acrylic_tint_opacity(
                self.snapshot.shell_opacity,
            ))
            .grid_row(0)
            .grid_row_span(2);
        Grid::new()
            .rows([GridLength::Auto, GridLength::STAR])
            .children((tint, title_bar, title_brand, body))
    }
}
impl Shell {
    fn t(&self) -> &'static crate::locale::Strings {
        self.snapshot.ui_language.strings()
    }
    /// Posts a changed custom interval; NumberBox also reports programmatic values.
    fn post_power_store(&self, action: PowerStoreAction) {
        BRIDGE.with(|bridge| bridge.borrow_mut().power_store_actions.push(action));
        self.app.post(AppCommand::PowerStoreAction);
    }
    fn post_media_renamer(&self, action: MediaRenamerAction) {
        BRIDGE.with(|bridge| bridge.borrow_mut().media_renamer_actions.push(action));
        self.app.post(AppCommand::MediaRenamerAction);
    }
    fn post_power_mouse(&self, action: PowerMouseAction) {
        BRIDGE.with(|bridge| bridge.borrow_mut().power_mouse_actions.push(action));
        self.app.post(AppCommand::PowerMouseAction);
    }
    fn apply_interval(&self, value: Option<f64>, total: impl FnOnce(i64) -> i64) {
        let Some(value) = value.filter(|value| value.is_finite()) else {
            return;
        };
        let minutes = crate::always_on::clamp_interval(total(value.round() as i64));
        if minutes != self.snapshot.interval_minutes {
            BRIDGE.with(|bridge| bridge.borrow_mut().interval = Some(minutes));
            self.app.post(AppCommand::ApplyInterval);
        }
    }
    fn toggled(&self, command: AppCommand) -> bool {
        let s = &self.snapshot;
        match command {
            AppCommand::ToggleDisplay => s.display,
            AppCommand::ToggleAlwaysOnWarn => s.always_on_warn,
            AppCommand::ToggleAcOnly => s.ac_only,
            AppCommand::ToggleSound => s.sound,
            AppCommand::ToggleStartup => s.startup,
            AppCommand::ToggleShowTray => s.show_tray,
            AppCommand::ToggleFrameAccent => s.frame_accent,
            AppCommand::ToggleRulerContinuous => s.ruler_continuous,
            AppCommand::ToggleRulerChannel => s.ruler_per_channel,
            AppCommand::ToggleRulerFeet => s.ruler_feet,
            AppCommand::ToggleResizeShrink => s.resize.shrink_only,
            AppCommand::ToggleResizeReplace => s.resize.replace,
            AppCommand::ToggleResizeOrientation => s.resize.ignore_orientation,
            AppCommand::ToggleResizeMetadata => s.resize.remove_metadata,
            AppCommand::ToggleResizeKeepDate => s.resize.keep_date,
            AppCommand::ToggleMouse => s.mouse.enabled,
            AppCommand::ToggleModuleAlwaysOn => s.modules.always_on,
            AppCommand::ToggleModuleAlwaysTop => s.modules.always_top,
            AppCommand::ToggleModuleScreenRuler => s.modules.screen_ruler,
            AppCommand::ToggleModuleImageResizer => s.modules.image_resizer,
            AppCommand::ToggleModulePowerStore => s.modules.power_store,
            AppCommand::ToggleModuleMediaRenamer => s.modules.media_renamer,
            _ => false,
        }
    }
    fn chosen(&self, order: &[AppCommand]) -> Option<usize> {
        let s = &self.snapshot;
        let id = match order.first().copied() {
            Some(AppCommand::AlwaysOnOff) => return s.mode,
            Some(AppCommand::RulerModeNone) => s.ruler_mode,
            Some(AppCommand::RulerUnitPx) => s.ruler_unit,
            Some(AppCommand::ResizeEncoderJpeg) => encoder_button(s.resize.fallback),
            Some(AppCommand::ResizePngDefault) => png_button(s.resize.png),
            Some(AppCommand::ThemeSystem) => s.theme,
            Some(AppCommand::LanguageSystem) => s.language,
            _ => return None,
        };
        order.iter().position(|item| item.id() == id)
    }
}
pub(super) fn mode_id(mode: crate::screen_ruler::Mode) -> usize {
    match mode {
        crate::screen_ruler::Mode::None => RULER_MODE_NONE,
        crate::screen_ruler::Mode::Bounds => RULER_MODE_BOUNDS,
        crate::screen_ruler::Mode::Spacing => RULER_MODE_SPACING,
        crate::screen_ruler::Mode::Horizontal => RULER_MODE_HORIZONTAL,
        crate::screen_ruler::Mode::Vertical => RULER_MODE_VERTICAL,
    }
}
pub(super) fn unit_id(unit: crate::screen_ruler::Unit) -> usize {
    match unit {
        crate::screen_ruler::Unit::Pixel => RULER_UNIT_PX,
        crate::screen_ruler::Unit::Inch => RULER_UNIT_IN,
        crate::screen_ruler::Unit::Centimetre => RULER_UNIT_CM,
        crate::screen_ruler::Unit::Millimetre => RULER_UNIT_MM,
        crate::screen_ruler::Unit::Dip => RULER_UNIT_DIP,
    }
}
pub(super) fn theme_id(theme: crate::settings::AppTheme) -> usize {
    match theme {
        crate::settings::AppTheme::System => THEME_SYSTEM,
        crate::settings::AppTheme::Light => THEME_LIGHT,
        crate::settings::AppTheme::Dark => THEME_DARK,
    }
}
pub(super) fn language_id(language: crate::settings::AppLanguage) -> usize {
    match language {
        crate::settings::AppLanguage::System => LANG_SYSTEM,
        crate::settings::AppLanguage::Turkish => LANG_TR,
        crate::settings::AppLanguage::English => LANG_EN,
        crate::settings::AppLanguage::Arabic => LANG_AR,
    }
}
fn encoder_button(encoder: FallbackEncoder) -> usize {
    match encoder {
        FallbackEncoder::Jpeg => RESIZE_ENC_JPEG,
        FallbackEncoder::Png => RESIZE_ENC_PNG,
        FallbackEncoder::Bmp => RESIZE_ENC_BMP,
        FallbackEncoder::Gif => RESIZE_ENC_GIF,
        FallbackEncoder::Tiff => RESIZE_ENC_TIFF,
    }
}
fn png_button(png: PngInterlace) -> usize {
    match png {
        PngInterlace::Default => RESIZE_PNG_DEFAULT,
        PngInterlace::On => RESIZE_PNG_ON,
        PngInterlace::Off => RESIZE_PNG_OFF,
    }
}
/// Index of the AlwaysOn session in `MODE_ORDER`; `None` for an expiry, which
/// has its own card rather than a radio choice.
pub(super) fn mode_index(session: crate::always_on::Session) -> Option<usize> {
    let command = AppCommand::for_session(session);
    MODE_ORDER.iter().position(|item| *item == command)
}
fn page_copy(page: Page, lang: crate::locale::UiLanguage) -> (&'static str, &'static str) {
    let t = lang.strings();
    match page {
        Page::General => (t.general_title, t.general_lead),
        Page::AlwaysOn => (t.always_on_title, t.always_on_lead),
        Page::AlwaysTop => (t.always_top_title, t.always_top_lead),
        Page::ScreenRuler => (t.ruler_title, t.ruler_page_lead),
        Page::ImageResizer => (t.resize_title, t.resize_lead),
        Page::PowerMouse => (t.mouse.title, t.mouse.lead),
        Page::PowerStore => (t.power_store_title, t.power_store_lead),
        Page::MediaRenamer => (t.rename.title, t.rename.lead),
        Page::Settings => (t.settings_title, t.settings_lead),
    }
}
fn target_arch_label() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "ARM64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        "diğer"
    }
}
fn settings_location_label() -> String {
    match std::env::var_os("LOCALAPPDATA") {
        Some(root) => std::path::PathBuf::from(root)
            .join("PowerKit")
            .join("settings.txt")
            .display()
            .to_string(),
        None => "%LOCALAPPDATA%\\PowerKit\\settings.txt".to_string(),
    }
}
fn about_facts(
    labels: &crate::locale::Strings,
    version: &str,
    arch: &str,
    channel: &str,
    package: &str,
    settings: &str,
) -> [(&'static str, String); 5] {
    [
        (labels.about_version, version.to_string()),
        (labels.about_arch, arch.to_string()),
        (labels.about_channel, channel.to_string()),
        (labels.about_package, package.to_string()),
        (labels.about_settings, settings.to_string()),
    ]
}
fn installed_about_facts(labels: &crate::locale::Strings) -> [(&'static str, String); 5] {
    about_facts(
        labels,
        env!("CARGO_PKG_VERSION"),
        target_arch_label(),
        if super::package::is_packaged() {
            labels.channel_packaged
        } else {
            labels.channel_unpackaged
        },
        PACKAGE_ID,
        &settings_location_label(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_round_trip_through_navigation_tags() {
        for page in ALL_PAGES {
            assert_eq!(Page::from_tag(page.tag()), Some(page));
        }
        assert_eq!(Page::from_tag("settings"), Some(Page::Settings));
        assert_eq!(Page::default(), Page::General);
        assert_eq!(RAIL_PAGES[0], Page::General);
        assert!(!RAIL_PAGES.contains(&Page::Settings));
    }
    #[test]
    fn page_copy_follows_language_catalog() {
        let tr = crate::locale::UiLanguage::Turkish;
        let en = crate::locale::UiLanguage::English;
        assert_eq!(page_copy(Page::General, tr).0, "Ana Sayfa");
        assert_eq!(page_copy(Page::General, en).0, "Home");
        assert_eq!(page_copy(Page::Settings, tr).0, "PowerKit ayarları");
        assert_eq!(page_copy(Page::Settings, en).0, "PowerKit settings");
        assert_eq!(page_copy(Page::AlwaysOn, tr).0, "AlwaysOn");
        assert_eq!(page_copy(Page::AlwaysTop, tr).0, "AlwaysTop");
        assert_eq!(page_copy(Page::ScreenRuler, tr).0, "ScreenRuler");
        assert_eq!(page_copy(Page::ImageResizer, tr).0, "ImageResizer");
        assert_eq!(page_copy(Page::PowerMouse, tr).0, "PowerMouse");
        assert_eq!(page_copy(Page::PowerStore, tr).0, "PowerStore");
        assert_eq!(page_copy(Page::PowerStore, en).0, "PowerStore");
        assert_eq!(Page::General.label(tr), "Ana Sayfa");
        assert_eq!(Page::General.label(en), "Home");
        assert_eq!(Page::Settings.label(tr), "Ayarlar");
        assert_eq!(Page::PowerStore.label(tr), "Power Store");
        assert_eq!(Page::PowerMouse.label(en), "Power Mouse");
        assert_eq!(page_copy(Page::MediaRenamer, tr).0, "MediaRenamer");
        assert_eq!(page_copy(Page::MediaRenamer, en).0, "MediaRenamer");
        assert_eq!(Page::MediaRenamer.label(en), "Media Renamer");
        assert_eq!(RAIL_PAGES[2], Page::MediaRenamer);
        assert_eq!(RAIL_PAGES[7], Page::PowerMouse);
    }
    #[test]
    fn choice_groups_map_back_to_their_settings() {
        assert_eq!(
            MODE_ORDER.map(AppCommand::id),
            [OFF, HALF_HOUR, HOUR, TWO_HOURS, CUSTOM, FOREVER]
        );
        for command in RULER_MODE_ORDER {
            assert_eq!(command.ruler_mode().map(mode_id), Some(command.id()));
        }
        for command in RULER_UNIT_ORDER {
            assert_eq!(command.ruler_unit().map(unit_id), Some(command.id()));
        }
        for command in RESIZE_ENCODER_ORDER {
            assert_eq!(
                command.resize_encoder().map(encoder_button),
                Some(command.id())
            );
        }
        for command in RESIZE_PNG_ORDER {
            assert_eq!(command.resize_png().map(png_button), Some(command.id()));
        }
        assert_eq!(AppCommand::AlwaysOnForever.ruler_mode(), None);
        assert_eq!(AppCommand::ResizeEncoderJpeg.resize_png(), None);
    }
    #[test]
    fn mode_index_follows_the_running_session() {
        use crate::always_on::Session;
        assert_eq!(mode_index(Session::Off), Some(0));
        assert_eq!(mode_index(Session::interval(0, 60, false)), Some(2));
        assert_eq!(mode_index(Session::interval(0, 60, true)), Some(4));
        assert_eq!(mode_index(Session::Indefinite), Some(5));
        assert_eq!(mode_index(Session::Expiry { end: 5, target: 5 }), None);
        assert_eq!(mode_index(Session::Process { pid: 5 }), None);
    }
    #[test]
    fn about_facts_list_version_architecture_channel_and_settings() {
        let tr = crate::locale::UiLanguage::Turkish.strings();
        let rows = about_facts(
            tr,
            "1.2.3",
            "ARM64",
            tr.channel_packaged,
            "PiriMedya.PowerKit",
            r"C:\data\settings.txt",
        );
        assert_eq!(
            rows.each_ref().map(|row| row.0),
            ["Sürüm", "Mimari", "Kanal", "Paket", "Ayarlar"]
        );
        assert_eq!(rows[0].1, "1.2.3");
        assert_eq!(rows[1].1, "ARM64");
        assert_eq!(rows[2].1, "MSIX paketi");
        assert_eq!(rows[3].1, "PiriMedya.PowerKit");
        assert_eq!(rows[4].1, r"C:\data\settings.txt");
        let installed = installed_about_facts(tr);
        assert_eq!(installed[0].1, env!("CARGO_PKG_VERSION"));
        assert!(matches!(installed[1].1.as_str(), "ARM64" | "x64" | "diğer"));
        assert_eq!(installed[2].1, tr.channel_unpackaged);
        assert_eq!(installed[3].1, PACKAGE_ID);
        assert!(installed[4].1.contains("PowerKit"));
        assert!(installed[4].1.contains("settings.txt"));
        assert_eq!(
            about_facts(
                crate::locale::UiLanguage::English.strings(),
                "1",
                "x",
                "c",
                "p",
                "s"
            )[0]
            .0,
            "Version"
        );
    }
}
