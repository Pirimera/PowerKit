//! Settings shell: a WinUI 3 window rendered by windows-reactor.
//!
//! The shell only renders snapshots and reports user intent. Every request goes
//! to the hidden Win32 app window as a `powerkit_app::intent::Intent` that
//! carries its own value, so preference writes, power requests and hotkey
//! ownership stay in `mod.rs`.
//! Shared Fluent surfaces live in `super::settings_ui`; per-tool pages in `pages`.
mod manager_icon;
mod mouse_page;
mod pages;
mod rename_drawer;
mod rename_page;
mod store_drawer;
mod store_page;
mod view_helpers;

use super::{resize_dialog, settings_ui};
pub use powerkit_app::command::{
    Command, LANG_ORDER, MODE_ORDER, RESIZE_ENCODER_ORDER, RESIZE_PNG_ORDER, RULER_MODE_ORDER,
    RULER_UNIT_ORDER, THEME_ORDER,
};
use powerkit_app::controller::media_renamer::Action as MediaRenamerAction;
use powerkit_app::controller::power_store::{Action as PowerStoreAction, DrawerView};
pub use powerkit_app::intent::{ExpiryPick, Intent, MouseAction as PowerMouseAction, MouseSlot};
use powerkit_app::ui::{DeviceImage, HostHandle, MouseImageKind};
pub use powerkit_app::view::{CaptureTarget, Page, Snapshot, mode_index, tray_page};
use powerkit_core::image_resizer::ResizeSettings;
use powerkit_core::primitives::{Rgb, UtcTicks};
use powerkit_core::tray_layout::{TrayEdit, TrayTool};
use std::{cell::RefCell, io, path::PathBuf};
use windows_reactor::*;

const CLIENT_WIDTH: f64 = 980.0;
const CLIENT_HEIGHT: f64 = 720.0;
const MIN_WIDTH: f64 = 640.0;
const MIN_HEIGHT: f64 = 480.0;
const PACKAGE_ID: &str = "PiriMedya.PowerKit";
const LABEL_WINDOW_TITLE: &str = "PowerKit";
const TITLE_LOGO: &[u8] = include_bytes!("../../../../../brand/windows/png/powerkit-32.png");
const RELEASE_NOTES_URI: &str = "https://github.com/Pirimera/PowerKit/blob/main/CHANGELOG.md";
/// Segoe Fluent icons for the pages; the tray menu draws the same glyphs.
pub trait PageGlyph {
    fn glyph(self) -> &'static str;
    fn selected_glyph(self) -> &'static str;
}
impl PageGlyph for Page {
    fn glyph(self) -> &'static str {
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
    fn selected_glyph(self) -> &'static str {
        match self {
            Page::General => "\u{EA8A}",
            Page::AlwaysTop => "\u{E842}",
            _ => self.glyph(),
        }
    }
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
/// PowerStore install option edited in the options card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsField {
    Scope,
    Architecture,
    Version,
    ExtraArgs,
}
/// Which settings color the shell ColorPicker is applying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTarget {
    Frame,
    Ruler,
}

