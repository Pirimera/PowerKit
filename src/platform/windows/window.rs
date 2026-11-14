//! Settings shell: Windows-theme aligned navigation and per-tool pages.
use super::{
    DISPLAY, EXIT, FOREVER, HALF_HOUR, HINT, HOUR, LABEL_DISPLAY, LABEL_EXIT, LABEL_STATUS_OFF,
    LABEL_UPDATE_CHECK, MEASURE, OFF, PIN, SOUND, STATUS, TWO_HOURS, UPDATE, UPDATE_STATUS, theme,
    wide,
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
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
        COLOR_BTNTEXT, COLOR_GRAYTEXT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOWTEXT, CreateCompatibleDC,
        CreateDIBSection, CreateFontW, CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH,
        DIB_RGB_COLORS, DT_CALCRECT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX,
        DT_PATH_ELLIPSIS, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject,
        DrawFocusRect, DrawTextW, Ellipse, FW_NORMAL, FW_SEMIBOLD, FillRect, GetStockObject,
        GetSysColor, HDC, HFONT, HGDIOBJ, InvalidateRect, NULL_PEN, OUT_DEFAULT_PRECIS, PS_SOLID,
        RoundRect, SRCCOPY, SelectObject, SetBkColor, SetBkMode, SetTextColor, TRANSPARENT,
    },
    UI::{
        Controls::{
            DRAWITEMSTRUCT,
            Dialogs::{CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW},
            ODS_FOCUS, ODS_SELECTED,
        },
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{
            BS_OWNERDRAW, ES_AUTOHSCROLL, GWLP_ID, GetClientRect, GetDlgItem, GetPropW,
            GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, SW_HIDE, SW_SHOW,
            SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetPropW, SetWindowPos, SetWindowTextW,
            ShowWindow, WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_SETFONT, WS_BORDER,
            WS_CHILD, WS_TABSTOP, WS_VISIBLE,
        },
    },
};
pub(super) const NAV_GENERAL: usize = 30;
pub(super) const NAV_ALWAYS_ON: usize = 31;
pub(super) const NAV_ALWAYS_TOP: usize = 32;
pub(super) const NAV_SCREEN_RULER: usize = 33;
pub(super) const NAV_IMAGE_RESIZER: usize = 34;
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
pub(super) const RULER_HOTKEY: usize = 53;
pub(super) const RULER_MODE_NONE: usize = 60;
pub(super) const RULER_MODE_BOUNDS: usize = 61;
pub(super) const RULER_MODE_SPACING: usize = 62;
pub(super) const RULER_MODE_HORIZONTAL: usize = 63;
pub(super) const RULER_MODE_VERTICAL: usize = 64;
pub(super) const RULER_CONTINUOUS: usize = 65;
pub(super) const RULER_CHANNEL: usize = 66;
pub(super) const RULER_FEET: usize = 67;
pub(super) const RULER_TOLERANCE: usize = 68;
pub(super) const RULER_TOLERANCE_DOWN: usize = 69;
pub(super) const RULER_TOLERANCE_UP: usize = 70;
pub(super) const RULER_COLOR: usize = 71;
pub(super) const RULER_UNIT_PX: usize = 72;
pub(super) const RULER_UNIT_IN: usize = 73;
pub(super) const RULER_UNIT_CM: usize = 74;
pub(super) const RULER_UNIT_MM: usize = 75;
pub(super) const RULER_UNIT_DIP: usize = 76;
pub(super) const RULER_STATUS: usize = 114;
pub(super) const RULER_LEAD: usize = 115;
pub(super) const RESIZE_OPEN: usize = 120;
pub(super) const RESIZE_SHRINK: usize = 121;
pub(super) const RESIZE_REPLACE: usize = 122;
pub(super) const RESIZE_ORIENTATION: usize = 123;
pub(super) const RESIZE_METADATA: usize = 124;
pub(super) const RESIZE_KEEP_DATE: usize = 125;
pub(super) const RESIZE_JPEG: usize = 126;
pub(super) const RESIZE_JPEG_DOWN: usize = 127;
pub(super) const RESIZE_JPEG_UP: usize = 128;
pub(super) const RESIZE_FILE_LABEL: usize = 129;
pub(super) const RESIZE_FILENAME: usize = 130;
pub(super) const RESIZE_FILE_HINT: usize = 131;
pub(super) const RESIZE_ENC_LABEL: usize = 132;
pub(super) const RESIZE_ENC_JPEG: usize = 133;
pub(super) const RESIZE_ENC_PNG: usize = 134;
pub(super) const RESIZE_ENC_BMP: usize = 135;
pub(super) const RESIZE_ENC_GIF: usize = 136;
pub(super) const RESIZE_ENC_TIFF: usize = 137;
pub(super) const RESIZE_PNG_LABEL: usize = 138;
pub(super) const RESIZE_PNG_DEFAULT: usize = 139;
pub(super) const RESIZE_PNG_ON: usize = 140;
pub(super) const RESIZE_PNG_OFF: usize = 141;
pub(super) const RESIZE_TIFF: usize = 142;
pub(super) const RESIZE_TIFF_DOWN: usize = 143;
pub(super) const RESIZE_TIFF_UP: usize = 144;
const LABEL_NAV_GENERAL: &str = "Genel";
const LABEL_NAV_ALWAYS_ON: &str = "AlwaysOn";
const LABEL_NAV_ALWAYS_TOP: &str = "AlwaysTop";
const LABEL_NAV_SCREEN_RULER: &str = "ScreenRuler";
const LABEL_NAV_IMAGE_RESIZER: &str = "ImageResizer";
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
const LABEL_GENERAL_LEAD: &str = "Sürüm bilgileri, güncellemeler ve uygulamadan çıkış.";
const LABEL_ABOUT_NAME: &str = "PowerKit";
const LABEL_ABOUT_SUB: &str = "AlwaysOn · AlwaysTop · ScreenRuler · ImageResizer";
const LABEL_GENERAL_UPDATES: &str = "Güncellemeler";
const LABEL_GENERAL_SESSION: &str = "Uygulama";
const ABOUT: usize = 150;
const HEAD_UPDATES: usize = 151;
const HEAD_SESSION: usize = 152;
const PACKAGE_ID: &str = "Pirimera.PowerKit";
const ABOUT_PAD: i32 = 20;
const ABOUT_TITLE: i32 = 26;
const ABOUT_SUB: i32 = 22;
const ABOUT_SPLIT: i32 = 14;
const ABOUT_ROW: i32 = 30;
const ABOUT_BOTTOM: i32 = 16;
const LABEL_ALWAYS_ON_TITLE: &str = "AlwaysOn";
const LABEL_ALWAYS_ON_LEAD: &str =
    "Sistemi uyanık tutar. İsterseniz boşta ekran uykusunu da engeller.";
