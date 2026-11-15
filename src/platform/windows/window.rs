//! Settings shell: a WinUI 3 window rendered by windows-reactor.
//!
//! The shell only renders snapshots and reports user intent. Every edit is posted to
//! the hidden Win32 app window as the same WM_COMMAND the tray menu uses, so
//! preference writes, power requests and hotkey ownership stay in `mod.rs`.
use super::{
    DISPLAY, EXIT, FOREVER, HALF_HOUR, HOUR, LABEL_DISPLAY, LABEL_EXIT, LABEL_UPDATE_CHECK,
    MEASURE, OFF, PIN, SOUND, TWO_HOURS, UPDATE, resize_dialog,
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
const CLIENT_WIDTH: f64 = 980.0;
const CLIENT_HEIGHT: f64 = 720.0;
const MIN_WIDTH: f64 = 640.0;
const MIN_HEIGHT: f64 = 480.0;
const PACKAGE_ID: &str = "Pirimera.PowerKit";
const LABEL_WINDOW_TITLE: &str = "PowerKit";
const LABEL_NAV_GENERAL: &str = "Genel";
const LABEL_NAV_ALWAYS_ON: &str = "AlwaysOn";
const LABEL_NAV_ALWAYS_TOP: &str = "AlwaysTop";
const LABEL_NAV_SCREEN_RULER: &str = "ScreenRuler";
const LABEL_NAV_IMAGE_RESIZER: &str = "ImageResizer";
const LABEL_ON: &str = "Açık";
const LABEL_OFF: &str = "Kapalı";
const LABEL_PIN: &str = "Son pencereyi üstte tut";
const LABEL_PIN_ACTION: &str = "Sabitle veya bırak";
const LABEL_SOUND: &str = "Ses çal";
const LABEL_MODE_OFF: &str = "AlwaysOn kapalı";
const LABEL_MODE_HALF: &str = "30 dakika";
const LABEL_MODE_HOUR: &str = "1 saat";
const LABEL_MODE_TWO: &str = "2 saat";
const LABEL_MODE_FOREVER: &str = "Süresiz";
const LABEL_MODE: &str = "Oturum";
const LABEL_HOTKEY_PIN: &str = "Etkinleştirme kısayolu";
const LABEL_HOTKEY_OP_UP: &str = "Saydamlığı artır";
const LABEL_HOTKEY_OP_DOWN: &str = "Saydamlığı azalt";
const LABEL_HOTKEY_CAPTURE: &str = "Yeni kısayola basın…";
const LABEL_FRAME_ACCENT: &str = "Sistem vurgu rengini kullan";
const LABEL_FRAME_COLOR: &str = "Kenarlık rengi";
const LABEL_FRAME_OPACITY: &str = "Kenarlık opaklığı";
const LABEL_FRAME_THICKNESS: &str = "Kenarlık kalınlığı";
const LABEL_STEPPER_DOWN: &str = "−";
const LABEL_STEPPER_UP: &str = "+";
const LABEL_GENERAL_TITLE: &str = "Genel";
const LABEL_GENERAL_LEAD: &str = "Sürüm bilgileri, güncellemeler ve uygulamadan çıkış.";
const LABEL_ABOUT_NAME: &str = "PowerKit";
const LABEL_ABOUT_SUB: &str = "AlwaysOn · AlwaysTop · ScreenRuler · ImageResizer";
const LABEL_GENERAL_UPDATES: &str = "Güncellemeler";
const LABEL_GENERAL_SESSION: &str = "Uygulama";
const LABEL_UPDATE_READY: &str = "Güncelle ve yeniden başlat";
const LABEL_ALWAYS_ON_TITLE: &str = "AlwaysOn";
const LABEL_ALWAYS_ON_LEAD: &str =
    "Sistemi uyanık tutar. İsterseniz boşta ekran uykusunu da engeller.";
const LABEL_ALWAYS_TOP_TITLE: &str = "AlwaysTop";
const LABEL_ALWAYS_TOP_LEAD: &str =
    "Odaklanan pencereleri üstte tutar. Pinler ve saydamlık kaydedilmez.";
const LABEL_TOP_LEAD: &str =
    "Kenarlık ve kısayollar kaydedilir. Sabitlenen pencerenin saydamlığı kaydedilmez.";
const LABEL_RULER_TITLE: &str = "ScreenRuler";
const LABEL_RULER_PAGE_LEAD: &str =
    "Ekrandaki pikselleri ölçer. Varsayılan kısayol Win+Ctrl+Shift+M.";
const LABEL_RULER_ON: &str = "ScreenRuler açık";
const LABEL_RULER_OFF: &str = "ScreenRuler kapalı";
const LABEL_RULER_START: &str = "Ölçümü başlat";
const LABEL_RULER_STOP: &str = "Ölçümü kapat";
const LABEL_RULER_MODE: &str = "Varsayılan kip";
const LABEL_RULER_NONE: &str = "Yok";
const LABEL_RULER_BOUNDS: &str = "Sınırlar";
const LABEL_RULER_SPACING: &str = "Aralık";
const LABEL_RULER_HORIZONTAL: &str = "Yatay";
const LABEL_RULER_VERTICAL: &str = "Dikey";
const LABEL_RULER_CONTINUOUS: &str = "Ölçerken ekranı sürekli yakala";
const LABEL_RULER_CHANNEL: &str = "Renk kanalı başına kenar algılama";
const LABEL_RULER_FEET: &str = "Çapraz çizgilere ayak çiz";
const LABEL_RULER_TOLERANCE: &str = "Piksel toleransı";
const LABEL_RULER_UNIT: &str = "Birim";
const LABEL_RULER_PX: &str = "Yalnızca piksel";
const LABEL_RULER_IN: &str = "İnç";
const LABEL_RULER_CM: &str = "Santimetre";
const LABEL_RULER_MM: &str = "Milimetre";
const LABEL_RULER_DIP: &str = "DIP";
const LABEL_RULER_COLOR: &str = "Çizgi rengi";
const LABEL_RULER_LEAD: &str = "Shift ölçümü yerinde tutar. Tekerlek toleransı 15 değiştirir. Esc kapatır. Ctrl+1–4 kip seçer.";
const LABEL_RESIZE_TITLE: &str = "ImageResizer";
const LABEL_RESIZE_LEAD: &str = "Küçük, Orta, Büyük ve Telefon boyutları PowerToys ile aynıdır. Yazmadan önce önizleme gerekir.";
const LABEL_RESIZE_OPEN: &str = "Resimleri seç";
const LABEL_RESIZE_SHRINK: &str = "Yalnızca küçült, büyütme";
const LABEL_RESIZE_REPLACE: &str = "Özgün resimlerin üzerine yaz";
const LABEL_RESIZE_ORIENTATION: &str = "Yönü yoksay";
const LABEL_RESIZE_METADATA: &str = "Görüntüyü etkilemeyen veriyi sil";
const LABEL_RESIZE_KEEP_DATE: &str = "Değiştirilme tarihini koru";
const LABEL_RESIZE_JPEG: &str = "JPEG kalitesi";
const LABEL_RESIZE_FILE: &str = "Dosya adı";
const LABEL_RESIZE_FILE_HINT: &str = "%1 ad · %2 boyut · %3–%4 seçilen · %5–%6 sonuç";
const LABEL_RESIZE_FILE_APPLY: &str = "Uygula";
const LABEL_RESIZE_ENCODER: &str = "Yedek kodlayıcı";
const LABEL_RESIZE_PNG: &str = "PNG geçişim";
const LABEL_RESIZE_TIFF: &str = "TIFF sıkıştırma";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    AlwaysOn,
    AlwaysTop,
    ScreenRuler,
    ImageResizer,
    General,
}
const PAGES: [Page; 5] = [
    Page::AlwaysOn,
    Page::AlwaysTop,
    Page::ScreenRuler,
    Page::ImageResizer,
    Page::General,
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
    fn label(self) -> &'static str {
        match self {
            Page::AlwaysOn => LABEL_NAV_ALWAYS_ON,
            Page::AlwaysTop => LABEL_NAV_ALWAYS_TOP,
            Page::ScreenRuler => LABEL_NAV_SCREEN_RULER,
            Page::ImageResizer => LABEL_NAV_IMAGE_RESIZER,
            Page::General => LABEL_NAV_GENERAL,
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
        context.window_visuals(
            WindowVisuals::new()
                .backdrop(WindowBackdrop::Mica)
                .theme(WindowTheme::System)
                .client_size(CLIENT_WIDTH, CLIENT_HEIGHT)
                .constraints(WindowConstraints {
                    min_width: Some(MIN_WIDTH),
                    min_height: Some(MIN_HEIGHT),
                    max_width: None,
                    max_height: None,
                }),
        );
        let items = PAGES.map(|page| {
            KeyedView::new(
                page.tag(),
                NavigationViewItem::new()
                    .tag(page.tag())
                    .is_selected(page == self.page)
                    .slots([
                        SlotView::new(NavigationViewItemSlot::Content, page.label()),
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
    fn toggled(&self, id: usize) -> bool {
        let s = &self.snapshot;
        match id {
            DISPLAY => s.display,
            SOUND => s.sound,
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
            _ => return None,
        };
        order.iter().position(|item| *item == id)
    }
    fn page_view(&self, context: &ViewContext<Self>) -> View {
        let (title, lead) = page_copy(self.page);
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
        vec![
            caption(&s.status),
            wide_card(
                LABEL_MODE,
                self.choices(
                    context,
                    &MODE_ORDER,
                    [
                        LABEL_MODE_OFF,
                        LABEL_MODE_HALF,
                        LABEL_MODE_HOUR,
                        LABEL_MODE_TWO,
                        LABEL_MODE_FOREVER,
                    ],
                ),
            ),
            self.toggle_card(context, DISPLAY, LABEL_DISPLAY),
        ]
    }
    fn always_top(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        vec![
            described_card(
                LABEL_PIN,
                &s.top_status,
                Button::new()
                    .on_click(context.message(Message::Command(PIN)))
                    .content(LABEL_PIN_ACTION),
            ),
            self.toggle_card(context, SOUND, LABEL_SOUND),
            self.hotkey_card(context, HOTKEY_PIN_EDIT, LABEL_HOTKEY_PIN, &s.pin_hotkey),
            self.hotkey_card(
                context,
                HOTKEY_OP_UP_EDIT,
                LABEL_HOTKEY_OP_UP,
                &s.opacity_up_hotkey,
            ),
            self.hotkey_card(
                context,
                HOTKEY_OP_DOWN_EDIT,
                LABEL_HOTKEY_OP_DOWN,
                &s.opacity_down_hotkey,
            ),
            self.toggle_card(context, FRAME_ACCENT, LABEL_FRAME_ACCENT),
            card(
                LABEL_FRAME_COLOR,
                color_button(context, FRAME_COLOR, s.frame_color),
            ),
            stepper_card(
                context,
                LABEL_FRAME_OPACITY,
                format!("%{}", s.frame_opacity),
                FRAME_OPACITY_DOWN,
                FRAME_OPACITY_UP,
            ),
            stepper_card(
                context,
                LABEL_FRAME_THICKNESS,
                format!("{} px", s.frame_thickness),
                FRAME_THICK_DOWN,
                FRAME_THICK_UP,
            ),
            caption(LABEL_TOP_LEAD),
        ]
    }
    fn screen_ruler(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let measure = Button::new()
            .style(ButtonStyle::Accent)
            .on_click(context.message(Message::Command(MEASURE)))
            .content(if s.ruler_active {
                LABEL_RULER_STOP
            } else {
                LABEL_RULER_START
            });
        let status = if s.ruler_active {
            LABEL_RULER_ON
        } else {
            LABEL_RULER_OFF
        };
        vec![
            described_card(LABEL_RULER_TITLE, status, measure),
            self.hotkey_card(context, RULER_HOTKEY, LABEL_HOTKEY_PIN, &s.ruler_hotkey),
            wide_card(
                LABEL_RULER_MODE,
                self.choices(
                    context,
                    &RULER_MODE_ORDER,
                    [
                        LABEL_RULER_NONE,
                        LABEL_RULER_BOUNDS,
                        LABEL_RULER_SPACING,
                        LABEL_RULER_HORIZONTAL,
                        LABEL_RULER_VERTICAL,
                    ],
                ),
            ),
            wide_card(
                LABEL_RULER_UNIT,
                self.choices(
                    context,
                    &RULER_UNIT_ORDER,
                    [
                        LABEL_RULER_PX,
                        LABEL_RULER_IN,
                        LABEL_RULER_CM,
                        LABEL_RULER_MM,
                        LABEL_RULER_DIP,
                    ],
                ),
            ),
            self.toggle_card(context, RULER_CONTINUOUS, LABEL_RULER_CONTINUOUS),
            self.toggle_card(context, RULER_CHANNEL, LABEL_RULER_CHANNEL),
            self.toggle_card(context, RULER_FEET, LABEL_RULER_FEET),
            stepper_card(
                context,
                LABEL_RULER_TOLERANCE,
                s.ruler_tolerance.to_string(),
                RULER_TOLERANCE_DOWN,
                RULER_TOLERANCE_UP,
            ),
            card(
                LABEL_RULER_COLOR,
                color_button(context, RULER_COLOR, s.ruler_color),
            ),
            caption(LABEL_RULER_LEAD),
        ]
    }
    fn image_resizer(&self, context: &ViewContext<Self>) -> Vec<View> {
        let resize = &self.snapshot.resize;
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
                    .content(LABEL_RESIZE_FILE_APPLY),
            ));
        vec![
            card(
                LABEL_RESIZE_OPEN,
                Button::new()
                    .style(ButtonStyle::Accent)
                    .on_click(context.message(Message::Command(RESIZE_OPEN)))
                    .content(LABEL_RESIZE_OPEN),
            ),
            self.toggle_card(context, RESIZE_SHRINK, LABEL_RESIZE_SHRINK),
            self.toggle_card(context, RESIZE_REPLACE, LABEL_RESIZE_REPLACE),
            self.toggle_card(context, RESIZE_ORIENTATION, LABEL_RESIZE_ORIENTATION),
            self.toggle_card(context, RESIZE_METADATA, LABEL_RESIZE_METADATA),
            self.toggle_card(context, RESIZE_KEEP_DATE, LABEL_RESIZE_KEEP_DATE),
            stepper_card(
                context,
                LABEL_RESIZE_JPEG,
                resize.jpeg_quality.to_string(),
                RESIZE_JPEG_DOWN,
                RESIZE_JPEG_UP,
            ),
            stepper_card(
                context,
                LABEL_RESIZE_TIFF,
                resize.tiff.label().to_string(),
                RESIZE_TIFF_DOWN,
                RESIZE_TIFF_UP,
            ),
            wide_card(
                LABEL_RESIZE_FILE,
                stack(vec![filename_row, caption(LABEL_RESIZE_FILE_HINT)], 6.0),
            ),
            wide_card(
                LABEL_RESIZE_ENCODER,
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
                LABEL_RESIZE_PNG,
                self.choices(
                    context,
                    &RESIZE_PNG_ORDER,
                    [
                        PngInterlace::Default.label(),
                        PngInterlace::On.label(),
                        PngInterlace::Off.label(),
                    ],
                ),
            ),
        ]
    }
    fn general(&self, context: &ViewContext<Self>) -> Vec<View> {
        let s = &self.snapshot;
        let mut about = vec![
            heading(LABEL_ABOUT_NAME, 20.0).into(),
            caption(LABEL_ABOUT_SUB),
        ];
        about.extend(installed_about_facts().map(|(name, value)| {
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
        let update_label = if s.update_ready {
            LABEL_UPDATE_READY
        } else {
            LABEL_UPDATE_CHECK
        };
        vec![
            surface(stack(about, 6.0)),
            section(LABEL_GENERAL_UPDATES),
            card(
                s.update_status,
                Button::new()
                    .on_click(context.message(Message::Command(UPDATE)))
                    .content(update_label),
            ),
            section(LABEL_GENERAL_SESSION),
            card(
                &s.hint,
                Button::new()
                    .on_click(context.message(Message::Command(EXIT)))
                    .content(LABEL_EXIT),
            ),
        ]
    }
    fn toggle_card(&self, context: &ViewContext<Self>, id: usize, label: &str) -> View {
        let toggle = ToggleSwitch::new()
            .is_on(self.toggled(id))
            .on_toggled(context.callback(move |on| Message::Toggle(id, on)))
            .slots([
                SlotView::new(ToggleSwitchSlot::OnContent, LABEL_ON),
                SlotView::new(ToggleSwitchSlot::OffContent, LABEL_OFF),
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
                LABEL_HOTKEY_CAPTURE
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
fn page_copy(page: Page) -> (&'static str, &'static str) {
    match page {
        Page::General => (LABEL_GENERAL_TITLE, LABEL_GENERAL_LEAD),
        Page::AlwaysOn => (LABEL_ALWAYS_ON_TITLE, LABEL_ALWAYS_ON_LEAD),
        Page::AlwaysTop => (LABEL_ALWAYS_TOP_TITLE, LABEL_ALWAYS_TOP_LEAD),
        Page::ScreenRuler => (LABEL_RULER_TITLE, LABEL_RULER_PAGE_LEAD),
        Page::ImageResizer => (LABEL_RESIZE_TITLE, LABEL_RESIZE_LEAD),
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
    version: &str,
    arch: &str,
    channel: &str,
    package: &str,
    settings: &str,
) -> [(&'static str, String); 5] {
    [
        ("Sürüm", version.to_string()),
        ("Mimari", arch.to_string()),
        ("Kanal", channel.to_string()),
        ("Paket", package.to_string()),
        ("Ayarlar", settings.to_string()),
    ]
}
fn installed_about_facts() -> [(&'static str, String); 5] {
    about_facts(
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
        assert_eq!(Page::default(), Page::AlwaysOn);
    }
    #[test]
    fn page_copy_stays_in_turkish() {
        assert_eq!(page_copy(Page::General).0, "Genel");
        assert_eq!(page_copy(Page::AlwaysOn).0, "AlwaysOn");
        assert_eq!(page_copy(Page::AlwaysTop).0, "AlwaysTop");
        assert_eq!(page_copy(Page::ScreenRuler).0, "ScreenRuler");
        assert_eq!(page_copy(Page::ImageResizer).0, "ImageResizer");
        assert_eq!(page_copy(Page::General).1, LABEL_GENERAL_LEAD);
        assert_eq!(Page::General.label(), "Genel");
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
        let rows = about_facts(
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
        let installed = installed_about_facts();
        assert_eq!(installed[0].1, env!("CARGO_PKG_VERSION"));
        assert!(matches!(installed[1].1.as_str(), "ARM64" | "x64" | "diğer"));
        assert!(installed[2].1 == "win-arm64" || installed[2].1 == "win-x64");
        assert_eq!(installed[3].1, PACKAGE_ID);
        assert!(installed[4].1.contains("PowerKit"));
        assert!(installed[4].1.contains("settings.txt"));
    }
}
