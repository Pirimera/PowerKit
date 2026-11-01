//! Settings shell: Windows-theme aligned navigation and per-tool pages.
use super::{
    DISPLAY, EXIT, FOREVER, HALF_HOUR, HINT, HOUR, LABEL_DISPLAY, LABEL_EXIT, LABEL_STATUS_OFF,
    LABEL_UPDATE_CHECK, OFF, PIN, SOUND, STATUS, TWO_HOURS, UPDATE, UPDATE_STATUS, theme, wide,
};
use std::{
    cell::RefCell,
    io,
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{HANDLE, HWND, LRESULT, RECT},
    Graphics::Gdi::{
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLOR_BTNTEXT, COLOR_HIGHLIGHTTEXT, CreateFontW,
        CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_END_ELLIPSIS,
        DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawFocusRect, DrawTextW,
        Ellipse, FW_NORMAL, FW_SEMIBOLD, FillRect, GetStockObject, GetSysColor, HFONT, HGDIOBJ,
        InvalidateRect, NULL_PEN, OUT_DEFAULT_PRECIS, PS_SOLID, RoundRect, SelectObject,
        SetBkColor, SetBkMode, SetTextColor, TRANSPARENT,
    },
    UI::{
        Controls::{
            DRAWITEMSTRUCT,
            Dialogs::{CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW},
            ODS_FOCUS, ODS_SELECTED,
        },
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{
            BS_OWNERDRAW, GWLP_ID, GetClientRect, GetDlgItem, GetPropW, GetWindowLongPtrW,
            GetWindowTextLengthW, GetWindowTextW, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER,
            SendMessageW, SetPropW, SetWindowPos, SetWindowTextW, ShowWindow, WM_CTLCOLORBTN,
            WM_CTLCOLORSTATIC, WM_SETFONT, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
        },
    },
};
pub(super) const NAV_GENERAL: usize = 30;
pub(super) const NAV_ALWAYS_ON: usize = 31;
pub(super) const NAV_ALWAYS_TOP: usize = 32;
pub(super) const PAGE_TITLE: usize = 110;
pub(super) const PAGE_LEAD: usize = 111;
pub(super) const TOP_STATUS: usize = 112;
pub(super) const TOP_LEAD: usize = 113;
pub(super) const HOTKEY_PIN_EDIT: usize = 40;
pub(super) const HOTKEY_OP_UP_EDIT: usize = 41;
pub(super) const HOTKEY_OP_DOWN_EDIT: usize = 42;
pub(super) const FRAME_ACCENT: usize = 43;
pub(super) const FRAME_COLOR: usize = 44;
pub(super) const FRAME_OPACITY: usize = 45;
pub(super) const FRAME_OPACITY_DOWN: usize = 46;
pub(super) const FRAME_OPACITY_UP: usize = 47;
pub(super) const FRAME_THICKNESS: usize = 48;
pub(super) const FRAME_THICK_DOWN: usize = 49;
pub(super) const FRAME_THICK_UP: usize = 50;
const LABEL_NAV_GENERAL: &str = "Genel";
const LABEL_NAV_ALWAYS_ON: &str = "AlwaysOn";
const LABEL_NAV_ALWAYS_TOP: &str = "AlwaysTop";
const LABEL_PIN: &str = "Son pencereyi üstte tut";
const LABEL_SOUND: &str = "Ses çal";
const LABEL_MODE_OFF: &str = "AlwaysOn kapalı";
const LABEL_MODE_HALF: &str = "30 dakika";
const LABEL_MODE_HOUR: &str = "1 saat";
const LABEL_MODE_TWO: &str = "2 saat";
const LABEL_MODE_FOREVER: &str = "Süresiz";
const LABEL_HOTKEY_PIN: &str = "Etkinleştirme kısayolu";
const LABEL_HOTKEY_OP_UP: &str = "Saydamlığı artır";
const LABEL_HOTKEY_OP_DOWN: &str = "Saydamlığı azalt";
const LABEL_FRAME_ACCENT: &str = "Sistem vurgu rengini kullan";
const LABEL_FRAME_COLOR: &str = "Kenarlık rengi";
const LABEL_FRAME_OPACITY: &str = "Kenarlık opaklığı";
const LABEL_FRAME_THICKNESS: &str = "Kenarlık kalınlığı";
const LABEL_STEPPER_DOWN: &str = "−";
const LABEL_STEPPER_UP: &str = "+";
const LABEL_GENERAL_TITLE: &str = "Genel";
const LABEL_GENERAL_LEAD: &str = "PowerKit genelindeki tercihler ve güncellemeler.";
const LABEL_ALWAYS_ON_TITLE: &str = "AlwaysOn";
const LABEL_ALWAYS_ON_LEAD: &str = "Sistemi ve isteğe bağlı olarak ekranı uyanık tutar.";
const LABEL_ALWAYS_TOP_TITLE: &str = "AlwaysTop";
const LABEL_ALWAYS_TOP_LEAD: &str =
    "Odaklanan pencereleri diğerlerinin üstünde tutar. Pinler kaydedilmez.";
const LABEL_TOP_OFF: &str = "AlwaysTop kapalı";
const LABEL_TOP_LEAD: &str =
    "Kenarlık ve kısayollar kaydedilir. Sabitlenen pencerenin saydamlığı kaydedilmez.";