const LABEL_ALWAYS_TOP_TITLE: &str = "AlwaysTop";
const LABEL_ALWAYS_TOP_LEAD: &str =
    "Odaklanan pencereleri üstte tutar. Pinler ve saydamlık kaydedilmez.";
const LABEL_TOP_OFF: &str = "AlwaysTop kapalı";
const LABEL_TOP_LEAD: &str =
    "Kenarlık ve kısayollar kaydedilir. Sabitlenen pencerenin saydamlığı kaydedilmez.";
const LABEL_RULER_TITLE: &str = "ScreenRuler";
const LABEL_RULER_PAGE_LEAD: &str =
    "Ekrandaki pikselleri ölçer. Varsayılan kısayol Win+Ctrl+Shift+M.";
const LABEL_RULER_OFF: &str = "ScreenRuler kapalı";
const LABEL_RULER_START: &str = "Ölçümü başlat";
const LABEL_RULER_NONE: &str = "Yok";
const LABEL_RULER_BOUNDS: &str = "Sınırlar";
const LABEL_RULER_SPACING: &str = "Aralık";
const LABEL_RULER_HORIZONTAL: &str = "Yatay";
const LABEL_RULER_VERTICAL: &str = "Dikey";
const LABEL_RULER_CONTINUOUS: &str = "Ölçerken ekranı sürekli yakala";
const LABEL_RULER_CHANNEL: &str = "Renk kanalı başına kenar algılama";
const LABEL_RULER_FEET: &str = "Çapraz çizgilere ayak çiz";
const LABEL_RULER_TOLERANCE: &str = "Piksel toleransı";
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
const LABEL_RESIZE_FILE: &str = "Dosya adı";
const LABEL_RESIZE_FILE_HINT: &str = "%1 ad · %2 boyut · %3–%4 seçilen · %5–%6 sonuç";
const LABEL_RESIZE_ENCODER: &str = "Yedek kodlayıcı";
const LABEL_RESIZE_PNG: &str = "PNG geçişim";
const SS_NOPREFIX: u32 = 0x0080;
const SS_OWNERDRAW: u32 = 0x000D;
const SS_EDITCONTROL: u32 = 0x2000;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    AlwaysOn,
    AlwaysTop,
    ScreenRuler,
    ImageResizer,
    General,
}
#[derive(Clone, Copy)]
enum Kind {
    Button,
    Check,
    Static,
    Heading,
    About,
    Nav,
    Mode,
    Hotkey,
    Color,
    Edit,
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
        id: NAV_SCREEN_RULER,
        label: LABEL_NAV_SCREEN_RULER,
        kind: Kind::Nav,
        page: None,
    },
    Spec {
        id: NAV_IMAGE_RESIZER,
        label: LABEL_NAV_IMAGE_RESIZER,
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
        id: RULER_STATUS,
        label: LABEL_RULER_OFF,
        kind: Kind::Static,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: MEASURE,
        label: LABEL_RULER_START,
        kind: Kind::Button,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_HOTKEY,
        label: LABEL_HOTKEY_PIN,
        kind: Kind::Hotkey,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_MODE_NONE,
        label: LABEL_RULER_NONE,
        kind: Kind::Mode,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_MODE_BOUNDS,
        label: LABEL_RULER_BOUNDS,
        kind: Kind::Mode,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_MODE_SPACING,
        label: LABEL_RULER_SPACING,
        kind: Kind::Mode,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_MODE_HORIZONTAL,
        label: LABEL_RULER_HORIZONTAL,
        kind: Kind::Mode,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_MODE_VERTICAL,
        label: LABEL_RULER_VERTICAL,
        kind: Kind::Mode,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_CONTINUOUS,
        label: LABEL_RULER_CONTINUOUS,
        kind: Kind::Check,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_CHANNEL,
        label: LABEL_RULER_CHANNEL,
        kind: Kind::Check,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_FEET,
        label: LABEL_RULER_FEET,
        kind: Kind::Check,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_TOLERANCE,
        label: LABEL_RULER_TOLERANCE,
        kind: Kind::Static,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_TOLERANCE_DOWN,
        label: LABEL_STEPPER_DOWN,
        kind: Kind::Button,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_TOLERANCE_UP,
        label: LABEL_STEPPER_UP,
        kind: Kind::Button,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_UNIT_PX,
        label: LABEL_RULER_PX,
        kind: Kind::Mode,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_UNIT_IN,
        label: LABEL_RULER_IN,
        kind: Kind::Mode,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_UNIT_CM,
        label: LABEL_RULER_CM,
        kind: Kind::Mode,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_UNIT_MM,
        label: LABEL_RULER_MM,
        kind: Kind::Mode,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_UNIT_DIP,
        label: LABEL_RULER_DIP,
        kind: Kind::Mode,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_COLOR,
        label: LABEL_RULER_COLOR,
        kind: Kind::Color,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RULER_LEAD,
        label: LABEL_RULER_LEAD,
        kind: Kind::Static,
        page: Some(Page::ScreenRuler),
    },
    Spec {
        id: RESIZE_OPEN,
        label: LABEL_RESIZE_OPEN,
        kind: Kind::Button,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_SHRINK,
        label: LABEL_RESIZE_SHRINK,
        kind: Kind::Check,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_REPLACE,
        label: LABEL_RESIZE_REPLACE,
        kind: Kind::Check,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_ORIENTATION,
        label: LABEL_RESIZE_ORIENTATION,
        kind: Kind::Check,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_METADATA,
        label: LABEL_RESIZE_METADATA,
        kind: Kind::Check,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_KEEP_DATE,
        label: LABEL_RESIZE_KEEP_DATE,
        kind: Kind::Check,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_JPEG,
        label: "JPEG kalitesi",
        kind: Kind::Static,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_JPEG_DOWN,
        label: LABEL_STEPPER_DOWN,
        kind: Kind::Button,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_JPEG_UP,
        label: LABEL_STEPPER_UP,
        kind: Kind::Button,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_FILE_LABEL,
        label: LABEL_RESIZE_FILE,
        kind: Kind::Static,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_FILENAME,
        label: "%1 (%2)",
        kind: Kind::Edit,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_FILE_HINT,
        label: LABEL_RESIZE_FILE_HINT,
        kind: Kind::Static,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_ENC_LABEL,
        label: LABEL_RESIZE_ENCODER,
        kind: Kind::Static,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_ENC_JPEG,
        label: crate::image_resizer::FallbackEncoder::Jpeg.label(),
        kind: Kind::Mode,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_ENC_PNG,
        label: crate::image_resizer::FallbackEncoder::Png.label(),
        kind: Kind::Mode,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_ENC_BMP,
        label: crate::image_resizer::FallbackEncoder::Bmp.label(),
        kind: Kind::Mode,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_ENC_GIF,
        label: crate::image_resizer::FallbackEncoder::Gif.label(),
        kind: Kind::Mode,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_ENC_TIFF,
        label: crate::image_resizer::FallbackEncoder::Tiff.label(),
        kind: Kind::Mode,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_PNG_LABEL,
        label: LABEL_RESIZE_PNG,
        kind: Kind::Static,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_PNG_DEFAULT,
        label: crate::image_resizer::PngInterlace::Default.label(),
        kind: Kind::Mode,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_PNG_ON,
        label: crate::image_resizer::PngInterlace::On.label(),
        kind: Kind::Mode,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_PNG_OFF,
        label: crate::image_resizer::PngInterlace::Off.label(),
        kind: Kind::Mode,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_TIFF,
        label: "TIFF sıkıştırma",
        kind: Kind::Static,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_TIFF_DOWN,
        label: LABEL_STEPPER_DOWN,
        kind: Kind::Button,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: RESIZE_TIFF_UP,
        label: LABEL_STEPPER_UP,
        kind: Kind::Button,
        page: Some(Page::ImageResizer),
    },
    Spec {
        id: ABOUT,
        label: "",
        kind: Kind::About,
        page: Some(Page::General),
    },
    Spec {
        id: HEAD_UPDATES,
        label: LABEL_GENERAL_UPDATES,
        kind: Kind::Heading,
        page: Some(Page::General),
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
        id: HEAD_SESSION,
        label: LABEL_GENERAL_SESSION,
        kind: Kind::Heading,
        page: Some(Page::General),
    },
    Spec {
        id: EXIT,
        label: LABEL_EXIT,
        kind: Kind::Button,
        page: Some(Page::General),
    },
    Spec {
        id: HINT,
        label: "",
        kind: Kind::Static,
        page: Some(Page::General),
    },
];
#[derive(Clone, Copy)]
pub(super) struct Metrics {
    pub nav_width: i32,
    pub pad: i32,
    pub control_h: i32,
    pub row_h: i32,
    pub title_h: i32,
    pub lead_h: i32,
    pub hint_h: i32,
    pub nav_item_h: i32,
    pub card_radius: i32,
    pub control_radius: i32,
    pub mode_min_w: i32,
    pub unit_min_w: i32,
    pub about_h: i32,
    pub section_h: i32,
    pub action_w: i32,
}
pub(super) fn metrics(dpi: u32) -> Metrics {
    let scale = |n: i32| n * dpi as i32 / 96;
    Metrics {
        nav_width: scale(220),
        pad: scale(24),
        control_h: scale(36),
        row_h: scale(48),
        title_h: scale(36),
        lead_h: scale(44),
        hint_h: scale(56),
        nav_item_h: scale(40),
        card_radius: scale(8),
        control_radius: scale(6),
        mode_min_w: scale(112),
        unit_min_w: scale(132),
        about_h: scale(
            ABOUT_PAD + ABOUT_TITLE + ABOUT_SUB + ABOUT_SPLIT + ABOUT_ROW * 5 + ABOUT_BOTTOM,
        ),
        section_h: scale(28),
        action_w: scale(260),
    }
}
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
pub(super) fn resize_encoder(id: usize) -> Option<crate::image_resizer::FallbackEncoder> {
    use crate::image_resizer::FallbackEncoder;
    Some(match id {
        RESIZE_ENC_JPEG => FallbackEncoder::Jpeg,
        RESIZE_ENC_PNG => FallbackEncoder::Png,
        RESIZE_ENC_BMP => FallbackEncoder::Bmp,
        RESIZE_ENC_GIF => FallbackEncoder::Gif,
        RESIZE_ENC_TIFF => FallbackEncoder::Tiff,
        _ => return None,
    })
}
pub(super) fn resize_png(id: usize) -> Option<crate::image_resizer::PngInterlace> {
    use crate::image_resizer::PngInterlace;
    Some(match id {
        RESIZE_PNG_DEFAULT => PngInterlace::Default,
        RESIZE_PNG_ON => PngInterlace::On,
        RESIZE_PNG_OFF => PngInterlace::Off,
        _ => return None,
    })
}
pub(super) fn encoder_button(encoder: crate::image_resizer::FallbackEncoder) -> usize {
    use crate::image_resizer::FallbackEncoder;
    match encoder {
        FallbackEncoder::Jpeg => RESIZE_ENC_JPEG,
        FallbackEncoder::Png => RESIZE_ENC_PNG,
        FallbackEncoder::Bmp => RESIZE_ENC_BMP,
        FallbackEncoder::Gif => RESIZE_ENC_GIF,
        FallbackEncoder::Tiff => RESIZE_ENC_TIFF,
    }
}
pub(super) fn png_button(png: crate::image_resizer::PngInterlace) -> usize {
    use crate::image_resizer::PngInterlace;
    match png {
        PngInterlace::Default => RESIZE_PNG_DEFAULT,
        PngInterlace::On => RESIZE_PNG_ON,
        PngInterlace::Off => RESIZE_PNG_OFF,
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
fn tile_group(id: usize) -> Option<&'static [usize]> {
    if MODE_ORDER.contains(&id) {
        Some(&MODE_ORDER)
    } else if RULER_MODE_ORDER.contains(&id) {
        Some(&RULER_MODE_ORDER)
    } else if RULER_UNIT_ORDER.contains(&id) {
        Some(&RULER_UNIT_ORDER)
    } else if RESIZE_ENCODER_ORDER.contains(&id) {
        Some(&RESIZE_ENCODER_ORDER)
    } else if RESIZE_PNG_ORDER.contains(&id) {
        Some(&RESIZE_PNG_ORDER)
    } else {
        None
    }
}
fn stepper_ids(id: usize) -> Option<(usize, usize, usize)> {
    match id {
        FRAME_OPACITY | FRAME_OPACITY_DOWN | FRAME_OPACITY_UP => {
            Some((FRAME_OPACITY, FRAME_OPACITY_DOWN, FRAME_OPACITY_UP))
        }
        FRAME_THICKNESS | FRAME_THICK_DOWN | FRAME_THICK_UP => {
            Some((FRAME_THICKNESS, FRAME_THICK_DOWN, FRAME_THICK_UP))
        }
        RULER_TOLERANCE | RULER_TOLERANCE_DOWN | RULER_TOLERANCE_UP => {
            Some((RULER_TOLERANCE, RULER_TOLERANCE_DOWN, RULER_TOLERANCE_UP))
        }
        RESIZE_JPEG | RESIZE_JPEG_DOWN | RESIZE_JPEG_UP => {
            Some((RESIZE_JPEG, RESIZE_JPEG_DOWN, RESIZE_JPEG_UP))
        }
        RESIZE_TIFF | RESIZE_TIFF_DOWN | RESIZE_TIFF_UP => {
            Some((RESIZE_TIFF, RESIZE_TIFF_DOWN, RESIZE_TIFF_UP))
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
        NAV_SCREEN_RULER => Some(Page::ScreenRuler),
        NAV_IMAGE_RESIZER => Some(Page::ImageResizer),
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
fn before_gap(id: usize, m: Metrics) -> i32 {
    if matches!(id, HEAD_UPDATES | HEAD_SESSION) {
        m.pad / 2
    } else {
        0
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
    pub section: HFONT,
    pub nav: HFONT,
}
impl Fonts {
    fn create(dpi: u32) -> io::Result<Self> {
        Ok(Self {
            body: ui_font(dpi, 14, FW_NORMAL as i32)?,
            title: ui_font(dpi, 28, FW_SEMIBOLD as i32)?,
            section: ui_font(dpi, 14, FW_SEMIBOLD as i32)?,
            nav: ui_font(dpi, 14, FW_NORMAL as i32)?,
        })
    }
    fn release(&mut self) {
        for font in [
            &mut self.body,
            &mut self.title,
            &mut self.section,
            &mut self.nav,
        ] {
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
    accent_soft: windows_sys::Win32::Graphics::Gdi::HBRUSH,
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
                accent_soft: CreateSolidBrush(theme.accent_soft),
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
            &mut self.accent_soft,
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
    pub ruler_mode: usize,
    pub ruler_unit: usize,
    pub capturing: Option<usize>,
    pub frame_preview: u32,
    pub ruler_preview: u32,
    pub resize_encoder: usize,
    pub resize_png: usize,
}
impl Default for Chrome {
    fn default() -> Self {
        Self {
            fonts: Fonts::default(),
            brushes: Brushes::default(),
            theme: None,
            page: Page::default(),
            selected_mode: OFF,
            ruler_mode: RULER_MODE_NONE,
            ruler_unit: RULER_UNIT_PX,
            capturing: None,
            frame_preview: 0,
            ruler_preview: crate::screen_ruler::DEFAULT_LINE_COLOR,
            resize_encoder: RESIZE_ENC_JPEG,
            resize_png: RESIZE_PNG_DEFAULT,
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
        Kind::Static | Kind::Heading | Kind::About => "STATIC",
        Kind::Edit => "EDIT",
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
            Kind::Heading => SS_NOPREFIX,
            Kind::About => SS_NOPREFIX | SS_OWNERDRAW,
            Kind::Edit => WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
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
    if matches!(id, HINT | TOP_LEAD | PAGE_LEAD | RULER_LEAD) {
        m.hint_h.max(m.lead_h)
    } else if id == PAGE_TITLE {
        m.title_h
    } else if id == ABOUT {
        m.about_h
    } else if matches!(id, HEAD_UPDATES | HEAD_SESSION) {
        m.section_h
    } else if spec(id)
        .is_some_and(|item| matches!(item.kind, Kind::Check | Kind::Hotkey | Kind::Color))
    {
        m.row_h
    } else {
        m.control_h
    }
}
fn fill_rounded(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    rect: RECT,
    radius: i32,
    brush: windows_sys::Win32::Graphics::Gdi::HBRUSH,
    pen: windows_sys::Win32::Graphics::Gdi::HPEN,
) {
    // SAFETY: hdc is the caller's draw DC; brush/pen stay owned by Chrome.
    unsafe {
        let old_brush = SelectObject(hdc, brush as HGDIOBJ);
        let old_pen = SelectObject(hdc, pen as HGDIOBJ);
        RoundRect(
            hdc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radius,
            radius,
        );
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
    }
}
fn primary_action(id: usize) -> bool {
    id == UPDATE
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
            section: chrome.fonts.section,
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
        let gap = (m.pad / 3).max(8);
        let mut tiles_placed = std::collections::HashSet::new();
        let mut steppers_placed = std::collections::HashSet::new();
        for item in SPECS {
            let control = GetDlgItem(hwnd, item.id as i32);
            if control.is_null() {
                continue;
            }
            let show = visible_on(page, item);
            if let Some(group) = tile_group(item.id) {
                let key = group[0];
                if show && tiles_placed.insert(key) {
                    let min_cell = if key == RULER_UNIT_PX {
                        m.unit_min_w
                    } else {
                        m.mode_min_w
                    };
                    let cells = wrap_cells(content_w, min_cell, gap, m.control_h, group.len());
                    for (index, id) in group.iter().enumerate() {
                        let cell = cells[index];
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
                    y += cells
                        .iter()
                        .map(|cell| cell.bottom)
                        .max()
                        .unwrap_or(m.control_h)
                        + gap;
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
                if show {
                    y += before_gap(item.id, m);
                }
                let top = y;
                if show {
                    y += height + gap;
                    if item.id == PAGE_LEAD {
                        y += m.pad / 2;
                    }
                }
                let width = if matches!(item.id, UPDATE | EXIT) {
                    content_w.min(m.action_w)
                } else {
                    content_w
                };
                (x0, top, width, height)
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
    } else if spec(id).is_some_and(|item| matches!(item.kind, Kind::Heading)) {
        fonts.section
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
    for item in SPECS {
        let control = unsafe { GetDlgItem(hwnd, item.id as i32) };
        if !control.is_null() {
            unsafe {
                InvalidateRect(control, null(), 1);
            }
        }
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
        if let Some(theme) = chrome.theme {
            let hair = CreateSolidBrush(theme.stroke);
            if !hair.is_null() {
                let divider = RECT {
                    left: m.nav_width - 1,
                    top: m.pad / 2,
                    right: m.nav_width,
                    bottom: client.bottom - m.pad / 2,
                };
                FillRect(hdc, &divider, hair);
                DeleteObject(hair);
            }
        }
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
    if !matches!(msg, WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT)
        || chrome.theme.is_none()
    {
        return None;
    }
    let theme = chrome.theme?;
    let id = unsafe { GetWindowLongPtrW(child, GWLP_ID) } as usize;
    let muted = matches!(
        id,
        HINT | PAGE_LEAD | TOP_LEAD | UPDATE_STATUS | RULER_LEAD | RESIZE_FILE_HINT
    ) || spec(id).is_some_and(|item| matches!(item.kind, Kind::Heading));
    let heading = spec(id).is_some_and(|item| matches!(item.kind, Kind::Heading));
    let (brush, fill) = if spec(id).is_some_and(|item| matches!(item.kind, Kind::Nav)) {
        (chrome.brushes.pane, theme.pane)
    } else if matches!(msg, WM_CTLCOLOREDIT) {
        (chrome.brushes.card, theme.card)
    } else {
        (chrome.brushes.background, theme.background)
    };
    // SAFETY: hdc is the control color DC; brush stays owned by Chrome.
    unsafe {
        SetTextColor(
            hdc,
            if muted || heading {
                theme.muted
            } else {
                theme.text
            },
        );
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
        Kind::About => draw_about(hwnd, draw, chrome, theme),
        Kind::Static | Kind::Heading | Kind::Edit => false,
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
    let width = scale(44);
    let height = scale(22);
    let pad = scale(12);
    let top = (row.top + row.bottom - height) / 2;
    RECT {
        left: row.right - pad - width,
        top,
        right: row.right - pad,
        bottom: top + height,
    }
}
fn toggle_thumb(on: bool, track: RECT, dpi: u32) -> RECT {
    let height = (track.bottom - track.top).max(1);
    let inset = (3 * dpi as i32 / 96).clamp(2, (height / 3).max(2));
    let size = (height - inset * 2).max(1);
    let left = if on {
        track.right - inset - size
    } else {
        track.left + inset
    };
    RECT {
        left,
        top: track.top + inset,
        right: left + size,
        bottom: track.top + inset + size,
    }
}
struct ToggleColors {
    card: u32,
    track: u32,
    thumb: u32,
}
fn color_bgr(color: u32) -> [u8; 3] {
    [
        ((color >> 16) & 0xff) as u8,
        ((color >> 8) & 0xff) as u8,
        (color & 0xff) as u8,
    ]
}
fn in_toggle_track(x: f32, y: f32, track: RECT) -> bool {
    let left = track.left as f32;
    let top = track.top as f32;
    let right = track.right as f32;
    let bottom = track.bottom as f32;
    let radius = (bottom - top).min(right - left) * 0.5;
    if radius <= 0.0 {
        return false;
    }
    let cy = (top + bottom) * 0.5;
    let x0 = left + radius;
    let x1 = right - radius;
    let px = x.clamp(x0, x1);
    let dx = x - px;
    let dy = y - cy;
    dx * dx + dy * dy <= radius * radius
}
fn in_thumb(x: f32, y: f32, thumb: RECT) -> bool {
    let cx = (thumb.left + thumb.right) as f32 * 0.5;
    let cy = (thumb.top + thumb.bottom) as f32 * 0.5;
    let radius = (thumb.right - thumb.left).min(thumb.bottom - thumb.top) as f32 * 0.5;
    let dx = x - cx;
    let dy = y - cy;
    radius > 0.0 && dx * dx + dy * dy <= radius * radius
}
fn raster_toggle(
    origin: (i32, i32),
    size: (i32, i32),
    track: RECT,
    thumb: RECT,
    colors: ToggleColors,
) -> Vec<u8> {
    let (width, height) = size;
    if width <= 0 || height <= 0 || width > 4096 || height > 4096 {
        return Vec::new();
    }
    const SAMPLES: u32 = 4;
    let card = color_bgr(colors.card);
    let track_color = color_bgr(colors.track);
    let thumb_color = color_bgr(colors.thumb);
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let mut b = 0u32;
            let mut g = 0u32;
            let mut r = 0u32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = origin.0 as f32 + x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let py = origin.1 as f32 + y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    let sample = if in_thumb(px, py, thumb) {
                        thumb_color
                    } else if in_toggle_track(px, py, track) {
                        track_color
                    } else {
                        card
                    };
                    b += sample[0] as u32;
                    g += sample[1] as u32;
                    r += sample[2] as u32;
                }
            }
            let i = ((y * width + x) * 4) as usize;
            let n = SAMPLES * SAMPLES;
            pixels[i] = (b / n) as u8;
            pixels[i + 1] = (g / n) as u8;
            pixels[i + 2] = (r / n) as u8;
            pixels[i + 3] = 255;
        }
    }
    pixels
}
fn toggle_bitmap_bounds(track: RECT, clip: RECT) -> RECT {
    let pad = 2;
    RECT {
        left: (track.left - pad).max(clip.left),
        top: (track.top - pad).max(clip.top),
        right: (track.right + pad).min(clip.right),
        bottom: (track.bottom + pad).min(clip.bottom),
    }
}
fn blit_bgra(hdc: HDC, x: i32, y: i32, width: i32, height: i32, pixels: &[u8]) -> bool {
    let byte_count = (width * height * 4) as usize;
    if width <= 0 || height <= 0 || pixels.len() < byte_count {
        return false;
    }
    let mut info = unsafe { std::mem::zeroed::<BITMAPINFO>() };
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let mut bits = null_mut();
    // SAFETY: info describes a top-down 32-bit DIB. bits stays valid until DeleteObject,
    // and the compatible DC is deleted before this function returns. hdc is the live draw DC.
    unsafe {
        let bitmap = CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bitmap.is_null() || bits.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            return false;
        }
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast(), byte_count);
        let memory = CreateCompatibleDC(hdc);
        if memory.is_null() {
            DeleteObject(bitmap);
            return false;
        }
        let previous = SelectObject(memory, bitmap);
        let copied = BitBlt(hdc, x, y, width, height, memory, 0, 0, SRCCOPY) != 0;
        SelectObject(memory, previous);
        DeleteDC(memory);
        DeleteObject(bitmap);
        copied
    }
}
pub(super) fn control_string(hwnd: HWND) -> String {
    String::from_utf16_lossy(&control_text(hwnd))
        .trim_end_matches('\0')
        .to_string()
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
fn paint_label(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    font: HFONT,
    text: &str,
    bounds: &mut RECT,
    color: u32,
    flags: u32,
) {
    let mut wide_text = wide(text);
    // SAFETY: hdc is the caller’s draw DC; the font stays owned by Chrome.
    unsafe {
        SetTextColor(hdc, color);
        let previous = SelectObject(hdc, font as HGDIOBJ);
        DrawTextW(hdc, wide_text.as_mut_ptr(), -1, bounds, flags);
        SelectObject(hdc, previous);
    }
}
fn draw_about(
    hwnd: HWND,
    draw: &DRAWITEMSTRUCT,
    chrome: &Chrome,
    theme: theme::WindowTheme,
) -> bool {
    if chrome.fonts.body.is_null() || chrome.fonts.section.is_null() {
        return false;
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let scale = |n: i32| n * dpi as i32 / 96;
    let facts = installed_about_facts();
    let text = if theme.high_contrast {
        unsafe { GetSysColor(COLOR_WINDOWTEXT) }
    } else {
        theme.text
    };
    let muted = if theme.high_contrast {
        unsafe { GetSysColor(COLOR_GRAYTEXT) }
    } else {
        theme.muted
    };
    let radius = scale(8).max(4);
    let pad = scale(ABOUT_PAD);
    // SAFETY: DRAWITEMSTRUCT hdc/rcItem are valid for this WM_DRAWITEM call.
    // The hairline brush is deleted before return; Chrome keeps its fonts and brushes.
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, chrome.brushes.background);
        fill_rounded(
            draw.hDC,
            draw.rcItem,
            radius,
            chrome.brushes.card,
            chrome.brushes.stroke,
        );
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        let left = draw.rcItem.left + pad;
        let right = draw.rcItem.right - pad;
        let mut cursor = draw.rcItem.top + pad;
        let version = facts[0].1.as_str();
        let mut version_bounds = RECT {
            left,
            top: cursor,
            right,
            bottom: cursor + scale(ABOUT_TITLE),
        };
        paint_label(
            draw.hDC,
            chrome.fonts.section,
            version,
            &mut version_bounds,
            text,
            DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
        );
        let version_w = (version_bounds.right - version_bounds.left).max(0);
        let mut version_rect = RECT {
            left: (right - version_w).max(left),
            top: cursor,
            right,
            bottom: cursor + scale(ABOUT_TITLE),
        };
        paint_label(
            draw.hDC,
            chrome.fonts.section,
            version,
            &mut version_rect,
            text,
            DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
        let mut name_rect = RECT {
            left,
            top: cursor,
            right: (version_rect.left - scale(12)).max(left),
            bottom: cursor + scale(ABOUT_TITLE),
        };
        paint_label(
            draw.hDC,
            chrome.fonts.section,
            LABEL_ABOUT_NAME,
            &mut name_rect,
            text,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
        cursor += scale(ABOUT_TITLE);
        let mut sub_rect = RECT {
            left,
            top: cursor,
            right,
            bottom: cursor + scale(ABOUT_SUB),
        };
        paint_label(
            draw.hDC,
            chrome.fonts.body,
            LABEL_ABOUT_SUB,
            &mut sub_rect,
            muted,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
        cursor += scale(ABOUT_SUB);
        let split = scale(ABOUT_SPLIT);
        let line_top = cursor + (split / 2).max(1);
        let hair = CreateSolidBrush(theme.stroke);
        if !hair.is_null() {
            let line = RECT {
                left,
                top: line_top,
                right,
                bottom: line_top + 1,
            };
            FillRect(draw.hDC, &line, hair);
            DeleteObject(hair);
        }
        cursor += split;
        let label_w = scale(108);
        let row_h = scale(ABOUT_ROW);
        for (label, value) in &facts {
            let mut label_rect = RECT {
                left,
                top: cursor,
                right: left + label_w,
                bottom: cursor + row_h,
            };
            paint_label(
                draw.hDC,
                chrome.fonts.body,
                label,
                &mut label_rect,
                muted,
                DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
            );
            let mut value_rect = RECT {
                left: left + label_w,
                top: cursor,
                right,
                bottom: cursor + row_h,
            };
            paint_label(
                draw.hDC,
                chrome.fonts.body,
                value,
                &mut value_rect,
                text,
                DT_LEFT
                    | DT_SINGLELINE
                    | DT_VCENTER
                    | DT_NOPREFIX
                    | DT_END_ELLIPSIS
                    | DT_PATH_ELLIPSIS,
            );
            cursor += row_h;
        }
    }
    true
}
fn draw_nav(
    hwnd: HWND,
    draw: &DRAWITEMSTRUCT,
    chrome: &Chrome,
    theme: theme::WindowTheme,
    item: &Spec,
) -> bool {
    let selected = page_from_nav(item.id) == Some(chrome.page);
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let radius = metrics(dpi).control_radius.max(4);
    // SAFETY: DRAWITEMSTRUCT hdc/rcItem are valid for this WM_DRAWITEM call.
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, chrome.brushes.pane);
        if selected {
            fill_rounded(
                draw.hDC,
                draw.rcItem,
                radius,
                chrome.brushes.selected,
                GetStockObject(NULL_PEN),
            );
        }
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        SetTextColor(
            draw.hDC,
            if selected && theme.high_contrast {
                GetSysColor(COLOR_HIGHLIGHTTEXT)
            } else if selected {
                theme.accent
            } else {
                theme.text
            },
        );
        let mut text = wide(item.label);
        let mut bounds = draw.rcItem;
        bounds.left += 14 * dpi as i32 / 96;
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
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let m = metrics(dpi);
    let id = draw.CtlID as usize;
    let primary = primary_action(id) && !theme.high_contrast;
    let brush = if primary {
        if pressed {
            chrome.brushes.button_hot
        } else {
            chrome.brushes.accent
        }
    } else if pressed {
        chrome.brushes.button_hot
    } else {
        chrome.brushes.card
    };
    let text = if primary {
        contrasting_text(theme.accent)
    } else if theme.high_contrast {
        unsafe { GetSysColor(COLOR_BTNTEXT) }
    } else {
        theme.text
    };
    // SAFETY: DRAWITEMSTRUCT hdc/rcItem are valid for this WM_DRAWITEM call.
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, chrome.brushes.background);
        let pen = if primary {
            GetStockObject(NULL_PEN)
        } else {
            chrome.brushes.stroke
        };
        fill_rounded(draw.hDC, draw.rcItem, m.control_radius, brush, pen);
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        SetTextColor(draw.hDC, text);
        let mut label = control_text(draw.hwndItem);
        let mut bounds = draw.rcItem;
        let hotkey = spec(id).is_some_and(|item| matches!(item.kind, Kind::Hotkey));
        if hotkey {
            bounds.left += 12 * dpi as i32 / 96;
        }
        let previous = SelectObject(draw.hDC, chrome.fonts.body as HGDIOBJ);
        DrawTextW(
            draw.hDC,
            label.as_mut_ptr(),
            -1,
            &mut bounds,
            if hotkey {
                DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS
            } else {
                DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS
            },
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
    let id = draw.CtlID as usize;
    let selected = if MODE_ORDER.contains(&id) {
        chrome.selected_mode == id
    } else if RULER_MODE_ORDER.contains(&id) {
        chrome.ruler_mode == id
    } else if RESIZE_ENCODER_ORDER.contains(&id) {
        chrome.resize_encoder == id
    } else if RESIZE_PNG_ORDER.contains(&id) {
        chrome.resize_png == id
    } else {
        chrome.ruler_unit == id
    };
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let m = metrics(dpi);
    let brush = if selected {
        if theme.high_contrast {
            chrome.brushes.accent
        } else {
            chrome.brushes.accent_soft
        }
    } else {
        chrome.brushes.card
    };
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, chrome.brushes.background);
        let pen = if selected && !theme.high_contrast {
            GetStockObject(NULL_PEN)
        } else {
            chrome.brushes.stroke
        };
        fill_rounded(draw.hDC, draw.rcItem, m.control_radius, brush, pen);
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        SetTextColor(
            draw.hDC,
            if selected {
                if theme.high_contrast {
                    GetSysColor(COLOR_HIGHLIGHTTEXT)
                } else {
                    theme.accent
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
    let m = metrics(dpi);
    let pad = (12 * dpi as i32 / 96).max(8);
    let size = (18 * dpi as i32 / 96).max(14);
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, chrome.brushes.background);
        fill_rounded(
            draw.hDC,
            draw.rcItem,
            m.card_radius,
            chrome.brushes.card,
            chrome.brushes.stroke,
        );
        let swatch = RECT {
            left: draw.rcItem.left + pad,
            top: (draw.rcItem.top + draw.rcItem.bottom - size) / 2,
            right: draw.rcItem.left + pad + size,
            bottom: (draw.rcItem.top + draw.rcItem.bottom - size) / 2 + size,
        };
        let brush = CreateSolidBrush(if draw.CtlID as usize == RULER_COLOR {
            chrome.ruler_preview
        } else {
            chrome.frame_preview
        });
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
    let m = metrics(dpi);
    let track_color = if on { theme.accent } else { theme.toggle };
    let bounds = toggle_bitmap_bounds(track, draw.rcItem);
    let pixels = raster_toggle(
        (bounds.left, bounds.top),
        (bounds.right - bounds.left, bounds.bottom - bounds.top),
        track,
        thumb,
        ToggleColors {
            card: theme.card,
            track: track_color,
            thumb: theme.thumb,
        },
    );
    // SAFETY: DRAWITEMSTRUCT hdc/rcItem are valid for this WM_DRAWITEM call.
    // GDI Ellipse/RoundRect stay as a fallback; the normal path blits a sampled pill.
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, chrome.brushes.background);
        fill_rounded(
            draw.hDC,
            draw.rcItem,
            m.card_radius,
            chrome.brushes.card,
            chrome.brushes.stroke,
        );
        if !blit_bgra(
            draw.hDC,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            &pixels,
        ) {
            let track_brush = if on {
                chrome.brushes.accent
            } else {
                chrome.brushes.toggle
            };
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
        }
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
        let mut text_bounds = draw.rcItem;
        let gap = (8 * dpi as i32 / 96).max(6);
        text_bounds.right = track.left - gap;
        let previous = SelectObject(draw.hDC, chrome.fonts.body as HGDIOBJ);
        DrawTextW(
            draw.hDC,
            text.as_mut_ptr(),
            -1,
            &mut text_bounds,
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
pub(super) const CLIENT_WIDTH: i32 = 820;
pub(super) const CLIENT_HEIGHT: i32 = 920;
pub(super) const MIN_WIDTH: i32 = 700;
pub(super) const MIN_HEIGHT: i32 = 780;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_maps_pages_and_keeps_actions_grouped() {
        assert_eq!(page_from_nav(NAV_GENERAL), Some(Page::General));
        assert_eq!(page_from_nav(NAV_ALWAYS_ON), Some(Page::AlwaysOn));
        assert_eq!(page_from_nav(NAV_ALWAYS_TOP), Some(Page::AlwaysTop));
        assert_eq!(page_from_nav(NAV_SCREEN_RULER), Some(Page::ScreenRuler));
        assert_eq!(page_from_nav(NAV_IMAGE_RESIZER), Some(Page::ImageResizer));
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
        assert_eq!(
            general,
            [
                ABOUT,
                HEAD_UPDATES,
                UPDATE,
                UPDATE_STATUS,
                HEAD_SESSION,
                EXIT,
                HINT
            ]
        );
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
        assert_eq!(metrics(96).control_h, 36);
        assert_eq!(metrics(144).control_h, 54);
        assert_eq!(metrics(192).control_h, 72);
        assert_eq!(metrics(96).row_h, 48);
        assert_eq!(metrics(96).nav_width, 220);
        assert_eq!(metrics(144).nav_width, 330);
        let m = metrics(96);
        let content_left = m.nav_width + m.pad / 2;
        let content_right = 820 - m.pad / 2;
        assert!(content_left >= m.nav_width);
        assert!(content_right < 820);
        assert!(content_right - content_left > m.pad * 8);
    }
    #[test]
    fn page_copy_stays_in_turkish() {
        assert_eq!(page_copy(Page::General).0, "Genel");
        assert_eq!(page_copy(Page::AlwaysOn).0, "AlwaysOn");
        assert_eq!(page_copy(Page::AlwaysTop).0, "AlwaysTop");
        assert_eq!(page_copy(Page::ScreenRuler).0, "ScreenRuler");
        assert_eq!(page_copy(Page::ImageResizer).0, "ImageResizer");
        let m = metrics(96);
        let content_w = (CLIENT_WIDTH - m.nav_width - m.pad * 2).max(m.pad * 8);
        let gap = (m.pad / 3).max(8);
        let mut y = m.pad;
        let mut tiles = std::collections::HashSet::new();
        let mut steppers = std::collections::HashSet::new();
        for item in SPECS {
            if !visible_on(Page::ImageResizer, item) || matches!(item.kind, Kind::Nav) {
                continue;
            }
            if let Some(group) = tile_group(item.id) {
                if tiles.insert(group[0]) {
                    let min_cell = if group[0] == RULER_UNIT_PX {
                        m.unit_min_w
                    } else {
                        m.mode_min_w
                    };
                    let cells = wrap_cells(content_w, min_cell, gap, m.control_h, group.len());
                    y += cells
                        .iter()
                        .map(|cell| cell.bottom)
                        .max()
                        .unwrap_or(m.control_h)
                        + gap;
                }
                continue;
            }
            if let Some((label, _, _)) = stepper_ids(item.id) {
                if steppers.insert(label) {
                    y += m.control_h + gap;
                }
                continue;
            }
            y += before_gap(item.id, m);
            y += control_height(item.id, m) + gap;
            if item.id == PAGE_LEAD {
                y += m.pad / 2;
            }
        }
        assert!(
            y <= CLIENT_HEIGHT,
            "ImageResizer page ends at {y}, client is {CLIENT_HEIGHT}"
        );
        let mut general_y = m.pad;
        for item in SPECS {
            if !visible_on(Page::General, item) || matches!(item.kind, Kind::Nav) {
                continue;
            }
            general_y += before_gap(item.id, m);
            general_y += control_height(item.id, m) + gap;
            if item.id == PAGE_LEAD {
                general_y += m.pad / 2;
            }
        }
        assert!(
            general_y <= MIN_HEIGHT,
            "General page ends at {general_y}, minimum client is {MIN_HEIGHT}"
        );
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
        assert_eq!(
            metrics(96).about_h,
            ABOUT_PAD
                + ABOUT_TITLE
                + ABOUT_SUB
                + ABOUT_SPLIT
                + ABOUT_ROW * rows.len() as i32
                + ABOUT_BOTTOM
        );
        let installed = installed_about_facts();
        assert_eq!(installed[0].1, env!("CARGO_PKG_VERSION"));
        assert!(matches!(installed[1].1.as_str(), "ARM64" | "x64" | "diğer"));
        assert!(installed[2].1 == "win-arm64" || installed[2].1 == "win-x64");
        assert_eq!(installed[3].1, PACKAGE_ID);
        assert!(installed[4].1.contains("PowerKit"));
        assert!(installed[4].1.contains("settings.txt"));
        assert_eq!(page_copy(Page::General).1, LABEL_GENERAL_LEAD);
        assert!(matches!(
            spec(ABOUT).map(|item| item.kind),
            Some(Kind::About)
        ));
        assert_eq!(metrics(96).action_w, 260);
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
        assert_eq!(on.right - on.left, on.bottom - on.top);
        assert_eq!(off.right - off.left, off.bottom - off.top);
        assert!(on.right - on.left < track.bottom - track.top);
        assert_eq!(on.top - track.top, track.bottom - on.bottom);
        assert_eq!(track.right - on.right, on.top - track.top);
        assert_eq!(off.left - track.left, on.top - track.top);
        assert!(off.left >= track.left);
        assert!(on.right <= track.right);
        let track = RECT {
            left: 0,
            top: 0,
            right: 40,
            bottom: 20,
        };
        let thumb = toggle_thumb(true, track, 96);
        let pixels = raster_toggle(
            (0, 0),
            (40, 20),
            track,
            thumb,
            ToggleColors {
                card: 0x0020_2020,
                track: 0x00FF_0000,
                thumb: 0x00FF_FFFF,
            },
        );
        let at = |x: i32, y: i32| {
            let i = ((y * 40 + x) * 4) as usize;
            [pixels[i], pixels[i + 1], pixels[i + 2]]
        };
        let center_x = (thumb.left + thumb.right) / 2;
        let center_y = (thumb.top + thumb.bottom) / 2;
        assert_eq!(at(center_x, center_y), [255, 255, 255]);
        assert_eq!(at(thumb.left + 1, center_y), [255, 255, 255]);
        assert_eq!(at(thumb.right - 2, center_y), [255, 255, 255]);
        assert_eq!(at(center_x, thumb.top + 1), [255, 255, 255]);
        assert_eq!(at(center_x, thumb.bottom - 2), [255, 255, 255]);
        assert_ne!(at(thumb.left, thumb.top), [255, 255, 255]);
        assert_eq!(at(1, center_y), [255, 0, 0]);
        assert_eq!(at(39, center_y), [255, 0, 0]);
        assert_eq!(at(0, 0), [0x20, 0x20, 0x20]);
        let off_thumb = toggle_thumb(false, track, 96);
        let off_pixels = raster_toggle(
            (0, 0),
            (40, 20),
            track,
            off_thumb,
            ToggleColors {
                card: 0x0020_2020,
                track: 0x0080_8080,
                thumb: 0x00FF_FFFF,
            },
        );
        let off_at = |x: i32, y: i32| {
            let i = ((y * 40 + x) * 4) as usize;
            [off_pixels[i], off_pixels[i + 1], off_pixels[i + 2]]
        };
        let off_x = (off_thumb.left + off_thumb.right) / 2;
        assert_eq!(off_at(off_x, center_y), [255, 255, 255]);
        assert!(off_x < center_x);
        assert!(matches!(
            spec(DISPLAY).map(|item| item.kind),
            Some(Kind::Check)
        ));
        assert!(matches!(
            spec(SOUND).map(|item| item.kind),
            Some(Kind::Check)
        ));
        assert!(matches!(
            spec(RULER_FEET).map(|item| item.kind),
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