#[derive(Default)]
struct Bridge {
    sender: Option<LocalSender<Message>>,
    latest: Snapshot,
    page: Page,
    hero_phase: u8,
}
/// Opens the WinUI Image Resizer dialog when the settings shell is running.
/// Returns false when WinUI is unavailable (smoke test or missing runtime); there is
/// no other resize UI.
pub fn open_resize(host: HostHandle, files: Vec<PathBuf>, settings: ResizeSettings) -> bool {
    let seed = resize_dialog::Seed {
        host,
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
pub fn publish(snapshot: Snapshot) {
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
pub fn refresh() {
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
pub fn show_page(page: Page) {
    let sender = BRIDGE.with(|bridge| {
        let mut bridge = bridge.borrow_mut();
        bridge.page = page;
        bridge.sender.clone()
    });
    if let Some(sender) = sender {
        sender.send(Message::Navigate(page));
    }
}
pub fn current_page() -> Page {
    BRIDGE.with(|bridge| bridge.borrow().page)
}
pub fn hero_phase() -> u8 {
    BRIDGE.with(|bridge| bridge.borrow().hero_phase)
}
/// Advances the Home hero animation when the shell shows General. A hidden or
/// minimized window skips the tick, so the tray app does not re-render every second.
pub fn tick_hero() {
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
/// Runs the WinUI application on this thread until its last window closes.
pub fn run(host: HostHandle) -> io::Result<()> {
    super::winui_host::expect(super::winui_host::WindowRole::Settings);
    App::run_component::<Shell>(AppWindow(host))
        .map_err(|error| io::Error::other(error.to_string()))
}
fn open_resize_window(context: &ComponentContext<Shell>, seed: resize_dialog::Seed) {
    super::winui_host::expect(super::winui_host::WindowRole::Dialog);
    let _ = context.open_window(View::component::<resize_dialog::Dialog>(seed));
}
/// The latest resize settings and UI strings for the resize dialog, which
/// shares this module's snapshot rather than asking the platform.
pub fn latest_resize() -> ResizeSettings {
    BRIDGE.with(|bridge| bridge.borrow().latest.resize.clone())
}
pub fn strings() -> &'static powerkit_core::locale::Strings {
    BRIDGE.with(|bridge| bridge.borrow().latest.ui_language.strings())
}
#[derive(Clone, PartialEq)]
struct AppWindow(HostHandle);
impl AppWindow {
    /// Hands a request to the host, which owns the application state.
    fn send(&self, intent: impl Into<Intent>) {
        self.0.send(intent.into());
    }
    /// A ready render of a recognized mouse; see `Host::mouse_image`.
    fn mouse_image(&self, depot: &'static str, kind: MouseImageKind) -> Option<DeviceImage> {
        self.0.mouse_image(depot, kind)
    }
    fn post(&self, command: Command) {
        self.send(command);
    }
}
/// Debug builds only: `POWERKIT_DEBUG_PAGE` names the page the shell opens
/// on, as a page tag with an optional PowerMouse tab (`power-mouse:ring`,
/// `power-mouse:scrolling`). It exists so a page can be looked at without
/// clicking through to it; release builds ignore the variable.
#[cfg(debug_assertions)]
fn debug_start(page: Page) -> (Page, mouse_page::MouseTab) {
    use mouse_page::MouseTab;
    let Ok(spec) = std::env::var("POWERKIT_DEBUG_PAGE") else {
        return (page, MouseTab::default());
    };
    let (tag, tab) = spec.split_once(':').unwrap_or((&spec, ""));
    let tab = match tab {
        "scrolling" => MouseTab::Scrolling,
        "ring" => MouseTab::Ring,
        _ => MouseTab::Buttons,
    };
    let page = Page::from_tag(tag).unwrap_or(page);
    BRIDGE.with(|bridge| bridge.borrow_mut().page = page);
    (page, tab)
}
#[derive(Clone)]
enum Message {
    Noop,
    Refresh(Box<Snapshot>),
    Navigate(Page),
    Command(Command),
    Toggle(Command, bool),
    Choose(&'static [Command], Option<usize>),
    EditFilename(String),
    ApplyFilename,
    PowerStore(PowerStoreAction),
    PowerStoreFilterText(String),
    MediaRenamer(MediaRenamerAction),
    MediaRenamerTemplate(String),
    MediaRenamerFilter(String),
    PowerMouse(PowerMouseAction),
    EditMouseText(MouseSlot, String),
    ApplyMouseText(MouseSlot),
    /// Switch the PowerMouse page to another tab.
    MouseTab(mouse_page::MouseTab),
    /// Select a trigger (or a ring slot) to edit on the PowerMouse page.
    MouseSelect(powerkit_core::power_mouse::Trigger),
    /// Select which gesture direction row is open in the PowerMouse inspector.
    MouseGestureSlot(powerkit_core::power_mouse::ActionSlot),
    Tray(TrayEdit),
    TrayShown(TrayTool, bool),
    TrayShortcuts(bool),
    OpenDiscoverDialog,
    EditDiscoverQuery(String),
    PickDiscoverSource(Option<usize>),
    SubmitDiscover,
    DiscoverDialogClosed(ContentDialogResult),
    OpenResetDialog,
    ResetDialogClosed(ContentDialogResult),
    EditPowerStoreOptions(OptionsField, String),
    SavePowerStoreOptions,
    ApplyColor(ColorTarget, Rgb),
    ApplyShellOpacity(i32),
    CaptureHotkey(Command),
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
    /// Text typed into a PowerMouse "Type text" action but not applied yet.
    mouse_text_draft: Option<(MouseSlot, String)>,
    /// PowerMouse page: the tab showing, and the trigger and ring slot whose
    /// editors are open.
    mouse_tab: mouse_page::MouseTab,
    mouse_trigger: powerkit_core::power_mouse::Trigger,
    mouse_ring_slot: powerkit_core::power_mouse::Trigger,
    mouse_gesture_slot: powerkit_core::power_mouse::ActionSlot,
    discover_dialog_open: bool,
    discover_query_draft: String,
    discover_source_index: usize,
    /// The Settings page "restore defaults" confirmation is showing.
    reset_dialog_open: bool,
    /// Unsaved options for the package open in the inspector drawer.
    power_store_options_draft: Option<powerkit_core::power_store::InstallOptions>,
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
        #[cfg(debug_assertions)]
        let (page, debug_mouse_tab) = debug_start(page);
        if let Some(seed) = resize_dialog::take_pending() {
            open_resize_window(context, seed);
        }
        Self {
            app: input.clone(),
            page,
            snapshot,
            filename_draft: None,
            applying_filename: false,
            mouse_text_draft: None,
            #[cfg(debug_assertions)]
            mouse_tab: debug_mouse_tab,
            #[cfg(not(debug_assertions))]
            mouse_tab: mouse_page::MouseTab::default(),
            mouse_trigger: powerkit_core::power_mouse::Trigger::Middle,
            mouse_ring_slot: powerkit_core::power_mouse::Trigger::Ring1,
            mouse_gesture_slot: powerkit_core::power_mouse::ActionSlot::Click,
            discover_dialog_open: false,
            discover_query_draft: String::new(),
            discover_source_index: 0,
            reset_dialog_open: false,
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
                let page = if self.snapshot.shows(page) {
                    page
                } else {
                    Page::General
                };
                let entered = page != self.page;
                if entered {
                    self.expiry_date = None;
                    self.expiry_time = None;
                    self.discover_dialog_open = false;
                    self.reset_dialog_open = false;
                }
                self.page = page;
                BRIDGE.with(|bridge| bridge.borrow_mut().page = page);
                if entered && page == Page::PowerStore {
                    self.app.post(Command::PowerStoreEnsureLoaded);
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
                    // WinRT reports both in the 100 ns ticks the pick uses.
                    self.app.send(Intent::Expiry(ExpiryPick {
                        date: self.expiry_date.map(UtcTicks),
                        time_of_day,
                    }));
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
            // A text box also reports the value a refresh just gave it; only
            // a real edit becomes a draft.
            Message::EditMouseText(target, text) => {
                if self.mouse_text(target).as_deref() == Some(text.as_str()) {
                    if self
                        .mouse_text_draft
                        .as_ref()
                        .is_some_and(|(at, _)| *at == target)
                    {
                        self.mouse_text_draft = None;
                    }
                } else {
                    self.mouse_text_draft = Some((target, text));
                }
            }
            Message::ApplyMouseText(target) => {
                if let Some((at, text)) = self.mouse_text_draft.take()
                    && at == target
                {
                    let text: String = text
                        .chars()
                        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
                        .take(powerkit_core::power_mouse::TEXT_MAX_CHARS)
                        .collect();
                    let current = self.snapshot.mouse.binding(target.trigger);
                    let binding =
                        target.replace(&current, powerkit_core::power_mouse::Action::Text(text));
                    self.post_power_mouse(PowerMouseAction::SetBinding {
                        trigger: target.trigger,
                        binding,
                    });
                }
            }
            Message::MouseTab(tab) => self.mouse_tab = tab,
            Message::MouseSelect(trigger) => {
                // An unsaved text belongs to the editor that is being left.
                self.mouse_text_draft = None;
                if trigger.is_ring_slot() {
                    self.mouse_ring_slot = trigger;
                } else {
                    self.mouse_trigger = trigger;
                    self.mouse_gesture_slot = powerkit_core::power_mouse::ActionSlot::Click;
                }
            }
            Message::MouseGestureSlot(slot) => {
                self.mouse_text_draft = None;
                self.mouse_gesture_slot = slot;
            }
            Message::Tray(edit) => self.post_tray(edit),
            // A switch also reports the value a refresh just gave it.
            Message::TrayShown(tool, on) => {
                if self.snapshot.tray.shown(tool) != on {
                    self.post_tray(TrayEdit::Toggle(tool));
                }
            }
            Message::TrayShortcuts(on) => {
                if self.snapshot.tray.shortcuts() != on {
                    self.post_tray(TrayEdit::ToggleShortcuts);
                }
            }
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
            Message::OpenResetDialog => self.reset_dialog_open = true,
            Message::ResetDialogClosed(result) => {
                self.reset_dialog_open = false;
                if result == ContentDialogResult::Primary {
                    self.app.post(Command::ResetGeneralSettings);
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
                    self.applying_filename = true;
                    self.app.send(Intent::ResizeFilename(text));
                }
            }
            Message::ApplyColor(target, color) => {
                self.app.send(match target {
                    ColorTarget::Frame => Intent::FrameColor(color),
                    ColorTarget::Ruler => Intent::RulerColor(color),
                });
            }
            Message::ApplyShellOpacity(percent) => {
                self.app.send(Intent::ShellOpacity(percent));
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
        let theme = match self.snapshot.theme {
            powerkit_core::settings::AppTheme::Light => WindowTheme::Light,
            powerkit_core::settings::AppTheme::Dark => WindowTheme::Dark,
            powerkit_core::settings::AppTheme::System => WindowTheme::System,
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
            .filter(|page| self.snapshot.shows(*page))
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
        } else if self.page == Page::PowerMouse {
            self.power_mouse_pane(context)
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
                    Page::PowerMouse => "power-mouse-pane",
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
    fn t(&self) -> &'static powerkit_core::locale::Strings {
        self.snapshot.ui_language.strings()
    }
    /// Posts a changed custom interval; NumberBox also reports programmatic values.
    fn post_power_store(&self, action: PowerStoreAction) {
        self.app.send(Intent::PowerStore(action));
    }
    fn post_media_renamer(&self, action: MediaRenamerAction) {
        self.app.send(Intent::MediaRenamer(action));
    }
    /// The saved text of the "Type text" action at `target`, if that is what
    /// the slot (or workflow step) currently holds.
    fn mouse_text(&self, target: MouseSlot) -> Option<String> {
        use powerkit_core::power_mouse::Action;
        let binding = self.snapshot.mouse.binding(target.trigger);
        let action = match (binding.slot(target.slot)?, target.step) {
            (action, None) => action,
            (Action::Workflow(steps), Some(step)) => steps.get(usize::from(step))?,
            _ => return None,
        };
        match action {
            Action::Text(text) => Some(text.clone()),
            _ => None,
        }
    }
    fn post_power_mouse(&self, action: PowerMouseAction) {
        self.app.send(Intent::PowerMouse(action));
    }
    fn post_tray(&self, edit: TrayEdit) {
        self.app.send(Intent::Tray(edit));
    }
    fn apply_interval(&self, value: Option<f64>, total: impl FnOnce(i64) -> i64) {
        let Some(value) = value.filter(|value| value.is_finite()) else {
            return;
        };
        let minutes = powerkit_core::always_on::clamp_interval(total(value.round() as i64));
        if minutes != self.snapshot.interval_minutes {
            self.app.send(Intent::Interval(minutes));
        }
    }
    fn toggled(&self, command: Command) -> bool {
        let s = &self.snapshot;
        match command {
            Command::ToggleDisplay => s.display,
            Command::ToggleAlwaysOnWarn => s.always_on_warn,
            Command::ToggleAcOnly => s.ac_only,
            Command::ToggleSound => s.sound,
            Command::ToggleStartup => s.startup,
            Command::ToggleShowTray => s.show_tray,
            Command::ToggleFrameAccent => s.frame_accent,
            Command::ToggleRulerContinuous => s.ruler_continuous,
            Command::ToggleRulerChannel => s.ruler_per_channel,
            Command::ToggleRulerFeet => s.ruler_feet,
            Command::ToggleResizeShrink => s.resize.shrink_only,
            Command::ToggleResizeReplace => s.resize.replace,
            Command::ToggleResizeOrientation => s.resize.ignore_orientation,
            Command::ToggleResizeMetadata => s.resize.remove_metadata,
            Command::ToggleResizeKeepDate => s.resize.keep_date,
            Command::ToggleMouse => s.mouse.enabled,
            Command::ToggleModuleAlwaysOn => s.modules.always_on,
            Command::ToggleModuleAlwaysTop => s.modules.always_top,
            Command::ToggleModuleScreenRuler => s.modules.screen_ruler,
            Command::ToggleModuleImageResizer => s.modules.image_resizer,
            Command::ToggleModulePowerStore => s.modules.power_store,
            Command::ToggleModuleMediaRenamer => s.modules.media_renamer,
            _ => false,
        }
    }
    fn chosen(&self, order: &[Command]) -> Option<usize> {
        let s = &self.snapshot;
        if order.first() == Some(&Command::AlwaysOnOff) {
            return s.mode;
        }
        // Each group holds commands of one kind, so only that kind can match.
        order.iter().position(|item| {
            item.ruler_mode() == Some(s.ruler_mode)
                || item.ruler_unit() == Some(s.ruler_unit)
                || item.resize_encoder() == Some(s.resize.fallback)
                || item.resize_png() == Some(s.resize.png)
                || item.theme() == Some(s.theme)
                || item.language() == Some(s.language)
        })
    }
}
fn page_copy(page: Page, lang: powerkit_core::locale::UiLanguage) -> (&'static str, &'static str) {
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
    labels: &powerkit_core::locale::Strings,
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
fn installed_about_facts(
    labels: &powerkit_core::locale::Strings,
    version: &str,
    packaged: bool,
) -> [(&'static str, String); 5] {
    about_facts(
        labels,
        version,
        target_arch_label(),
        if packaged {
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
        for page in Page::ALL {
            assert_eq!(Page::from_tag(page.tag()), Some(page));
        }
        assert_eq!(Page::from_tag("settings"), Some(Page::Settings));
        assert_eq!(Page::default(), Page::General);
        assert_eq!(RAIL_PAGES[0], Page::General);
        assert!(!RAIL_PAGES.contains(&Page::Settings));
    }
    #[test]
    fn page_copy_follows_language_catalog() {
        let tr = powerkit_core::locale::UiLanguage::Turkish;
        let en = powerkit_core::locale::UiLanguage::English;
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
    fn about_facts_list_version_architecture_channel_and_settings() {
        let tr = powerkit_core::locale::UiLanguage::Turkish.strings();
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
        let installed = installed_about_facts(tr, "4.5.6", false);
        assert_eq!(installed[0].1, "4.5.6");
        assert!(matches!(installed[1].1.as_str(), "ARM64" | "x64" | "diğer"));
        assert_eq!(installed[2].1, tr.channel_unpackaged);
        assert_eq!(installed[3].1, PACKAGE_ID);
        assert!(installed[4].1.contains("PowerKit"));
        assert!(installed[4].1.contains("settings.txt"));
        assert_eq!(
            about_facts(
                powerkit_core::locale::UiLanguage::English.strings(),
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
