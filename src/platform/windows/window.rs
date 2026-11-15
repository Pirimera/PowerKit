//! Settings shell: a WinUI 3 window rendered by windows-reactor.
//!
//! The shell only renders snapshots and reports user intent. Every edit is posted to
//! the hidden Win32 app window as the same WM_COMMAND the tray menu uses, so
//! preference writes, power requests and hotkey ownership stay in `mod.rs`.
use super::{
    DISPLAY, EXIT, FOREVER, HALF_HOUR, HOUR, MEASURE, OFF, PIN, SOUND, TWO_HOURS, UPDATE,
    resize_dialog,
};
use crate::image_resizer::{FallbackEncoder, PngInterlace, ResizeSettings};
use std::{cell::RefCell, io, mem::size_of, path::PathBuf};
use windows_reactor::*;
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        Controls::Dialogs::{CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW},
        WindowsAndMessaging::{PostMessageW, WM_COMMAND},
    },
};
pub(super) const HOTKEY_PIN_EDIT: usize = 40;
pub(super) const HOTKEY_OP_UP_EDIT: usize = 41;
pub(super) const HOTKEY_OP_DOWN_EDIT: usize = 42;
pub(super) const FRAME_ACCENT: usize = 43;
pub(super) const FRAME_COLOR: usize = 44;
pub(super) const FRAME_OPACITY_DOWN: usize = 46;
pub(super) const FRAME_OPACITY_UP: usize = 47;
pub(super) const FRAME_THICK_DOWN: usize = 49;
pub(super) const FRAME_THICK_UP: usize = 50;
pub(super) const RULER_HOTKEY: usize = 53;
pub(super) const RULER_MODE_NONE: usize = 60;
pub(super) const RULER_MODE_BOUNDS: usize = 61;
pub(super) const RULER_MODE_SPACING: usize = 62;
pub(super) const RULER_MODE_HORIZONTAL: usize = 63;
pub(super) const RULER_MODE_VERTICAL: usize = 64;
pub(super) const RULER_CONTINUOUS: usize = 65;
pub(super) const RULER_CHANNEL: usize = 66;
pub(super) const RULER_FEET: usize = 67;
pub(super) const RULER_TOLERANCE_DOWN: usize = 69;
pub(super) const RULER_TOLERANCE_UP: usize = 70;
pub(super) const RULER_COLOR: usize = 71;
pub(super) const RULER_UNIT_PX: usize = 72;
pub(super) const RULER_UNIT_IN: usize = 73;
pub(super) const RULER_UNIT_CM: usize = 74;
pub(super) const RULER_UNIT_MM: usize = 75;
pub(super) const RULER_UNIT_DIP: usize = 76;
pub(super) const RESIZE_OPEN: usize = 120;
pub(super) const RESIZE_SHRINK: usize = 121;
pub(super) const RESIZE_REPLACE: usize = 122;
pub(super) const RESIZE_ORIENTATION: usize = 123;
pub(super) const RESIZE_METADATA: usize = 124;
pub(super) const RESIZE_KEEP_DATE: usize = 125;
pub(super) const RESIZE_JPEG_DOWN: usize = 127;
pub(super) const RESIZE_JPEG_UP: usize = 128;
pub(super) const RESIZE_FILENAME: usize = 130;
pub(super) const RESIZE_ENC_JPEG: usize = 133;
pub(super) const RESIZE_ENC_PNG: usize = 134;
pub(super) const RESIZE_ENC_BMP: usize = 135;
pub(super) const RESIZE_ENC_GIF: usize = 136;
pub(super) const RESIZE_ENC_TIFF: usize = 137;
pub(super) const RESIZE_PNG_DEFAULT: usize = 139;
pub(super) const RESIZE_PNG_ON: usize = 140;
pub(super) const RESIZE_PNG_OFF: usize = 141;
pub(super) const RESIZE_TIFF_DOWN: usize = 143;
pub(super) const RESIZE_TIFF_UP: usize = 144;
pub(super) const MODE_ORDER: [usize; 5] = [OFF, HALF_HOUR, HOUR, TWO_HOURS, FOREVER];
pub(super) const RULER_MODE_ORDER: [usize; 5] = [
    RULER_MODE_NONE,
    RULER_MODE_BOUNDS,
    RULER_MODE_SPACING,
    RULER_MODE_HORIZONTAL,
    RULER_MODE_VERTICAL,
];
pub(super) const RULER_UNIT_ORDER: [usize; 5] = [
    RULER_UNIT_PX,
    RULER_UNIT_IN,
    RULER_UNIT_CM,
    RULER_UNIT_MM,
    RULER_UNIT_DIP,
];
pub(super) const RESIZE_ENCODER_ORDER: [usize; 5] = [
    RESIZE_ENC_JPEG,
    RESIZE_ENC_PNG,
    RESIZE_ENC_BMP,
    RESIZE_ENC_GIF,
    RESIZE_ENC_TIFF,
];
pub(super) const RESIZE_PNG_ORDER: [usize; 3] = [RESIZE_PNG_DEFAULT, RESIZE_PNG_ON, RESIZE_PNG_OFF];
pub(super) const STARTUP: usize = 150;
pub(super) const SHOW_TRAY: usize = 151;
pub(super) const AUTO_UPDATE: usize = 152;
pub(super) const UPDATE_NOTIFY: usize = 153;
pub(super) const SHOW_NOTES: usize = 154;
pub(super) const THEME_SYSTEM: usize = 155;
pub(super) const THEME_LIGHT: usize = 156;
pub(super) const THEME_DARK: usize = 157;
pub(super) const LANG_SYSTEM: usize = 158;
pub(super) const LANG_TR: usize = 159;
pub(super) const OPEN_COLOR_SETTINGS: usize = 160;
pub(super) const LANG_EN: usize = 161;
pub(super) const LANG_AR: usize = 162;
pub(super) const THEME_ORDER: [usize; 3] = [THEME_SYSTEM, THEME_LIGHT, THEME_DARK];
pub(super) const LANG_ORDER: [usize; 4] = [LANG_SYSTEM, LANG_TR, LANG_EN, LANG_AR];
const CLIENT_WIDTH: f64 = 980.0;
const CLIENT_HEIGHT: f64 = 720.0;
const MIN_WIDTH: f64 = 640.0;
const MIN_HEIGHT: f64 = 480.0;
const PACKAGE_ID: &str = "Pirimera.PowerKit";
const LABEL_WINDOW_TITLE: &str = "PowerKit";
const LABEL_STEPPER_DOWN: &str = "−";
const LABEL_STEPPER_UP: &str = "+";
const LABEL_ABOUT_NAME: &str = "PowerKit";
const GLYPH_SYNC: &str = "\u{E72C}";
const GLYPH_STARTUP: &str = "\u{E7E8}";
const GLYPH_LANGUAGE: &str = "\u{E774}";
const GLYPH_THEME: &str = "\u{E790}";
const GLYPH_TRAY: &str = "\u{E7EC}";
const RELEASE_NOTES_URI: &str = "https://github.com/Pirimera/PowerKit/releases";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    General,
    AlwaysOn,
    AlwaysTop,
    ScreenRuler,
    ImageResizer,
}
const PAGES: [Page; 5] = [
    Page::General,
    Page::AlwaysOn,
    Page::AlwaysTop,
    Page::ScreenRuler,
    Page::ImageResizer,
];
impl Page {
    fn tag(self) -> &'static str {
        match self {
            Page::AlwaysOn => "always-on",
            Page::AlwaysTop => "always-top",
            Page::ScreenRuler => "screen-ruler",
            Page::ImageResizer => "image-resizer",
            Page::General => "general",
        }
    }
    fn from_tag(tag: &str) -> Option<Self> {
        PAGES.into_iter().find(|page| page.tag() == tag)
    }
    fn label(self, lang: crate::locale::UiLanguage) -> &'static str {
        let t = lang.strings();
        match self {
            Page::AlwaysOn => t.nav_always_on,
            Page::AlwaysTop => t.nav_always_top,
            Page::ScreenRuler => t.nav_screen_ruler,
            Page::ImageResizer => t.nav_image_resizer,
            Page::General => t.nav_general,
        }
    }
    fn glyph(self) -> &'static str {
        match self {
            Page::AlwaysOn => "\u{E7E8}",
            Page::AlwaysTop => "\u{E718}",
            Page::ScreenRuler => "\u{ED5E}",
            Page::ImageResizer => "\u{E91B}",
            Page::General => "\u{E713}",
        }
    }
}
/// Everything the shell renders, rebuilt by `mod.rs` from its state on each change.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Snapshot {
    pub status: String,
    pub mode: Option<usize>,
    pub display: bool,
    pub top_status: String,
    pub sound: bool,
    pub capturing: Option<usize>,
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
    pub update_status: &'static str,
    pub update_ready: bool,
    pub startup: bool,
    pub theme: usize,
    pub language: usize,
    pub show_tray: bool,
    pub auto_update: bool,
    pub update_notify: bool,
    pub show_notes_after_update: bool,
    pub ui_language: crate::locale::UiLanguage,
    pub hint: String,
}
#[derive(Default)]
struct Bridge {
    sender: Option<LocalSender<Message>>,
    latest: Snapshot,
    page: Page,
    filename: Option<String>,
}
/// Opens the WinUI Image Resizer dialog when the settings shell is running.
/// Returns false when WinUI is unavailable so the caller can use the Win32 dialog.
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
/// The filename draft the user applied, consumed by the RESIZE_FILENAME command.
pub(super) fn take_filename() -> Option<String> {
    BRIDGE.with(|bridge| bridge.borrow_mut().filename.take())
}
/// Runs the WinUI application on this thread until its last window closes.
pub(super) fn run(app: HWND) -> io::Result<()> {
    App::run_component::<Shell>(AppWindow(app as isize))
        .map_err(|error| io::Error::other(error.to_string()))
}
#[derive(Clone, Copy, PartialEq)]
struct AppWindow(isize);
impl AppWindow {
    fn post(self, id: usize) {
        // SAFETY: the app window lives on this UI thread; posting to a destroyed
        // window fails without side effects.
        unsafe {
            PostMessageW(self.0 as HWND, WM_COMMAND, id, 0);
        }
    }
}
#[derive(Clone)]
enum Message {
    Refresh(Box<Snapshot>),
    Navigate(Page),
    Select(Option<String>),
    Command(usize),
    Toggle(usize, bool),
    Choose(&'static [usize], Option<usize>),
    EditFilename(String),
    ApplyFilename,
    OpenResize,
}
struct Shell {
    app: AppWindow,
    page: Page,
    snapshot: Snapshot,
    filename_draft: Option<String>,
    applying_filename: bool,
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
            let _ = context.open_window(View::component::<resize_dialog::Dialog>(seed));
        }
        Self {
            app: *input,
            page,
            snapshot,
            filename_draft: None,
            applying_filename: false,
        }
    }
    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            Message::Refresh(snapshot) => {
                self.snapshot = *snapshot;
                if self.applying_filename {
                    self.applying_filename = false;
                    self.filename_draft = None;
                }
            }
            Message::Navigate(page) => self.page = page,
            Message::Select(tag) => {
                if let Some(page) = tag.as_deref().and_then(Page::from_tag) {
                    self.page = page;
                    BRIDGE.with(|bridge| bridge.borrow_mut().page = page);
                }
            }
            Message::Command(id) => self.app.post(id),
            Message::Toggle(id, on) => {
                if self.toggled(id) != on {
                    self.app.post(id);
                }
            }
            Message::Choose(order, index) => {
                if let Some(&id) = index.and_then(|index| order.get(index))
                    && self.chosen(order) != index
                {
                    self.app.post(id);
                }
            }
            Message::EditFilename(text) => self.filename_draft = Some(text),
            Message::ApplyFilename => {
                if let Some(text) = self.filename_draft.clone() {
                    BRIDGE.with(|bridge| bridge.borrow_mut().filename = Some(text));
                    self.applying_filename = true;
                    self.app.post(RESIZE_FILENAME);
                }
            }
            Message::OpenResize => {
                if let Some(seed) = resize_dialog::take_pending() {
                    let _ = context.open_window(View::component::<resize_dialog::Dialog>(seed));
                }
            }
        }
    }
    fn view(&self, _: &AppWindow, context: &mut ViewContext<Self>) -> View {
        context.window_title(LABEL_WINDOW_TITLE);
        let theme = match self.snapshot.theme {
            THEME_LIGHT => WindowTheme::Light,
            THEME_DARK => WindowTheme::Dark,
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
        let lang = self.snapshot.ui_language;
        let items = PAGES.map(|page| {
            KeyedView::new(
                page.tag(),
                NavigationViewItem::new()
                    .tag(page.tag())
                    .is_selected(page == self.page)
                    .slots([
                        SlotView::new(NavigationViewItemSlot::Content, page.label(lang)),
                        SlotView::new(
                            NavigationViewItemSlot::Icon,
                            FontIcon::new().glyph(page.glyph()),
                        ),
                    ]),
            )
        });
        let content = ScrollViewer::new().content(
            StackPanel::new()
                .max_width(1000.0)
                .margin(Thickness::new(36.0, 28.0, 36.0, 36.0))
                .keyed_children([KeyedView::new(self.page.tag(), self.page_view(context))]),
        );
        NavigationView::new()
            .pane_display_mode(NavigationViewPaneDisplayMode::Left)
            .is_settings_visible(false)
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
            .on_selected_tag_changed(context.callback(Message::Select))
            .slots([
                SlotView::collection(NavigationViewSlot::MenuItems, items),
                SlotView::new(NavigationViewSlot::Content, content),
            ])
    }
}
impl Shell {
    fn t(&self) -> &'static crate::locale::Strings {
        self.snapshot.ui_language.strings()
    }
    fn toggled(&self, id: usize) -> bool {
        let s = &self.snapshot;
        match id {
            DISPLAY => s.display,
            SOUND => s.sound,
            STARTUP => s.startup,
            SHOW_TRAY => s.show_tray,
            AUTO_UPDATE => s.auto_update,
            UPDATE_NOTIFY => s.update_notify,
            SHOW_NOTES => s.show_notes_after_update,
            FRAME_ACCENT => s.frame_accent,
            RULER_CONTINUOUS => s.ruler_continuous,
            RULER_CHANNEL => s.ruler_per_channel,
            RULER_FEET => s.ruler_feet,
            RESIZE_SHRINK => s.resize.shrink_only,
            RESIZE_REPLACE => s.resize.replace,
            RESIZE_ORIENTATION => s.resize.ignore_orientation,
            RESIZE_METADATA => s.resize.remove_metadata,
            RESIZE_KEEP_DATE => s.resize.keep_date,
            _ => false,
        }
    }
    fn chosen(&self, order: &[usize]) -> Option<usize> {
        let s = &self.snapshot;
        let id = match order.first().copied() {
            Some(OFF) => return s.mode,
            Some(RULER_MODE_NONE) => s.ruler_mode,
            Some(RULER_UNIT_PX) => s.ruler_unit,
            Some(RESIZE_ENC_JPEG) => encoder_button(s.resize.fallback),
            Some(RESIZE_PNG_DEFAULT) => png_button(s.resize.png),
            Some(THEME_SYSTEM) => s.theme,
            Some(LANG_SYSTEM) => s.language,
            _ => return None,
        };
        order.iter().position(|item| *item == id)
    }
    fn page_view(&self, context: &ViewContext<Self>) -> View {
        let (title, lead) = page_copy(self.page, self.snapshot.ui_language);
        let mut rows = vec![
            heading(title, 28.0).into(),
            TextBlock::new()
                .text(lead)
                .text_wrapping(TextWrapping::Wrap)
                .opacity(0.8)
                .margin(Thickness::new(0.0, 0.0, 0.0, 12.0))
                .into(),
        ];
        rows.extend(match self.page {
            Page::AlwaysOn => self.always_on(context),
            Page::AlwaysTop => self.always_top(context),
            Page::ScreenRuler => self.screen_ruler(context),
            Page::ImageResizer => self.image_resizer(context),
            Page::General => self.general(context),
        });
        stack(rows, 8.0)
    }
    fn always_on(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let t = self.t();
        vec![
            caption(&s.status),
            wide_card(
                t.mode,
                self.choices(
                    context,
                    &MODE_ORDER,
                    [
                        t.mode_off,
                        t.mode_half,
                        t.mode_hour,
                        t.mode_two,
                        t.mode_forever,
                    ],
                ),
            ),
            self.toggle_card(context, DISPLAY, t.display),
        ]
    }
    fn always_top(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let t = self.t();
        vec![
            described_card(
                t.pin,
                &s.top_status,
                Button::new()
                    .on_click(context.message(Message::Command(PIN)))
                    .content(t.pin_action),
            ),
            self.toggle_card(context, SOUND, t.sound),
            self.hotkey_card(context, HOTKEY_PIN_EDIT, t.hotkey_pin, &s.pin_hotkey),
            self.hotkey_card(
                context,
                HOTKEY_OP_UP_EDIT,
                t.hotkey_op_up,
                &s.opacity_up_hotkey,
            ),
            self.hotkey_card(
                context,
                HOTKEY_OP_DOWN_EDIT,
                t.hotkey_op_down,
                &s.opacity_down_hotkey,
            ),
            self.toggle_card(context, FRAME_ACCENT, t.frame_accent),
            card(
                t.frame_color,
                color_button(context, FRAME_COLOR, s.frame_color),
            ),
            stepper_card(
                context,
                t.frame_opacity,
                format!("%{}", s.frame_opacity),
                FRAME_OPACITY_DOWN,
                FRAME_OPACITY_UP,
            ),
            stepper_card(
                context,
                t.frame_thickness,
                format!("{} px", s.frame_thickness),
                FRAME_THICK_DOWN,
                FRAME_THICK_UP,
            ),
            caption(t.always_top_lead),
        ]
    }
    fn screen_ruler(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let t = self.t();
        let measure = Button::new()
            .style(ButtonStyle::Accent)
            .on_click(context.message(Message::Command(MEASURE)))
            .content(if s.ruler_active {
                t.ruler_stop
            } else {
                t.ruler_start
            });
        let status = if s.ruler_active {
            t.ruler_on
        } else {
            t.ruler_off
        };
        vec![
            described_card(t.ruler_title, status, measure),
            self.hotkey_card(context, RULER_HOTKEY, t.hotkey_pin, &s.ruler_hotkey),
            wide_card(
                t.ruler_mode,
                self.choices(
                    context,
                    &RULER_MODE_ORDER,
                    [
                        t.ruler_none,
                        t.ruler_bounds,
                        t.ruler_spacing,
                        t.ruler_horizontal,
                        t.ruler_vertical,
                    ],
                ),
            ),
            wide_card(
                t.ruler_unit,
                self.choices(
                    context,
                    &RULER_UNIT_ORDER,
                    [t.ruler_px, t.ruler_in, t.ruler_cm, t.ruler_mm, t.ruler_dip],
                ),
            ),
            self.toggle_card(context, RULER_CONTINUOUS, t.ruler_continuous),
            self.toggle_card(context, RULER_CHANNEL, t.ruler_channel),
            self.toggle_card(context, RULER_FEET, t.ruler_feet),
            stepper_card(
                context,
                t.ruler_tolerance,
                s.ruler_tolerance.to_string(),
                RULER_TOLERANCE_DOWN,
                RULER_TOLERANCE_UP,
            ),
            card(
                t.ruler_color,
                color_button(context, RULER_COLOR, s.ruler_color),
            ),
            caption(t.ruler_lead),
        ]
    }
    fn image_resizer(&self, context: &ViewContext<Self>) -> Vec<View> {
        let resize = &self.snapshot.resize;
        let t = self.t();
        let filename = self
            .filename_draft
            .clone()
            .unwrap_or_else(|| resize.filename.clone());
        let filename_row = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(8.0)
            .children((
                TextBox::new()
                    .grid_column(0)
                    .text(filename)
                    .on_text_changed(context.callback(Message::EditFilename)),
                Button::new()
                    .grid_column(1)
                    .is_enabled(self.filename_draft.is_some())
                    .on_click(context.message(Message::ApplyFilename))
                    .content(t.resize_file_apply),
            ));
        vec![
            card(
                t.resize_open,
                Button::new()
                    .style(ButtonStyle::Accent)
                    .on_click(context.message(Message::Command(RESIZE_OPEN)))
                    .content(t.resize_open),
            ),
            self.toggle_card(context, RESIZE_SHRINK, t.resize_shrink),
            self.toggle_card(context, RESIZE_REPLACE, t.resize_replace),
            self.toggle_card(context, RESIZE_ORIENTATION, t.resize_orientation),
            self.toggle_card(context, RESIZE_METADATA, t.resize_metadata),
            self.toggle_card(context, RESIZE_KEEP_DATE, t.resize_keep_date),
            stepper_card(
                context,
                t.resize_jpeg,
                resize.jpeg_quality.to_string(),
                RESIZE_JPEG_DOWN,
                RESIZE_JPEG_UP,
            ),
            stepper_card(
                context,
                t.resize_tiff,
                resize.tiff.label().to_string(),
                RESIZE_TIFF_DOWN,
                RESIZE_TIFF_UP,
            ),
            wide_card(
                t.resize_file,
                stack(vec![filename_row, caption(t.resize_file_hint)], 6.0),
            ),
            wide_card(
                t.resize_encoder,
                self.choices(
                    context,
                    &RESIZE_ENCODER_ORDER,
                    [
                        FallbackEncoder::Jpeg.label(),
                        FallbackEncoder::Png.label(),
                        FallbackEncoder::Bmp.label(),
                        FallbackEncoder::Gif.label(),
                        FallbackEncoder::Tiff.label(),
                    ],
                ),
            ),
            wide_card(
                t.resize_png,
                self.choices(
                    context,
                    &RESIZE_PNG_ORDER,
                    [t.png_default, t.png_on, t.png_off],
                ),
            ),
        ]
    }
    fn general(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let t = self.t();
        let update_label = if s.update_ready {
            t.update_ready
        } else {
            t.update_check
        };
        let version = format!("v{}", env!("CARGO_PKG_VERSION"));
        let notes: View = match HyperlinkButton::new().navigate_uri(RELEASE_NOTES_URI) {
            Ok(button) => button
                .horizontal_alignment(HorizontalAlignment::Left)
                .content(
                    TextBlock::new()
                        .text(t.release_notes)
                        .text_wrapping(TextWrapping::Wrap),
                ),
            Err(_) => TextBlock::new()
                .text(t.release_notes)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        };
        // Stack the action under the version block so narrow panes do not clip the notes link.
        let update_header = StackPanel::new().spacing(10.0).children((
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR])
                .column_spacing(12.0)
                .children((
                    FontIcon::new()
                        .grid_column(0)
                        .glyph(GLYPH_SYNC)
                        .vertical_alignment(VerticalAlignment::Top)
                        .margin(Thickness::new(0.0, 4.0, 0.0, 0.0)),
                    StackPanel::new().grid_column(1).spacing(2.0).children((
                        TextBlock::new()
                            .text(version)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .text_wrapping(TextWrapping::Wrap),
                        notes,
                        TextBlock::new()
                            .text(s.update_status)
                            .opacity(0.8)
                            .text_wrapping(TextWrapping::WrapWholeWords),
                    )),
                )),
            Button::new()
                .style(ButtonStyle::Accent)
                .horizontal_alignment(HorizontalAlignment::Left)
                .on_click(context.message(Message::Command(UPDATE)))
                .content(update_label),
        ));
        let update_options = stack(
            vec![
                checkbox(
                    self,
                    context,
                    AUTO_UPDATE,
                    t.auto_update,
                    Some(t.auto_update_hint),
                ),
                checkbox(self, context, UPDATE_NOTIFY, t.update_notify, None),
                checkbox(self, context, SHOW_NOTES, t.show_notes, None),
            ],
            8.0,
        );
        let updates = Expander::new().is_expanded(true).slots([
            SlotView::new(ExpanderSlot::Header, update_header),
            SlotView::new(ExpanderSlot::Content, update_options),
        ]);
        let mut about = vec![heading(LABEL_ABOUT_NAME, 20.0).into(), caption(t.about_sub)];
        about.extend(installed_about_facts(t).map(|(name, value)| {
            Grid::new()
                .columns([GridLength::Pixel(96.0), GridLength::STAR])
                .column_spacing(12.0)
                .children((
                    TextBlock::new().grid_column(0).text(name).opacity(0.8),
                    TextBlock::new()
                        .grid_column(1)
                        .text(value)
                        .is_text_selection_enabled(true)
                        .text_wrapping(TextWrapping::Wrap),
                ))
        }));
        let language = ComboBox::new()
            .min_width(180.0)
            .items_source([t.lang_system, t.lang_tr, t.lang_en, t.lang_ar])
            .selected_index(self.chosen(&LANG_ORDER))
            .on_selection_changed(
                context.callback(move |index: Option<usize>| Message::Choose(&LANG_ORDER, index)),
            );
        let theme = ComboBox::new()
            .min_width(180.0)
            .items_source([t.theme_system, t.theme_light, t.theme_dark])
            .selected_index(self.chosen(&THEME_ORDER))
            .on_selection_changed(
                context.callback(move |index: Option<usize>| Message::Choose(&THEME_ORDER, index)),
            );
        let color_link = HyperlinkButton::new()
            .on_click(context.message(Message::Command(OPEN_COLOR_SETTINGS)))
            .content(t.color_settings);
        vec![
            section(t.general_updates),
            updates,
            section(t.general_startup),
            icon_toggle_card(
                self,
                context,
                GLYPH_STARTUP,
                t.startup,
                t.startup_hint,
                STARTUP,
            ),
            section(t.general_appearance),
            icon_described_card(
                GLYPH_LANGUAGE,
                t.language,
                caption(t.language_hint),
                language,
            ),
            icon_described_card(GLYPH_THEME, t.theme, color_link, theme),
            icon_toggle_card(
                self,
                context,
                GLYPH_TRAY,
                t.show_tray,
                t.show_tray_hint,
                SHOW_TRAY,
            ),
            section(t.general_session),
            surface(stack(about, 6.0)),
            card(
                &s.hint,
                Button::new()
                    .on_click(context.message(Message::Command(EXIT)))
                    .content(t.exit),
            ),
        ]
    }
    fn toggle_card(&self, context: &ViewContext<Self>, id: usize, label: &str) -> View {
        let toggle = ToggleSwitch::new()
            .is_on(self.toggled(id))
            .on_toggled(context.callback(move |on| Message::Toggle(id, on)))
            .slots([
                SlotView::new(ToggleSwitchSlot::OnContent, self.t().on),
                SlotView::new(ToggleSwitchSlot::OffContent, self.t().off),
            ]);
        card(label, toggle)
    }
    fn hotkey_card(
        &self,
        context: &ViewContext<Self>,
        id: usize,
        label: &str,
        hotkey: &str,
    ) -> View {
        let capturing = self.snapshot.capturing == Some(id);
        let button = Button::new()
            .style(capturing.then_some(ButtonStyle::Accent))
            .on_click(context.message(Message::Command(id)))
            .content(if capturing {
                self.t().hotkey_capture
            } else {
                hotkey
            });
        card(label, button)
    }
    fn choices<const N: usize>(
        &self,
        context: &ViewContext<Self>,
        order: &'static [usize; N],
        labels: [&str; N],
    ) -> View {
        RadioButtons::new()
            .items_source(labels)
            .selected_index(self.chosen(order))
            .on_selection_changed(
                context.callback(move |index: Option<usize>| Message::Choose(order, index)),
            )
            .into()
    }
}
fn heading(text: &str, size: f64) -> TextBlock {
    TextBlock::new()
        .text(text)
        .font_size(size)
        .font_weight(FontWeight::SEMI_BOLD)
        .text_wrapping(TextWrapping::Wrap)
}
fn caption(text: &str) -> View {
    TextBlock::new()
        .text(text)
        .text_wrapping(TextWrapping::Wrap)
        .opacity(0.8)
        .into()
}
fn section(text: &str) -> View {
    TextBlock::new()
        .text(text)
        .font_weight(FontWeight::SEMI_BOLD)
        .margin(Thickness::new(0.0, 16.0, 0.0, 4.0))
        .into()
}
fn stack(rows: Vec<View>, spacing: f64) -> View {
    StackPanel::new().spacing(spacing).keyed_children(
        rows.into_iter()
            .enumerate()
            .map(|(index, row)| KeyedView::new(index, row)),
    )
}
fn surface(content: View) -> View {
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(CornerRadius::uniform(8.0))
        .padding(Thickness::xy(16.0, 14.0))
        .content(content)
}
fn card(title: &str, control: impl Into<View>) -> View {
    row(
        TextBlock::new()
            .text(title)
            .text_wrapping(TextWrapping::Wrap)
            .into(),
        control.into(),
    )
}
fn described_card(title: &str, description: &str, control: impl Into<View>) -> View {
    let labels = stack(
        vec![
            TextBlock::new()
                .text(title)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
            caption(description),
        ],
        2.0,
    );
    row(labels, control.into())
}
fn checkbox(
    shell: &Shell,
    context: &ViewContext<Shell>,
    id: usize,
    title: &str,
    hint: Option<&str>,
) -> View {
    let mut labels = vec![
        TextBlock::new()
            .text(title)
            .text_wrapping(TextWrapping::Wrap)
            .into(),
    ];
    if let Some(hint) = hint {
        labels.push(caption(hint));
    }
    CheckBox::new()
        .is_checked(shell.toggled(id))
        .on_is_checked_changed(context.callback(move |on| Message::Toggle(id, on)))
        .content(stack(labels, 2.0))
}
fn icon_toggle_card(
    shell: &Shell,
    context: &ViewContext<Shell>,
    glyph: &str,
    title: &str,
    description: &str,
    id: usize,
) -> View {
    let toggle = ToggleSwitch::new()
        .is_on(shell.toggled(id))
        .on_toggled(context.callback(move |on| Message::Toggle(id, on)))
        .slots([
            SlotView::new(ToggleSwitchSlot::OnContent, shell.t().on),
            SlotView::new(ToggleSwitchSlot::OffContent, shell.t().off),
        ]);
    icon_described_card(glyph, title, caption(description), toggle)
}
fn icon_described_card(
    glyph: &str,
    title: &str,
    description: View,
    control: impl Into<View>,
) -> View {
    let labels = stack(
        vec![
            TextBlock::new()
                .text(title)
                .text_wrapping(TextWrapping::Wrap)
                .into(),
            description,
        ],
        2.0,
    );
    surface(
        Grid::new()
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .children((
                FontIcon::new()
                    .grid_column(0)
                    .glyph(glyph)
                    .vertical_alignment(VerticalAlignment::Center),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((labels,)),
                StackPanel::new()
                    .grid_column(2)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((control.into(),)),
            )),
    )
}
fn row(labels: View, control: View) -> View {
    surface(
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(16.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((labels,)),
                StackPanel::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((control,)),
            )),
    )
}
fn wide_card(title: &str, control: View) -> View {
    surface(stack(
        vec![TextBlock::new().text(title).into(), control],
        8.0,
    ))
}
fn stepper_card(
    context: &ViewContext<Shell>,
    title: &str,
    value: String,
    down: usize,
    up: usize,
) -> View {
    let control = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .children((
            Button::new()
                .width(40.0)
                .on_click(context.message(Message::Command(down)))
                .content(LABEL_STEPPER_DOWN),
            TextBlock::new()
                .text(value)
                .min_width(64.0)
                .vertical_alignment(VerticalAlignment::Center)
                .horizontal_alignment(HorizontalAlignment::Center),
            Button::new()
                .width(40.0)
                .on_click(context.message(Message::Command(up)))
                .content(LABEL_STEPPER_UP),
        ));
    card(title, control)
}
fn color_button(context: &ViewContext<Shell>, id: usize, colorref: u32) -> View {
    let [r, g, b, _] = colorref.to_le_bytes();
    Button::new()
        .on_click(context.message(Message::Command(id)))
        .content(
            Border::new()
                .width(48.0)
                .height(20.0)
                .corner_radius(CornerRadius::uniform(4.0))
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(Thickness::uniform(1.0))
                .background(Color::rgb(r, g, b)),
        )
}
pub(super) fn ruler_mode(id: usize) -> Option<crate::screen_ruler::Mode> {
    Some(match id {
        RULER_MODE_NONE => crate::screen_ruler::Mode::None,
        RULER_MODE_BOUNDS => crate::screen_ruler::Mode::Bounds,
        RULER_MODE_SPACING => crate::screen_ruler::Mode::Spacing,
        RULER_MODE_HORIZONTAL => crate::screen_ruler::Mode::Horizontal,
        RULER_MODE_VERTICAL => crate::screen_ruler::Mode::Vertical,
        _ => return None,
    })
}
pub(super) fn ruler_unit(id: usize) -> Option<crate::screen_ruler::Unit> {
    Some(match id {
        RULER_UNIT_PX => crate::screen_ruler::Unit::Pixel,
        RULER_UNIT_IN => crate::screen_ruler::Unit::Inch,
        RULER_UNIT_CM => crate::screen_ruler::Unit::Centimetre,
        RULER_UNIT_MM => crate::screen_ruler::Unit::Millimetre,
        RULER_UNIT_DIP => crate::screen_ruler::Unit::Dip,
        _ => return None,
    })
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
pub(super) fn theme_from_id(id: usize) -> Option<crate::settings::AppTheme> {
    Some(match id {
        THEME_SYSTEM => crate::settings::AppTheme::System,
        THEME_LIGHT => crate::settings::AppTheme::Light,
        THEME_DARK => crate::settings::AppTheme::Dark,
        _ => return None,
    })
}
pub(super) fn language_id(language: crate::settings::AppLanguage) -> usize {
    match language {
        crate::settings::AppLanguage::System => LANG_SYSTEM,
        crate::settings::AppLanguage::Turkish => LANG_TR,
        crate::settings::AppLanguage::English => LANG_EN,
        crate::settings::AppLanguage::Arabic => LANG_AR,
    }
}
pub(super) fn language_from_id(id: usize) -> Option<crate::settings::AppLanguage> {
    Some(match id {
        LANG_SYSTEM => crate::settings::AppLanguage::System,
        LANG_TR => crate::settings::AppLanguage::Turkish,
        LANG_EN => crate::settings::AppLanguage::English,
        LANG_AR => crate::settings::AppLanguage::Arabic,
        _ => return None,
    })
}
pub(super) fn resize_encoder(id: usize) -> Option<FallbackEncoder> {
    Some(match id {
        RESIZE_ENC_JPEG => FallbackEncoder::Jpeg,
        RESIZE_ENC_PNG => FallbackEncoder::Png,
        RESIZE_ENC_BMP => FallbackEncoder::Bmp,
        RESIZE_ENC_GIF => FallbackEncoder::Gif,
        RESIZE_ENC_TIFF => FallbackEncoder::Tiff,
        _ => return None,
    })
}
pub(super) fn resize_png(id: usize) -> Option<PngInterlace> {
    Some(match id {
        RESIZE_PNG_DEFAULT => PngInterlace::Default,
        RESIZE_PNG_ON => PngInterlace::On,
        RESIZE_PNG_OFF => PngInterlace::Off,
        _ => return None,
    })
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
/// Index of the AlwaysOn session in `MODE_ORDER`; `None` for a timed session the
/// user did not start from one of the listed durations.
pub(super) fn mode_index(mode: crate::always_on::Mode, timed: Option<usize>) -> Option<usize> {
    let id = match mode {
        crate::always_on::Mode::Off => OFF,
        crate::always_on::Mode::Indefinite => FOREVER,
        crate::always_on::Mode::Until(_) => timed?,
    };
    MODE_ORDER.iter().position(|item| *item == id)
}
pub(super) fn pick_color(owner: HWND, current: u32) -> Option<u32> {
    thread_local! {
        static CUSTOM: RefCell<[u32; 16]> = const { RefCell::new([0; 16]) };
    }
    CUSTOM.with(|colors| {
        let mut colors = colors.borrow_mut();
        let mut dialog = unsafe { std::mem::zeroed::<CHOOSECOLORW>() };
        dialog.lStructSize = size_of::<CHOOSECOLORW>() as u32;
        dialog.hwndOwner = owner;
        dialog.rgbResult = current;
        dialog.lpCustColors = colors.as_mut_ptr();
        dialog.Flags = CC_RGBINIT | CC_FULLOPEN;
        // SAFETY: dialog and the 16 custom colors stay alive for this modal call.
        (unsafe { ChooseColorW(&mut dialog) } != 0).then_some(dialog.rgbResult)
    })
}
fn page_copy(page: Page, lang: crate::locale::UiLanguage) -> (&'static str, &'static str) {
    let t = lang.strings();
    match page {
        Page::General => (t.general_title, t.general_lead),
        Page::AlwaysOn => (t.always_on_title, t.always_on_lead),
        Page::AlwaysTop => (t.always_top_title, t.always_top_lead),
        Page::ScreenRuler => (t.ruler_title, t.ruler_page_lead),
        Page::ImageResizer => (t.resize_title, t.resize_lead),
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
        crate::updates::channel(),
        PACKAGE_ID,
        &settings_location_label(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_round_trip_through_navigation_tags() {
        for page in PAGES {
            assert_eq!(Page::from_tag(page.tag()), Some(page));
        }
        assert_eq!(Page::from_tag("settings"), None);
        assert_eq!(Page::default(), Page::General);
        assert_eq!(PAGES[0], Page::General);
    }
    #[test]
    fn page_copy_follows_language_catalog() {
        let tr = crate::locale::UiLanguage::Turkish;
        let en = crate::locale::UiLanguage::English;
        assert_eq!(page_copy(Page::General, tr).0, "Genel");
        assert_eq!(page_copy(Page::General, en).0, "General");
        assert_eq!(page_copy(Page::AlwaysOn, tr).0, "AlwaysOn");
        assert_eq!(page_copy(Page::AlwaysTop, tr).0, "AlwaysTop");
        assert_eq!(page_copy(Page::ScreenRuler, tr).0, "ScreenRuler");
        assert_eq!(page_copy(Page::ImageResizer, tr).0, "ImageResizer");
        assert_eq!(Page::General.label(tr), "Genel");
        assert_eq!(Page::General.label(en), "General");
    }
    #[test]
    fn choice_groups_map_back_to_their_settings() {
        assert_eq!(MODE_ORDER, [OFF, HALF_HOUR, HOUR, TWO_HOURS, FOREVER]);
        for id in RULER_MODE_ORDER {
            assert_eq!(ruler_mode(id).map(mode_id), Some(id));
        }
        for id in RULER_UNIT_ORDER {
            assert_eq!(ruler_unit(id).map(unit_id), Some(id));
        }
        for id in RESIZE_ENCODER_ORDER {
            assert_eq!(resize_encoder(id).map(encoder_button), Some(id));
        }
        for id in RESIZE_PNG_ORDER {
            assert_eq!(resize_png(id).map(png_button), Some(id));
        }
        assert_eq!(ruler_mode(FOREVER), None);
        assert_eq!(resize_png(RESIZE_ENC_JPEG), None);
    }
    #[test]
    fn mode_index_follows_the_running_session() {
        use crate::always_on::Mode;
        assert_eq!(mode_index(Mode::Off, None), Some(0));
        assert_eq!(mode_index(Mode::Indefinite, None), Some(4));
        assert_eq!(mode_index(Mode::Until(5), Some(HOUR)), Some(2));
        assert_eq!(mode_index(Mode::Until(5), None), None);
        assert_eq!(mode_index(Mode::Until(5), Some(DISPLAY)), None);
    }
    #[test]
    fn about_facts_list_version_architecture_channel_and_settings() {
        let tr = crate::locale::UiLanguage::Turkish.strings();
        let rows = about_facts(
            tr,
            "1.2.3",
            "ARM64",
            "win-arm64",
            "Pirimera.PowerKit",
            r"C:\data\settings.txt",
        );
        assert_eq!(
            rows.each_ref().map(|row| row.0),
            ["Sürüm", "Mimari", "Kanal", "Paket", "Ayarlar"]
        );
        assert_eq!(rows[0].1, "1.2.3");
        assert_eq!(rows[1].1, "ARM64");
        assert_eq!(rows[2].1, "win-arm64");
        assert_eq!(rows[3].1, "Pirimera.PowerKit");
        assert_eq!(rows[4].1, r"C:\data\settings.txt");
        let installed = installed_about_facts(tr);
        assert_eq!(installed[0].1, env!("CARGO_PKG_VERSION"));
        assert!(matches!(installed[1].1.as_str(), "ARM64" | "x64" | "diğer"));
        assert!(installed[2].1 == "win-arm64" || installed[2].1 == "win-x64");
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