const SS_NOPREFIX: u32 = 0x0080;
const SS_EDITCONTROL: u32 = 0x2000;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    AlwaysOn,
    AlwaysTop,
    General,
}
#[derive(Clone, Copy)]
enum Kind {
    Button,
    Check,
    Static,
    Nav,
    Mode,
    Hotkey,
    Color,
}
struct Spec {
    id: usize,
    label: &'static str,
    kind: Kind,
    page: Option<Page>,
}
const SPECS: &[Spec] = &[
    Spec {
        id: NAV_GENERAL,
        label: LABEL_NAV_GENERAL,
        kind: Kind::Nav,
        page: None,
    },
    Spec {
        id: NAV_ALWAYS_ON,
        label: LABEL_NAV_ALWAYS_ON,
        kind: Kind::Nav,
        page: None,
    },
    Spec {
        id: NAV_ALWAYS_TOP,
        label: LABEL_NAV_ALWAYS_TOP,
        kind: Kind::Nav,
        page: None,
    },
    Spec {
        id: PAGE_TITLE,
        label: LABEL_ALWAYS_ON_TITLE,
        kind: Kind::Static,
        page: None,
    },
    Spec {
        id: PAGE_LEAD,
        label: LABEL_ALWAYS_ON_LEAD,
        kind: Kind::Static,
        page: None,
    },
    Spec {
        id: STATUS,
        label: LABEL_STATUS_OFF,
        kind: Kind::Static,
        page: Some(Page::AlwaysOn),
    },
    Spec {
        id: OFF,
        label: LABEL_MODE_OFF,
        kind: Kind::Mode,
        page: Some(Page::AlwaysOn),
    },
    Spec {
        id: HALF_HOUR,
        label: LABEL_MODE_HALF,
        kind: Kind::Mode,
        page: Some(Page::AlwaysOn),
    },
    Spec {
        id: HOUR,
        label: LABEL_MODE_HOUR,
        kind: Kind::Mode,
        page: Some(Page::AlwaysOn),
    },
    Spec {
        id: TWO_HOURS,
        label: LABEL_MODE_TWO,
        kind: Kind::Mode,
        page: Some(Page::AlwaysOn),
    },
    Spec {
        id: FOREVER,
        label: LABEL_MODE_FOREVER,
        kind: Kind::Mode,
        page: Some(Page::AlwaysOn),
    },
    Spec {
        id: DISPLAY,
        label: LABEL_DISPLAY,
        kind: Kind::Check,
        page: Some(Page::AlwaysOn),
    },
    Spec {
        id: TOP_STATUS,
        label: LABEL_TOP_OFF,
        kind: Kind::Static,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: PIN,
        label: LABEL_PIN,
        kind: Kind::Button,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: SOUND,
        label: LABEL_SOUND,
        kind: Kind::Check,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: HOTKEY_PIN_EDIT,
        label: LABEL_HOTKEY_PIN,
        kind: Kind::Hotkey,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: HOTKEY_OP_UP_EDIT,
        label: LABEL_HOTKEY_OP_UP,
        kind: Kind::Hotkey,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: HOTKEY_OP_DOWN_EDIT,
        label: LABEL_HOTKEY_OP_DOWN,
        kind: Kind::Hotkey,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: FRAME_ACCENT,
        label: LABEL_FRAME_ACCENT,
        kind: Kind::Check,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: FRAME_COLOR,
        label: LABEL_FRAME_COLOR,
        kind: Kind::Color,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: FRAME_OPACITY,
        label: LABEL_FRAME_OPACITY,
        kind: Kind::Static,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: FRAME_OPACITY_DOWN,
        label: LABEL_STEPPER_DOWN,
        kind: Kind::Button,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: FRAME_OPACITY_UP,
        label: LABEL_STEPPER_UP,
        kind: Kind::Button,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: FRAME_THICKNESS,
        label: LABEL_FRAME_THICKNESS,
        kind: Kind::Static,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: FRAME_THICK_DOWN,
        label: LABEL_STEPPER_DOWN,
        kind: Kind::Button,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: FRAME_THICK_UP,
        label: LABEL_STEPPER_UP,
        kind: Kind::Button,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: TOP_LEAD,
        label: LABEL_TOP_LEAD,
        kind: Kind::Static,
        page: Some(Page::AlwaysTop),
    },
    Spec {
        id: UPDATE,
        label: LABEL_UPDATE_CHECK,
        kind: Kind::Button,
        page: Some(Page::General),
    },
    Spec {
        id: UPDATE_STATUS,
        label: "",
        kind: Kind::Static,
        page: Some(Page::General),
    },
    Spec {
        id: HINT,
        label: "",
        kind: Kind::Static,
        page: Some(Page::General),
    },
    Spec {
        id: EXIT,
        label: LABEL_EXIT,
        kind: Kind::Button,
        page: Some(Page::General),
    },
];
#[derive(Clone, Copy)]
pub(super) struct Metrics {
    pub nav_width: i32,
    pub pad: i32,
    pub control_h: i32,
    pub title_h: i32,
    pub lead_h: i32,
    pub hint_h: i32,
    pub nav_item_h: i32,
    pub card_radius: i32,
    pub mode_min_w: i32,
}
pub(super) fn metrics(dpi: u32) -> Metrics {
    let scale = |n: i32| n * dpi as i32 / 96;
    Metrics {
        nav_width: scale(200),
        pad: scale(20),
        control_h: scale(32),
        title_h: scale(28),
        lead_h: scale(40),
        hint_h: scale(64),
        nav_item_h: scale(36),
        card_radius: scale(8),
        mode_min_w: scale(108),
    }
}
pub(super) const MODE_ORDER: [usize; 5] = [OFF, HALF_HOUR, HOUR, TWO_HOURS, FOREVER];
pub(super) fn wrap_cells(
    width: i32,
    min_cell: i32,
    gap: i32,
    row_h: i32,
    count: usize,
) -> Vec<RECT> {
    if count == 0 || width <= 0 {
        return Vec::new();
    }
    let cols = ((width + gap) / (min_cell + gap)).clamp(1, count as i32) as usize;
    let cell_w = ((width - gap * (cols as i32 - 1)) / cols as i32).max(1);
    (0..count)
        .map(|index| {
            let col = (index % cols) as i32;
            let row = (index / cols) as i32;
            let left = col * (cell_w + gap);
            let top = row * (row_h + gap);
            RECT {
                left,
                top,
                right: left + cell_w,
                bottom: top + row_h,
            }
        })
        .collect()
}
fn is_mode(id: usize) -> bool {
    MODE_ORDER.contains(&id)
}
fn stepper_ids(id: usize) -> Option<(usize, usize, usize)> {
    match id {
        FRAME_OPACITY | FRAME_OPACITY_DOWN | FRAME_OPACITY_UP => {
            Some((FRAME_OPACITY, FRAME_OPACITY_DOWN, FRAME_OPACITY_UP))
        }
        FRAME_THICKNESS | FRAME_THICK_DOWN | FRAME_THICK_UP => {
            Some((FRAME_THICKNESS, FRAME_THICK_DOWN, FRAME_THICK_UP))
        }
        _ => None,
    }
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
pub(super) fn mode_button(mode: crate::always_on::Mode, timed: Option<usize>) -> usize {
    match mode {
        crate::always_on::Mode::Off => OFF,
        crate::always_on::Mode::Indefinite => FOREVER,
        crate::always_on::Mode::Until(_) => timed.filter(|id| MODE_ORDER.contains(id)).unwrap_or(0),
    }
}
pub(super) fn page_from_nav(id: usize) -> Option<Page> {
    match id {
        NAV_GENERAL => Some(Page::General),
        NAV_ALWAYS_ON => Some(Page::AlwaysOn),
        NAV_ALWAYS_TOP => Some(Page::AlwaysTop),
        _ => None,
    }
}
fn spec(id: usize) -> Option<&'static Spec> {
    SPECS.iter().find(|item| item.id == id)
}
fn visible_on(page: Page, item: &Spec) -> bool {
    item.page.is_none() || item.page == Some(page)
}
fn page_copy(page: Page) -> (&'static str, &'static str) {
    match page {
        Page::General => (LABEL_GENERAL_TITLE, LABEL_GENERAL_LEAD),
        Page::AlwaysOn => (LABEL_ALWAYS_ON_TITLE, LABEL_ALWAYS_ON_LEAD),
        Page::AlwaysTop => (LABEL_ALWAYS_TOP_TITLE, LABEL_ALWAYS_TOP_LEAD),
    }
}
fn ui_font(dpi: u32, px: i32, weight: i32) -> io::Result<HFONT> {
    let height = -(px * dpi as i32 / 96);
    // SAFETY: face name is a temporary NUL-terminated buffer; GDI copies it.
    let font = unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe UI").as_ptr(),
        )
    };
    if font.is_null() {
        Err(io::Error::other("Arayüz yazı tipi oluşturulamadı"))
    } else {
        Ok(font)
    }
}
#[derive(Default)]
pub(super) struct Fonts {
    pub body: HFONT,
    pub title: HFONT,
    pub nav: HFONT,
}
impl Fonts {
    fn create(dpi: u32) -> io::Result<Self> {
        Ok(Self {
            body: ui_font(dpi, 14, FW_NORMAL as i32)?,
            title: ui_font(dpi, 20, FW_SEMIBOLD as i32)?,
            nav: ui_font(dpi, 14, FW_NORMAL as i32)?,
        })
    }
    fn release(&mut self) {
        for font in [&mut self.body, &mut self.title, &mut self.nav] {
            if !font.is_null() {
                unsafe {
                    DeleteObject(*font);
                }
                *font = null_mut();
            }
        }
    }
}
#[derive(Default)]
pub(super) struct Brushes {
    background: windows_sys::Win32::Graphics::Gdi::HBRUSH,
    pane: windows_sys::Win32::Graphics::Gdi::HBRUSH,
    card: windows_sys::Win32::Graphics::Gdi::HBRUSH,
    selected: windows_sys::Win32::Graphics::Gdi::HBRUSH,
    accent: windows_sys::Win32::Graphics::Gdi::HBRUSH,
    button: windows_sys::Win32::Graphics::Gdi::HBRUSH,
    button_hot: windows_sys::Win32::Graphics::Gdi::HBRUSH,
    toggle: windows_sys::Win32::Graphics::Gdi::HBRUSH,
    thumb: windows_sys::Win32::Graphics::Gdi::HBRUSH,
    stroke: windows_sys::Win32::Graphics::Gdi::HPEN,
}
impl Brushes {
    fn create(theme: theme::WindowTheme) -> Self {
        // SAFETY: CreateSolidBrush copies the COLORREF; we own the returned handle.
        unsafe {
            Self {
                background: CreateSolidBrush(theme.background),
                pane: CreateSolidBrush(theme.pane),
                card: CreateSolidBrush(theme.card),
                selected: CreateSolidBrush(theme.selected),
                accent: CreateSolidBrush(theme.accent),
                button: CreateSolidBrush(theme.button),
                button_hot: CreateSolidBrush(theme.button_hot),
                toggle: CreateSolidBrush(theme.toggle),
                thumb: CreateSolidBrush(theme.thumb),
                stroke: CreatePen(PS_SOLID, 1, theme.stroke),
            }
        }
    }
    fn release(&mut self) {
        for brush in [
            &mut self.background,
            &mut self.pane,
            &mut self.card,
            &mut self.selected,
            &mut self.accent,
            &mut self.button,
            &mut self.button_hot,
            &mut self.toggle,
            &mut self.thumb,
        ] {
            if !brush.is_null() {
                unsafe {
                    DeleteObject(*brush);
                }
                *brush = null_mut();
            }
        }
        if !self.stroke.is_null() {
            unsafe {
                DeleteObject(self.stroke);
            }
            self.stroke = null_mut();
        }
    }
}
pub(super) struct Chrome {
    pub fonts: Fonts,
    pub brushes: Brushes,
    pub theme: Option<theme::WindowTheme>,
    pub page: Page,
    pub selected_mode: usize,
    pub capturing: Option<usize>,
    pub frame_preview: u32,
}
impl Default for Chrome {
    fn default() -> Self {
        Self {
            fonts: Fonts::default(),
            brushes: Brushes::default(),
            theme: None,
            page: Page::default(),
            selected_mode: OFF,
            capturing: None,
            frame_preview: 0,
        }
    }
}
impl Chrome {
    pub(super) fn release(&mut self) {
        self.fonts.release();
        self.brushes.release();
        self.theme = None;
    }
}
fn class_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Static => "STATIC",
        Kind::Button | Kind::Check | Kind::Nav | Kind::Mode | Kind::Hotkey | Kind::Color => {
            "BUTTON"
        }
    }
}
fn control_style(kind: Kind) -> u32 {
    WS_CHILD
        | WS_VISIBLE
        | match kind {
            Kind::Static => SS_NOPREFIX | SS_EDITCONTROL,
            Kind::Button | Kind::Check | Kind::Nav | Kind::Mode | Kind::Hotkey | Kind::Color => {
                WS_TABSTOP | BS_OWNERDRAW as u32
            }
        }
}
pub(super) fn create_controls(
    hwnd: HWND,
    instance: windows_sys::Win32::Foundation::HINSTANCE,
    display: bool,
    sound: bool,
) -> io::Result<()> {
    // SAFETY: children belong to the live top-level window; IDs stay unique.
    unsafe {
        for item in SPECS {
            let control = windows_sys::Win32::UI::WindowsAndMessaging::CreateWindowExW(
                0,
                wide(class_name(item.kind)).as_ptr(),
                wide(item.label).as_ptr(),
                control_style(item.kind),
                0,
                0,
                0,
                0,
                hwnd,
                item.id as _,
                instance,
                std::ptr::null(),
            );
            if control.is_null() {
                return Err(io::Error::last_os_error());
            }
            if item.id == DISPLAY {
                set_check(control, display);
            }
            if item.id == SOUND {
                set_check(control, sound);
            }
            if item.id == FRAME_ACCENT {
                set_check(control, true);
            }
        }
    }
    theme_children(hwnd, &theme::window_theme());
    Ok(())
}
fn theme_children(hwnd: HWND, next: &theme::WindowTheme) {
    let dark = !next.light && !next.high_contrast;
    theme::apply_control_theme(hwnd, dark);
    for item in SPECS {
        let control = unsafe { GetDlgItem(hwnd, item.id as i32) };
        if !control.is_null() {
            theme::apply_control_theme(control, dark);
        }
    }
}
fn content_origin(m: Metrics) -> (i32, i32) {
    (m.nav_width + m.pad, m.pad)
}
fn control_height(id: usize, m: Metrics) -> i32 {
    if matches!(id, HINT | TOP_LEAD | PAGE_LEAD) {
        m.hint_h.max(m.lead_h)
    } else if id == PAGE_TITLE {
        m.title_h
    } else {
        m.control_h
    }
}
pub(super) fn prepare_fonts(dpi: u32, chrome: &mut Chrome) -> io::Result<(Page, Fonts)> {
    let fonts = Fonts::create(dpi)?;
    let mut old = std::mem::replace(&mut chrome.fonts, fonts);
    old.release();
    Ok((
        chrome.page,
        Fonts {
            body: chrome.fonts.body,
            title: chrome.fonts.title,
            nav: chrome.fonts.nav,
        },
    ))
}
pub(super) fn apply_layout(hwnd: HWND, dpi: u32, page: Page, fonts: &Fonts) {
    let m = metrics(dpi);
    let mut client = RECT::default();
    // SAFETY: hwnd is live; font handles remain owned by Chrome for this apply.
    unsafe {
        GetClientRect(hwnd, &mut client);
        let content_w = (client.right - m.nav_width - m.pad * 2).max(m.pad * 8);
        let (x0, mut y) = content_origin(m);
        let mut nav_y = m.pad;
        let gap = m.pad / 3;
        let mode_cells = wrap_cells(content_w, m.mode_min_w, gap, m.control_h, MODE_ORDER.len());
        let mut modes_placed = false;
        let mut steppers_placed = std::collections::HashSet::new();
        for item in SPECS {
            let control = GetDlgItem(hwnd, item.id as i32);
            if control.is_null() {
                continue;
            }
            let show = visible_on(page, item);
            if is_mode(item.id) {
                if show && !modes_placed {
                    for (index, id) in MODE_ORDER.iter().enumerate() {
                        let cell = mode_cells[index];
                        position(
                            hwnd,
                            *id,
                            RECT {
                                left: x0 + cell.left,
                                top: y + cell.top,
                                right: x0 + cell.right,
                                bottom: y + cell.bottom,
                            },
                            fonts,
                            true,
                        );
                    }
                    y += mode_cells
                        .iter()
                        .map(|cell| cell.bottom)
                        .max()
                        .unwrap_or(m.control_h)
                        + gap;
                    modes_placed = true;
                } else if !show {
                    position(
                        hwnd,
                        item.id,
                        bounds(x0, y, content_w, m.control_h),
                        fonts,
                        false,
                    );
                }
                continue;
            }
            if let Some((label, down, up)) = stepper_ids(item.id) {
                if show && steppers_placed.insert(label) {
                    let btn = m.control_h;
                    position(
                        hwnd,
                        label,
                        bounds(
                            x0,
                            y,
                            (content_w - btn * 2 - gap * 2).max(m.pad),
                            m.control_h,
                        ),
                        fonts,
                        true,
                    );
                    position(
                        hwnd,
                        down,
                        bounds(x0 + content_w - btn * 2 - gap, y, btn, m.control_h),
                        fonts,
                        true,
                    );
                    position(
                        hwnd,
                        up,
                        bounds(x0 + content_w - btn, y, btn, m.control_h),
                        fonts,
                        true,
                    );
                    y += m.control_h + gap;
                } else if !show {
                    position(
                        hwnd,
                        item.id,
                        bounds(x0, y, content_w, m.control_h),
                        fonts,
                        false,
                    );
                }
                continue;
            }
            let (x, top, width, height) = if matches!(item.kind, Kind::Nav) {
                let top = nav_y;
                nav_y += m.nav_item_h + m.pad / 4;
                (m.pad / 2, top, m.nav_width - m.pad, m.nav_item_h)
            } else {
                let height = control_height(item.id, m);
                let top = y;
                if show {
                    y += height + gap;
                    if item.id == PAGE_LEAD {
                        y += m.pad / 2;
                    }
                }
                (x0, top, content_w, height)
            };
            position(hwnd, item.id, bounds(x, top, width, height), fonts, show);
        }
    }
    apply_page_text(hwnd, page);
}
fn bounds(x: i32, y: i32, width: i32, height: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}
fn position(hwnd: HWND, id: usize, rect: RECT, fonts: &Fonts, show: bool) {
    let control = unsafe { GetDlgItem(hwnd, id as i32) };
    if control.is_null() {
        return;
    }
    let font = if spec(id).is_some_and(|item| matches!(item.kind, Kind::Nav)) {
        fonts.nav
    } else if id == PAGE_TITLE {
        fonts.title
    } else {
        fonts.body
    };
    unsafe {
        SetWindowPos(
            control,
            null_mut(),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        SendMessageW(control, WM_SETFONT, font as usize, 1);
        ShowWindow(control, if show { SW_SHOW } else { SW_HIDE });
    }
}
fn apply_page_text(hwnd: HWND, page: Page) {
    let (title, lead) = page_copy(page);
    // SAFETY: page labels are static; controls belong to this window.
    unsafe {
        SetWindowTextW(GetDlgItem(hwnd, PAGE_TITLE as i32), wide(title).as_ptr());
        SetWindowTextW(GetDlgItem(hwnd, PAGE_LEAD as i32), wide(lead).as_ptr());
    }
}
pub(super) fn select_page(chrome: &mut Chrome, page: Page) {
    chrome.page = page;
}
pub(super) fn refresh_theme(chrome: &mut Chrome) -> theme::WindowTheme {
    let next = theme::window_theme();
    if chrome.theme != Some(next) {
        chrome.brushes.release();
        chrome.brushes = Brushes::create(next);
        chrome.theme = Some(next);
    }
    next
}
pub(super) fn apply_theme_surfaces(hwnd: HWND, next: theme::WindowTheme) {
    theme::apply_window_chrome(hwnd);
    theme_children(hwnd, &next);
}
fn content_card(client: RECT, m: Metrics) -> RECT {
    RECT {
        left: m.nav_width + m.pad / 2,
        top: m.pad / 2,
        right: client.right - m.pad / 2,
        bottom: client.bottom - m.pad / 2,
    }
}
pub(super) fn erase(
    hwnd: HWND,
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    chrome: &Chrome,
) -> bool {
    if chrome.brushes.background.is_null() {
        return false;
    }
    let mut client = RECT::default();
    let m = metrics(unsafe { GetDpiForWindow(hwnd) });
    // SAFETY: hdc is the erase-background DC for our window; pens/brushes stay owned by Chrome.
    unsafe {
        GetClientRect(hwnd, &mut client);
        FillRect(hdc, &client, chrome.brushes.background);
        let mut pane = client;
        pane.right = m.nav_width;
        FillRect(hdc, &pane, chrome.brushes.pane);
        let card = content_card(client, m);
        let old_brush = SelectObject(hdc, chrome.brushes.card as HGDIOBJ);
        let old_pen = SelectObject(hdc, chrome.brushes.stroke as HGDIOBJ);
        RoundRect(
            hdc,
            card.left,
            card.top,
            card.right,
            card.bottom,
            m.card_radius,
            m.card_radius,
        );
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
    }
    true
}
pub(super) fn colorize(
    hwnd: HWND,
    msg: u32,
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    child: HWND,
    chrome: &Chrome,
) -> Option<LRESULT> {
    if !matches!(msg, WM_CTLCOLORSTATIC | WM_CTLCOLORBTN) || chrome.theme.is_none() {
        return None;
    }
    let theme = chrome.theme?;
    let id = unsafe { GetWindowLongPtrW(child, GWLP_ID) } as usize;
    let muted = matches!(id, HINT | PAGE_LEAD | TOP_LEAD | UPDATE_STATUS);
    let (brush, fill) = if spec(id).is_some_and(|item| matches!(item.kind, Kind::Nav)) {
        (chrome.brushes.pane, theme.pane)
    } else {
        (chrome.brushes.card, theme.card)
    };
    // SAFETY: hdc is the control color DC; brush stays owned by Chrome.
    unsafe {
        SetTextColor(hdc, if muted { theme.muted } else { theme.text });
        SetBkMode(hdc, TRANSPARENT as i32);
        SetBkColor(hdc, fill);
    }
    let _ = hwnd;
    Some(brush as isize)
}
pub(super) fn draw_item(hwnd: HWND, draw: &DRAWITEMSTRUCT, chrome: &Chrome) -> bool {
    let Some(theme) = chrome.theme else {
        return false;
    };
    let id = draw.CtlID as usize;
    let Some(item) = spec(id) else {
        return false;
    };
    match item.kind {
        Kind::Nav => draw_nav(hwnd, draw, chrome, theme, item),
        Kind::Button | Kind::Hotkey => draw_button(hwnd, draw, chrome, theme),
        Kind::Mode => draw_mode(hwnd, draw, chrome, theme),
        Kind::Color => draw_color(hwnd, draw, chrome, theme),
        Kind::Check => draw_toggle(hwnd, draw, chrome, theme),
        Kind::Static => false,
    }
}
const TOGGLE_PROP: &str = "PowerKit.Toggle";
pub(super) fn set_check(hwnd: HWND, on: bool) {
    if hwnd.is_null() {
        return;
    }
    let name = wide(TOGGLE_PROP);
    // SAFETY: hwnd is our live owner-draw toggle; the string atom is process-local.
    unsafe {
        SetPropW(hwnd, name.as_ptr(), (usize::from(on) + 1) as HANDLE);
        InvalidateRect(hwnd, null(), 1);
    }
}
fn is_check(hwnd: HWND) -> bool {
    let name = wide(TOGGLE_PROP);
    // SAFETY: hwnd is the live toggle; missing props read as null.
    unsafe { GetPropW(hwnd, name.as_ptr()) as usize == 2 }
}
fn toggle_track(row: RECT, dpi: u32) -> RECT {
    let scale = |n: i32| n * dpi as i32 / 96;
    let width = scale(40);
    let height = scale(20);
    let pad = scale(4);
    let top = (row.top + row.bottom - height) / 2;
    RECT {
        left: row.right - pad - width,
        top,
        right: row.right - pad,
        bottom: top + height,
    }
}
fn toggle_thumb(on: bool, track: RECT, dpi: u32) -> RECT {
    let pad = (2 * dpi as i32 / 96).max(2);
    let size = (track.bottom - track.top - pad * 2).max(pad);
    let left = if on {
        track.right - pad - size
    } else {
        track.left + pad
    };
    RECT {
        left,
        top: track.top + pad,
        right: left + size,
        bottom: track.top + pad + size,
    }
}
fn control_text(hwnd: HWND) -> Vec<u16> {
    // SAFETY: hwnd is the live owner-draw control; the buffer is sized from GetWindowTextLengthW.
    unsafe {
        let len = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut text = vec![0u16; len + 1];
        GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32);
        text
    }
}
fn draw_nav(
    hwnd: HWND,
    draw: &DRAWITEMSTRUCT,
    chrome: &Chrome,
    theme: theme::WindowTheme,
    item: &Spec,
) -> bool {
    let selected = page_from_nav(item.id) == Some(chrome.page);
    let brush = if selected {
        chrome.brushes.selected
    } else {
        chrome.brushes.pane
    };
    // SAFETY: DRAWITEMSTRUCT hdc/rcItem are valid for this WM_DRAWITEM call.
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, brush);
        if selected {
            let mut accent = draw.rcItem;
            accent.right = accent.left + (4 * GetDpiForWindow(hwnd) as i32 / 96).max(3);
            FillRect(draw.hDC, &accent, chrome.brushes.accent);
        }
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        SetTextColor(
            draw.hDC,
            if selected && theme.high_contrast {
                GetSysColor(COLOR_HIGHLIGHTTEXT)
            } else {
                theme.text
            },
        );
        let mut text = wide(item.label);
        let mut bounds = draw.rcItem;
        bounds.left += 12 * GetDpiForWindow(hwnd) as i32 / 96;
        let previous = SelectObject(draw.hDC, chrome.fonts.nav as HGDIOBJ);
        DrawTextW(
            draw.hDC,
            text.as_mut_ptr(),
            -1,
            &mut bounds,
            DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
        SelectObject(draw.hDC, previous);
    }
    true
}
fn draw_button(
    hwnd: HWND,
    draw: &DRAWITEMSTRUCT,
    chrome: &Chrome,
    theme: theme::WindowTheme,
) -> bool {
    let pressed = draw.itemState & ODS_SELECTED != 0;
    let brush = if pressed {
        chrome.brushes.button_hot
    } else {
        chrome.brushes.button
    };
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let radius = (4 * dpi as i32 / 96).max(3);
    // SAFETY: DRAWITEMSTRUCT hdc/rcItem are valid for this WM_DRAWITEM call.
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, chrome.brushes.card);
        let old_brush = SelectObject(draw.hDC, brush as HGDIOBJ);
        let old_pen = SelectObject(draw.hDC, chrome.brushes.stroke as HGDIOBJ);
        RoundRect(
            draw.hDC,
            draw.rcItem.left,
            draw.rcItem.top,
            draw.rcItem.right,
            draw.rcItem.bottom,
            radius,
            radius,
        );
        SelectObject(draw.hDC, old_pen);
        SelectObject(draw.hDC, old_brush);
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        SetTextColor(
            draw.hDC,
            if theme.high_contrast {
                GetSysColor(COLOR_BTNTEXT)
            } else {
                theme.text
            },
        );
        let mut text = control_text(draw.hwndItem);
        let mut bounds = draw.rcItem;
        let previous = SelectObject(draw.hDC, chrome.fonts.body as HGDIOBJ);
        DrawTextW(
            draw.hDC,
            text.as_mut_ptr(),
            -1,
            &mut bounds,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
        SelectObject(draw.hDC, previous);
        if draw.itemState & ODS_FOCUS != 0 {
            let mut focus = draw.rcItem;
            let inset = (3 * dpi as i32 / 96).max(2);
            focus.left += inset;
            focus.top += inset;
            focus.right -= inset;
            focus.bottom -= inset;
            DrawFocusRect(draw.hDC, &focus);
        }
    }
    true
}
fn contrasting_text(color: u32) -> u32 {
    let r = color & 0xff;
    let g = (color >> 8) & 0xff;
    let b = (color >> 16) & 0xff;
    if r * 299 + g * 587 + b * 114 >= 140_000 {
        0
    } else {
        0x00FF_FFFF
    }
}
fn draw_mode(
    hwnd: HWND,
    draw: &DRAWITEMSTRUCT,
    chrome: &Chrome,
    theme: theme::WindowTheme,
) -> bool {
    let selected = chrome.selected_mode == draw.CtlID as usize;
    let brush = if selected {
        chrome.brushes.accent
    } else {
        chrome.brushes.button
    };
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let radius = (4 * dpi as i32 / 96).max(3);
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, chrome.brushes.card);
        let old_brush = SelectObject(draw.hDC, brush as HGDIOBJ);
        let old_pen = SelectObject(draw.hDC, chrome.brushes.stroke as HGDIOBJ);
        RoundRect(
            draw.hDC,
            draw.rcItem.left,
            draw.rcItem.top,
            draw.rcItem.right,
            draw.rcItem.bottom,
            radius,
            radius,
        );
        SelectObject(draw.hDC, old_pen);
        SelectObject(draw.hDC, old_brush);
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        SetTextColor(
            draw.hDC,
            if selected {
                if theme.high_contrast {
                    GetSysColor(COLOR_HIGHLIGHTTEXT)
                } else {
                    contrasting_text(theme.accent)
                }
            } else if theme.high_contrast {
                GetSysColor(COLOR_BTNTEXT)
            } else {
                theme.text
            },
        );
        let mut text = control_text(draw.hwndItem);
        let mut bounds = draw.rcItem;
        let previous = SelectObject(draw.hDC, chrome.fonts.body as HGDIOBJ);
        DrawTextW(
            draw.hDC,
            text.as_mut_ptr(),
            -1,
            &mut bounds,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
        SelectObject(draw.hDC, previous);
        if draw.itemState & ODS_FOCUS != 0 {
            let mut focus = draw.rcItem;
            let inset = (3 * dpi as i32 / 96).max(2);
            focus.left += inset;
            focus.top += inset;
            focus.right -= inset;
            focus.bottom -= inset;
            DrawFocusRect(draw.hDC, &focus);
        }
    }
    true
}
fn draw_color(
    hwnd: HWND,
    draw: &DRAWITEMSTRUCT,
    chrome: &Chrome,
    theme: theme::WindowTheme,
) -> bool {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let radius = (4 * dpi as i32 / 96).max(3);
    let pad = (8 * dpi as i32 / 96).max(6);
    let size = (16 * dpi as i32 / 96).max(12);
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, chrome.brushes.card);
        let old_brush = SelectObject(draw.hDC, chrome.brushes.button as HGDIOBJ);
        let old_pen = SelectObject(draw.hDC, chrome.brushes.stroke as HGDIOBJ);
        RoundRect(
            draw.hDC,
            draw.rcItem.left,
            draw.rcItem.top,
            draw.rcItem.right,
            draw.rcItem.bottom,
            radius,
            radius,
        );
        SelectObject(draw.hDC, old_pen);
        SelectObject(draw.hDC, old_brush);
        let swatch = RECT {
            left: draw.rcItem.left + pad,
            top: (draw.rcItem.top + draw.rcItem.bottom - size) / 2,
            right: draw.rcItem.left + pad + size,
            bottom: (draw.rcItem.top + draw.rcItem.bottom - size) / 2 + size,
        };
        let brush = CreateSolidBrush(chrome.frame_preview);
        FillRect(draw.hDC, &swatch, brush);
        DeleteObject(brush);
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        SetTextColor(
            draw.hDC,
            if theme.high_contrast {
                GetSysColor(COLOR_BTNTEXT)
            } else {
                theme.text
            },
        );
        let mut text = control_text(draw.hwndItem);
        let mut bounds = draw.rcItem;
        bounds.left = swatch.right + pad;
        let previous = SelectObject(draw.hDC, chrome.fonts.body as HGDIOBJ);
        DrawTextW(
            draw.hDC,
            text.as_mut_ptr(),
            -1,
            &mut bounds,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
        SelectObject(draw.hDC, previous);
    }
    true
}
fn draw_toggle(
    hwnd: HWND,
    draw: &DRAWITEMSTRUCT,
    chrome: &Chrome,
    theme: theme::WindowTheme,
) -> bool {
    let on = is_check(draw.hwndItem);
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let track = toggle_track(draw.rcItem, dpi);
    let thumb = toggle_thumb(on, track, dpi);
    let radius = (track.bottom - track.top).max(2);
    let track_brush = if on {
        chrome.brushes.accent
    } else {
        chrome.brushes.toggle
    };
    // SAFETY: DRAWITEMSTRUCT hdc/rcItem are valid for this WM_DRAWITEM call.
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, chrome.brushes.card);
        let old_brush = SelectObject(draw.hDC, track_brush as HGDIOBJ);
        let old_pen = SelectObject(draw.hDC, GetStockObject(NULL_PEN));
        RoundRect(
            draw.hDC,
            track.left,
            track.top,
            track.right,
            track.bottom,
            radius,
            radius,
        );
        SelectObject(draw.hDC, chrome.brushes.thumb as HGDIOBJ);
        Ellipse(draw.hDC, thumb.left, thumb.top, thumb.right, thumb.bottom);
        SelectObject(draw.hDC, old_pen);
        SelectObject(draw.hDC, old_brush);
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        SetTextColor(
            draw.hDC,
            if theme.high_contrast {
                GetSysColor(COLOR_BTNTEXT)
            } else {
                theme.text
            },
        );
        let mut text = control_text(draw.hwndItem);
        let mut bounds = draw.rcItem;
        let gap = (8 * dpi as i32 / 96).max(6);
        bounds.right = track.left - gap;
        let previous = SelectObject(draw.hDC, chrome.fonts.body as HGDIOBJ);
        DrawTextW(
            draw.hDC,
            text.as_mut_ptr(),
            -1,
            &mut bounds,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
        SelectObject(draw.hDC, previous);
        if draw.itemState & ODS_FOCUS != 0 {
            let mut focus = draw.rcItem;
            let inset = (3 * dpi as i32 / 96).max(2);
            focus.left += inset;
            focus.top += inset;
            focus.right -= inset;
            focus.bottom -= inset;
            DrawFocusRect(draw.hDC, &focus);
        }
    }
    true
}
pub(super) const CLIENT_WIDTH: i32 = 760;
pub(super) const CLIENT_HEIGHT: i32 = 680;
pub(super) const MIN_WIDTH: i32 = 640;
pub(super) const MIN_HEIGHT: i32 = 560;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_maps_pages_and_keeps_actions_grouped() {
        assert_eq!(page_from_nav(NAV_GENERAL), Some(Page::General));
        assert_eq!(page_from_nav(NAV_ALWAYS_ON), Some(Page::AlwaysOn));
        assert_eq!(page_from_nav(NAV_ALWAYS_TOP), Some(Page::AlwaysTop));
        assert_eq!(page_from_nav(FOREVER), None);
        let always_on: Vec<usize> = SPECS
            .iter()
            .filter(|item| item.page == Some(Page::AlwaysOn))
            .map(|item| item.id)
            .collect();
        assert_eq!(
            always_on,
            [STATUS, OFF, HALF_HOUR, HOUR, TWO_HOURS, FOREVER, DISPLAY]
        );
        let general: Vec<usize> = SPECS
            .iter()
            .filter(|item| item.page == Some(Page::General))
            .map(|item| item.id)
            .collect();
        assert_eq!(general, [UPDATE, UPDATE_STATUS, HINT, EXIT]);
        let always_top: Vec<usize> = SPECS
            .iter()
            .filter(|item| item.page == Some(Page::AlwaysTop))
            .map(|item| item.id)
            .collect();
        assert_eq!(
            always_top,
            [
                TOP_STATUS,
                PIN,
                SOUND,
                HOTKEY_PIN_EDIT,
                HOTKEY_OP_UP_EDIT,
                HOTKEY_OP_DOWN_EDIT,
                FRAME_ACCENT,
                FRAME_COLOR,
                FRAME_OPACITY,
                FRAME_OPACITY_DOWN,
                FRAME_OPACITY_UP,
                FRAME_THICKNESS,
                FRAME_THICK_DOWN,
                FRAME_THICK_UP,
                TOP_LEAD
            ]
        );
    }
    #[test]
    fn dpi_metrics_keep_smoke_button_height() {
        assert_eq!(metrics(96).control_h, 32);
        assert_eq!(metrics(144).control_h, 48);
        assert_eq!(metrics(192).control_h, 64);
        assert_eq!(metrics(96).nav_width, 200);
        assert_eq!(metrics(144).nav_width, 300);
        let card = content_card(
            RECT {
                left: 0,
                top: 0,
                right: 760,
                bottom: 560,
            },
            metrics(96),
        );
        assert!(card.left >= metrics(96).nav_width);
        assert!(card.right < 760);
    }
    #[test]
    fn page_copy_stays_in_turkish() {
        assert_eq!(page_copy(Page::General).0, "Genel");
        assert_eq!(page_copy(Page::AlwaysOn).0, "AlwaysOn");
        assert_eq!(page_copy(Page::AlwaysTop).0, "AlwaysTop");
    }
    #[test]
    fn toggle_thumb_moves_to_the_on_side() {
        let track = RECT {
            left: 100,
            top: 10,
            right: 140,
            bottom: 30,
        };
        let off = toggle_thumb(false, track, 96);
        let on = toggle_thumb(true, track, 96);
        assert!(off.left < on.left);
        assert_eq!(off.bottom - off.top, on.bottom - on.top);
        assert!(off.left >= track.left);
        assert!(on.right <= track.right);
        assert!(matches!(
            spec(DISPLAY).map(|item| item.kind),
            Some(Kind::Check)
        ));
        assert!(matches!(
            spec(SOUND).map(|item| item.kind),
            Some(Kind::Check)
        ));
    }
    #[test]
    fn mode_tiles_wrap_when_the_row_is_too_narrow() {
        let wide = wrap_cells(580, 108, 8, 32, 5);
        assert_eq!(wide.len(), 5);
        assert!(wide.iter().all(|cell| cell.top == 0));
        let narrow = wrap_cells(100, 108, 8, 32, 5);
        assert_eq!(narrow.iter().filter(|cell| cell.top == 0).count(), 1);
        assert!(narrow.last().is_some_and(|cell| cell.top > 0));
        assert_eq!(MODE_ORDER, [OFF, HALF_HOUR, HOUR, TWO_HOURS, FOREVER]);
    }
}
