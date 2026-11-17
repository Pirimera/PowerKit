//! Shared tray/settings command vocabulary.
//!
//! Win32 still carries commands as `WM_COMMAND` ids (tray menus require them).
//! `AppCommand` is the typed form used by dispatch and the settings shell; `id` /
//! `from_id` are the only adapters between the two.
use crate::always_on::{PRESET_MINUTES, Session};
use crate::image_resizer::{FallbackEncoder, PngInterlace};

pub(crate) const OFF: usize = 10;
pub(crate) const FOREVER: usize = 11;
pub(crate) const HALF_HOUR: usize = 12;
pub(crate) const HOUR: usize = 13;
pub(crate) const TWO_HOURS: usize = 14;
pub(crate) const DISPLAY: usize = 15;
pub(crate) const EXIT: usize = 16;
pub(crate) const OPEN: usize = 17;
pub(crate) const CUSTOM: usize = 19;
pub(crate) const PIN: usize = 20;
pub(crate) const SOUND: usize = 21;
pub(crate) const OPACITY_UP: usize = 22;
pub(crate) const OPACITY_DOWN: usize = 23;
pub(crate) const MEASURE: usize = 24;
pub(crate) const RESIZE: usize = 25;
pub(crate) const BIND: usize = 26;
pub(crate) const EXTEND: usize = 27;
pub(crate) const HOTKEY_PIN_EDIT: usize = 40;
pub(crate) const HOTKEY_OP_UP_EDIT: usize = 41;
pub(crate) const HOTKEY_OP_DOWN_EDIT: usize = 42;
pub(crate) const FRAME_ACCENT: usize = 43;
pub(crate) const FRAME_COLOR: usize = 44;
pub(crate) const FRAME_OPACITY_DOWN: usize = 46;
pub(crate) const FRAME_OPACITY_UP: usize = 47;
pub(crate) const FRAME_THICK_DOWN: usize = 49;
pub(crate) const FRAME_THICK_UP: usize = 50;
pub(crate) const RULER_HOTKEY: usize = 53;
pub(crate) const ALWAYS_ON_HOTKEY: usize = 54;
pub(crate) const RULER_MODE_NONE: usize = 60;
pub(crate) const RULER_MODE_BOUNDS: usize = 61;
pub(crate) const RULER_MODE_SPACING: usize = 62;
pub(crate) const RULER_MODE_HORIZONTAL: usize = 63;
pub(crate) const RULER_MODE_VERTICAL: usize = 64;
pub(crate) const RULER_CONTINUOUS: usize = 65;
pub(crate) const RULER_CHANNEL: usize = 66;
pub(crate) const RULER_FEET: usize = 67;
pub(crate) const RULER_TOLERANCE_DOWN: usize = 69;
pub(crate) const RULER_TOLERANCE_UP: usize = 70;
pub(crate) const RULER_COLOR: usize = 71;
pub(crate) const RULER_UNIT_PX: usize = 72;
pub(crate) const RULER_UNIT_IN: usize = 73;
pub(crate) const RULER_UNIT_CM: usize = 74;
pub(crate) const RULER_UNIT_MM: usize = 75;
pub(crate) const RULER_UNIT_DIP: usize = 76;
pub(crate) const RESIZE_OPEN: usize = 120;
pub(crate) const RESIZE_SHRINK: usize = 121;
pub(crate) const RESIZE_REPLACE: usize = 122;
pub(crate) const RESIZE_ORIENTATION: usize = 123;
pub(crate) const RESIZE_METADATA: usize = 124;
pub(crate) const RESIZE_KEEP_DATE: usize = 125;
pub(crate) const RESIZE_JPEG_DOWN: usize = 127;
pub(crate) const RESIZE_JPEG_UP: usize = 128;
pub(crate) const RESIZE_FILENAME: usize = 130;
pub(crate) const RESIZE_ENC_JPEG: usize = 133;
pub(crate) const RESIZE_ENC_PNG: usize = 134;
pub(crate) const RESIZE_ENC_BMP: usize = 135;
pub(crate) const RESIZE_ENC_GIF: usize = 136;
pub(crate) const RESIZE_ENC_TIFF: usize = 137;
pub(crate) const RESIZE_PNG_DEFAULT: usize = 139;
pub(crate) const RESIZE_PNG_ON: usize = 140;
pub(crate) const RESIZE_PNG_OFF: usize = 141;
pub(crate) const RESIZE_TIFF_DOWN: usize = 143;
pub(crate) const RESIZE_TIFF_UP: usize = 144;
pub(crate) const STARTUP: usize = 150;
pub(crate) const SHOW_TRAY: usize = 151;
pub(crate) const THEME_SYSTEM: usize = 155;
pub(crate) const THEME_LIGHT: usize = 156;
pub(crate) const THEME_DARK: usize = 157;
pub(crate) const LANG_SYSTEM: usize = 158;
pub(crate) const LANG_TR: usize = 159;
pub(crate) const OPEN_COLOR_SETTINGS: usize = 160;
pub(crate) const LANG_EN: usize = 161;
pub(crate) const LANG_AR: usize = 162;
pub(crate) const APPLY_SHELL_OPACITY: usize = 163;
pub(crate) const EXPIRY: usize = 164;
pub(crate) const APPLY_INTERVAL: usize = 165;
pub(crate) const ALWAYS_ON_WARN: usize = 166;
pub(crate) const AC_ONLY: usize = 167;
pub(crate) const BATTERY_FLOOR_DOWN: usize = 168;
pub(crate) const BATTERY_FLOOR_UP: usize = 169;
pub(crate) const POWER_STORE_REFRESH: usize = 170;
pub(crate) const POWER_STORE_ENSURE_LOADED: usize = 171;
pub(crate) const POWER_STORE_ACTION: usize = 175;
pub(crate) const POWER_STORE_CANCEL_PENDING: usize = 176;
pub(crate) const POWER_STORE_UPDATE_ALL: usize = 177;
pub(crate) const POWER_STORE_CANCEL_RUNNING: usize = 178;
pub(crate) const POWER_STORE_CHECK_NOW: usize = 181;
pub(crate) const POWER_STORE_OPEN: usize = 182;
pub(crate) const MEDIA_RENAMER_ACTION: usize = 190;
pub(crate) const MEDIA_RENAMER_OPEN: usize = 191;
pub(crate) const TOGGLE_MOUSE: usize = 200;
pub(crate) const POWER_MOUSE_ACTION: usize = 201;
pub(crate) const TOGGLE_MODULE_ALWAYS_ON: usize = 210;
pub(crate) const TOGGLE_MODULE_ALWAYS_TOP: usize = 211;
pub(crate) const TOGGLE_MODULE_SCREEN_RULER: usize = 212;
pub(crate) const TOGGLE_MODULE_IMAGE_RESIZER: usize = 213;
pub(crate) const TOGGLE_MODULE_POWER_STORE: usize = 214;
pub(crate) const TOGGLE_MODULE_MEDIA_RENAMER: usize = 215;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppCommand {
    AlwaysOnOff,
    AlwaysOnForever,
    AlwaysOnHalfHour,
    AlwaysOnHour,
    AlwaysOnTwoHours,
    AlwaysOnInterval,
    AlwaysOnExpiry,
    ApplyInterval,
    AlwaysOnBindProcess,
    AlwaysOnExtend,
    CaptureAlwaysOnHotkey,
    ToggleAlwaysOnWarn,
    ToggleAcOnly,
    BatteryFloorDown,
    BatteryFloorUp,
    ToggleDisplay,
    Exit,
    OpenSettings,
    TogglePin,
    ToggleSound,
    OpacityUp,
    OpacityDown,
    ToggleMeasure,
    ResizeOpen,
    CapturePinHotkey,
    CaptureOpacityUpHotkey,
    CaptureOpacityDownHotkey,
    ToggleFrameAccent,
    PickFrameColor,
    FrameOpacityDown,
    FrameOpacityUp,
    FrameThicknessDown,
    FrameThicknessUp,
    CaptureRulerHotkey,
    RulerModeNone,
    RulerModeBounds,
    RulerModeSpacing,
    RulerModeHorizontal,
    RulerModeVertical,
    ToggleRulerContinuous,
    ToggleRulerChannel,
    ToggleRulerFeet,
    RulerToleranceDown,
    RulerToleranceUp,
    PickRulerColor,
    RulerUnitPx,
    RulerUnitIn,
    RulerUnitCm,
    RulerUnitMm,
    RulerUnitDip,
    ToggleResizeShrink,
    ToggleResizeReplace,
    ToggleResizeOrientation,
    ToggleResizeMetadata,
    ToggleResizeKeepDate,
    ResizeJpegDown,
    ResizeJpegUp,
    ApplyResizeFilename,
    ResizeEncoderJpeg,
    ResizeEncoderPng,
    ResizeEncoderBmp,
    ResizeEncoderGif,
    ResizeEncoderTiff,
    ResizePngDefault,
    ResizePngOn,
    ResizePngOff,
    ResizeTiffDown,
    ResizeTiffUp,
    ToggleStartup,
    ToggleShowTray,
    ThemeSystem,
    ThemeLight,
    ThemeDark,
    LanguageSystem,
    LanguageTurkish,
    LanguageEnglish,
    LanguageArabic,
    OpenColorSettings,
    ApplyShellOpacity,
    PowerStoreRefresh,
    PowerStoreEnsureLoaded,
    PowerStoreAction,
    PowerStoreCancelPending,
    PowerStoreUpdateAll,
    PowerStoreCancelRunning,
    PowerStoreCheckNow,
    PowerStoreOpen,
    MediaRenamerAction,
    MediaRenamerOpen,
    ToggleMouse,
    PowerMouseAction,
    ToggleModuleAlwaysOn,
    ToggleModuleAlwaysTop,
    ToggleModuleScreenRuler,
    ToggleModuleImageResizer,
    ToggleModulePowerStore,
    ToggleModuleMediaRenamer,
}

impl AppCommand {
    pub(crate) fn id(self) -> usize {
        match self {
            Self::AlwaysOnOff => OFF,
            Self::AlwaysOnForever => FOREVER,
            Self::AlwaysOnHalfHour => HALF_HOUR,
            Self::AlwaysOnHour => HOUR,
            Self::AlwaysOnTwoHours => TWO_HOURS,
            Self::AlwaysOnInterval => CUSTOM,
            Self::AlwaysOnExpiry => EXPIRY,
            Self::ApplyInterval => APPLY_INTERVAL,
            Self::AlwaysOnBindProcess => BIND,
            Self::AlwaysOnExtend => EXTEND,
            Self::CaptureAlwaysOnHotkey => ALWAYS_ON_HOTKEY,
            Self::ToggleAlwaysOnWarn => ALWAYS_ON_WARN,
            Self::ToggleAcOnly => AC_ONLY,
            Self::BatteryFloorDown => BATTERY_FLOOR_DOWN,
            Self::BatteryFloorUp => BATTERY_FLOOR_UP,
            Self::ToggleDisplay => DISPLAY,
            Self::Exit => EXIT,
            Self::OpenSettings => OPEN,
            Self::TogglePin => PIN,
            Self::ToggleSound => SOUND,
            Self::OpacityUp => OPACITY_UP,
            Self::OpacityDown => OPACITY_DOWN,
            Self::ToggleMeasure => MEASURE,
            Self::ResizeOpen => RESIZE_OPEN,
            Self::CapturePinHotkey => HOTKEY_PIN_EDIT,
            Self::CaptureOpacityUpHotkey => HOTKEY_OP_UP_EDIT,
            Self::CaptureOpacityDownHotkey => HOTKEY_OP_DOWN_EDIT,
            Self::ToggleFrameAccent => FRAME_ACCENT,
            Self::PickFrameColor => FRAME_COLOR,
            Self::FrameOpacityDown => FRAME_OPACITY_DOWN,
            Self::FrameOpacityUp => FRAME_OPACITY_UP,
            Self::FrameThicknessDown => FRAME_THICK_DOWN,
            Self::FrameThicknessUp => FRAME_THICK_UP,
            Self::CaptureRulerHotkey => RULER_HOTKEY,
            Self::RulerModeNone => RULER_MODE_NONE,
            Self::RulerModeBounds => RULER_MODE_BOUNDS,
            Self::RulerModeSpacing => RULER_MODE_SPACING,
            Self::RulerModeHorizontal => RULER_MODE_HORIZONTAL,
            Self::RulerModeVertical => RULER_MODE_VERTICAL,
            Self::ToggleRulerContinuous => RULER_CONTINUOUS,
            Self::ToggleRulerChannel => RULER_CHANNEL,
            Self::ToggleRulerFeet => RULER_FEET,
            Self::RulerToleranceDown => RULER_TOLERANCE_DOWN,
            Self::RulerToleranceUp => RULER_TOLERANCE_UP,
            Self::PickRulerColor => RULER_COLOR,
            Self::RulerUnitPx => RULER_UNIT_PX,
            Self::RulerUnitIn => RULER_UNIT_IN,
            Self::RulerUnitCm => RULER_UNIT_CM,
            Self::RulerUnitMm => RULER_UNIT_MM,
            Self::RulerUnitDip => RULER_UNIT_DIP,
            Self::ToggleResizeShrink => RESIZE_SHRINK,
            Self::ToggleResizeReplace => RESIZE_REPLACE,
            Self::ToggleResizeOrientation => RESIZE_ORIENTATION,
            Self::ToggleResizeMetadata => RESIZE_METADATA,
            Self::ToggleResizeKeepDate => RESIZE_KEEP_DATE,
            Self::ResizeJpegDown => RESIZE_JPEG_DOWN,
            Self::ResizeJpegUp => RESIZE_JPEG_UP,
            Self::ApplyResizeFilename => RESIZE_FILENAME,
            Self::ResizeEncoderJpeg => RESIZE_ENC_JPEG,
            Self::ResizeEncoderPng => RESIZE_ENC_PNG,
            Self::ResizeEncoderBmp => RESIZE_ENC_BMP,
            Self::ResizeEncoderGif => RESIZE_ENC_GIF,
            Self::ResizeEncoderTiff => RESIZE_ENC_TIFF,
            Self::ResizePngDefault => RESIZE_PNG_DEFAULT,
            Self::ResizePngOn => RESIZE_PNG_ON,
            Self::ResizePngOff => RESIZE_PNG_OFF,
            Self::ResizeTiffDown => RESIZE_TIFF_DOWN,
            Self::ResizeTiffUp => RESIZE_TIFF_UP,
            Self::ToggleStartup => STARTUP,
            Self::ToggleShowTray => SHOW_TRAY,
            Self::ThemeSystem => THEME_SYSTEM,
            Self::ThemeLight => THEME_LIGHT,
            Self::ThemeDark => THEME_DARK,
            Self::LanguageSystem => LANG_SYSTEM,
            Self::LanguageTurkish => LANG_TR,
            Self::LanguageEnglish => LANG_EN,
            Self::LanguageArabic => LANG_AR,
            Self::OpenColorSettings => OPEN_COLOR_SETTINGS,
            Self::ApplyShellOpacity => APPLY_SHELL_OPACITY,
            Self::PowerStoreRefresh => POWER_STORE_REFRESH,
            Self::PowerStoreEnsureLoaded => POWER_STORE_ENSURE_LOADED,
            Self::PowerStoreAction => POWER_STORE_ACTION,
            Self::PowerStoreCancelPending => POWER_STORE_CANCEL_PENDING,
            Self::PowerStoreUpdateAll => POWER_STORE_UPDATE_ALL,
            Self::PowerStoreCancelRunning => POWER_STORE_CANCEL_RUNNING,
            Self::PowerStoreCheckNow => POWER_STORE_CHECK_NOW,
            Self::PowerStoreOpen => POWER_STORE_OPEN,
            Self::MediaRenamerAction => MEDIA_RENAMER_ACTION,
            Self::MediaRenamerOpen => MEDIA_RENAMER_OPEN,
            Self::ToggleMouse => TOGGLE_MOUSE,
            Self::PowerMouseAction => POWER_MOUSE_ACTION,
            Self::ToggleModuleAlwaysOn => TOGGLE_MODULE_ALWAYS_ON,
            Self::ToggleModuleAlwaysTop => TOGGLE_MODULE_ALWAYS_TOP,
            Self::ToggleModuleScreenRuler => TOGGLE_MODULE_SCREEN_RULER,
            Self::ToggleModuleImageResizer => TOGGLE_MODULE_IMAGE_RESIZER,
            Self::ToggleModulePowerStore => TOGGLE_MODULE_POWER_STORE,
            Self::ToggleModuleMediaRenamer => TOGGLE_MODULE_MEDIA_RENAMER,
        }
    }

    pub(crate) fn from_id(id: usize) -> Option<Self> {
        Some(match id {
            OFF => Self::AlwaysOnOff,
            FOREVER => Self::AlwaysOnForever,
            HALF_HOUR => Self::AlwaysOnHalfHour,
            HOUR => Self::AlwaysOnHour,
            TWO_HOURS => Self::AlwaysOnTwoHours,
            CUSTOM => Self::AlwaysOnInterval,
            EXPIRY => Self::AlwaysOnExpiry,
            APPLY_INTERVAL => Self::ApplyInterval,
            BIND => Self::AlwaysOnBindProcess,
            EXTEND => Self::AlwaysOnExtend,
            ALWAYS_ON_HOTKEY => Self::CaptureAlwaysOnHotkey,
            ALWAYS_ON_WARN => Self::ToggleAlwaysOnWarn,
            AC_ONLY => Self::ToggleAcOnly,
            BATTERY_FLOOR_DOWN => Self::BatteryFloorDown,
            BATTERY_FLOOR_UP => Self::BatteryFloorUp,
            DISPLAY => Self::ToggleDisplay,
            EXIT => Self::Exit,
            OPEN => Self::OpenSettings,
            PIN => Self::TogglePin,
            SOUND => Self::ToggleSound,
            OPACITY_UP => Self::OpacityUp,
            OPACITY_DOWN => Self::OpacityDown,
            MEASURE => Self::ToggleMeasure,
            RESIZE | RESIZE_OPEN => Self::ResizeOpen,
            HOTKEY_PIN_EDIT => Self::CapturePinHotkey,
            HOTKEY_OP_UP_EDIT => Self::CaptureOpacityUpHotkey,
            HOTKEY_OP_DOWN_EDIT => Self::CaptureOpacityDownHotkey,
            FRAME_ACCENT => Self::ToggleFrameAccent,
            FRAME_COLOR => Self::PickFrameColor,
            FRAME_OPACITY_DOWN => Self::FrameOpacityDown,
            FRAME_OPACITY_UP => Self::FrameOpacityUp,
            FRAME_THICK_DOWN => Self::FrameThicknessDown,
            FRAME_THICK_UP => Self::FrameThicknessUp,
            RULER_HOTKEY => Self::CaptureRulerHotkey,
            RULER_MODE_NONE => Self::RulerModeNone,
            RULER_MODE_BOUNDS => Self::RulerModeBounds,
            RULER_MODE_SPACING => Self::RulerModeSpacing,
            RULER_MODE_HORIZONTAL => Self::RulerModeHorizontal,
            RULER_MODE_VERTICAL => Self::RulerModeVertical,
            RULER_CONTINUOUS => Self::ToggleRulerContinuous,
            RULER_CHANNEL => Self::ToggleRulerChannel,
            RULER_FEET => Self::ToggleRulerFeet,
            RULER_TOLERANCE_DOWN => Self::RulerToleranceDown,
            RULER_TOLERANCE_UP => Self::RulerToleranceUp,
            RULER_COLOR => Self::PickRulerColor,
            RULER_UNIT_PX => Self::RulerUnitPx,
            RULER_UNIT_IN => Self::RulerUnitIn,
            RULER_UNIT_CM => Self::RulerUnitCm,
            RULER_UNIT_MM => Self::RulerUnitMm,
            RULER_UNIT_DIP => Self::RulerUnitDip,
            RESIZE_SHRINK => Self::ToggleResizeShrink,
            RESIZE_REPLACE => Self::ToggleResizeReplace,
            RESIZE_ORIENTATION => Self::ToggleResizeOrientation,
            RESIZE_METADATA => Self::ToggleResizeMetadata,
            RESIZE_KEEP_DATE => Self::ToggleResizeKeepDate,
            RESIZE_JPEG_DOWN => Self::ResizeJpegDown,
            RESIZE_JPEG_UP => Self::ResizeJpegUp,
            RESIZE_FILENAME => Self::ApplyResizeFilename,
            RESIZE_ENC_JPEG => Self::ResizeEncoderJpeg,
            RESIZE_ENC_PNG => Self::ResizeEncoderPng,
            RESIZE_ENC_BMP => Self::ResizeEncoderBmp,
            RESIZE_ENC_GIF => Self::ResizeEncoderGif,
            RESIZE_ENC_TIFF => Self::ResizeEncoderTiff,
            RESIZE_PNG_DEFAULT => Self::ResizePngDefault,
            RESIZE_PNG_ON => Self::ResizePngOn,
            RESIZE_PNG_OFF => Self::ResizePngOff,
            RESIZE_TIFF_DOWN => Self::ResizeTiffDown,
            RESIZE_TIFF_UP => Self::ResizeTiffUp,
            STARTUP => Self::ToggleStartup,
            SHOW_TRAY => Self::ToggleShowTray,
            THEME_SYSTEM => Self::ThemeSystem,
            THEME_LIGHT => Self::ThemeLight,
            THEME_DARK => Self::ThemeDark,
            LANG_SYSTEM => Self::LanguageSystem,
            LANG_TR => Self::LanguageTurkish,
            LANG_EN => Self::LanguageEnglish,
            LANG_AR => Self::LanguageArabic,
            OPEN_COLOR_SETTINGS => Self::OpenColorSettings,
            APPLY_SHELL_OPACITY => Self::ApplyShellOpacity,
            POWER_STORE_REFRESH => Self::PowerStoreRefresh,
            POWER_STORE_ENSURE_LOADED => Self::PowerStoreEnsureLoaded,
            POWER_STORE_ACTION => Self::PowerStoreAction,
            POWER_STORE_CANCEL_PENDING => Self::PowerStoreCancelPending,
            POWER_STORE_UPDATE_ALL => Self::PowerStoreUpdateAll,
            POWER_STORE_CANCEL_RUNNING => Self::PowerStoreCancelRunning,
            POWER_STORE_CHECK_NOW => Self::PowerStoreCheckNow,
            POWER_STORE_OPEN => Self::PowerStoreOpen,
            MEDIA_RENAMER_ACTION => Self::MediaRenamerAction,
            MEDIA_RENAMER_OPEN => Self::MediaRenamerOpen,
            TOGGLE_MOUSE => Self::ToggleMouse,
            POWER_MOUSE_ACTION => Self::PowerMouseAction,
            TOGGLE_MODULE_ALWAYS_ON => Self::ToggleModuleAlwaysOn,
            TOGGLE_MODULE_ALWAYS_TOP => Self::ToggleModuleAlwaysTop,
            TOGGLE_MODULE_SCREEN_RULER => Self::ToggleModuleScreenRuler,
            TOGGLE_MODULE_IMAGE_RESIZER => Self::ToggleModuleImageResizer,
            TOGGLE_MODULE_POWER_STORE => Self::ToggleModulePowerStore,
            TOGGLE_MODULE_MEDIA_RENAMER => Self::ToggleModuleMediaRenamer,
            _ => return None,
        })
    }

    /// Session a mode command starts. Expiry needs a picked time, so it is not here.
    pub(crate) fn always_on_session(self, now_ms: u64, interval_minutes: u32) -> Option<Session> {
        let [half, hour, two] = PRESET_MINUTES;
        Some(match self {
            Self::AlwaysOnOff => Session::Off,
            Self::AlwaysOnForever => Session::Indefinite,
            Self::AlwaysOnHalfHour => Session::interval(now_ms, half, false),
            Self::AlwaysOnHour => Session::interval(now_ms, hour, false),
            Self::AlwaysOnTwoHours => Session::interval(now_ms, two, false),
            Self::AlwaysOnInterval => Session::interval(now_ms, interval_minutes, true),
            _ => return None,
        })
    }

    /// The mode command that represents a running session in radio groups.
    pub(crate) fn for_session(session: Session) -> Self {
        let [half, hour, two] = PRESET_MINUTES;
        match session {
            Session::Off => Self::AlwaysOnOff,
            Session::Indefinite => Self::AlwaysOnForever,
            Session::Interval { custom: true, .. } => Self::AlwaysOnInterval,
            Session::Interval { minutes, .. } if minutes == half => Self::AlwaysOnHalfHour,
            Session::Interval { minutes, .. } if minutes == hour => Self::AlwaysOnHour,
            Session::Interval { minutes, .. } if minutes == two => Self::AlwaysOnTwoHours,
            Session::Interval { .. } => Self::AlwaysOnInterval,
            Session::Expiry { .. } => Self::AlwaysOnExpiry,
            Session::Process { .. } => Self::AlwaysOnBindProcess,
        }
    }

    pub(crate) fn ruler_mode(self) -> Option<crate::screen_ruler::Mode> {
        Some(match self {
            Self::RulerModeNone => crate::screen_ruler::Mode::None,
            Self::RulerModeBounds => crate::screen_ruler::Mode::Bounds,
            Self::RulerModeSpacing => crate::screen_ruler::Mode::Spacing,
            Self::RulerModeHorizontal => crate::screen_ruler::Mode::Horizontal,
            Self::RulerModeVertical => crate::screen_ruler::Mode::Vertical,
            _ => return None,
        })
    }

    pub(crate) fn ruler_unit(self) -> Option<crate::screen_ruler::Unit> {
        Some(match self {
            Self::RulerUnitPx => crate::screen_ruler::Unit::Pixel,
            Self::RulerUnitIn => crate::screen_ruler::Unit::Inch,
            Self::RulerUnitCm => crate::screen_ruler::Unit::Centimetre,
            Self::RulerUnitMm => crate::screen_ruler::Unit::Millimetre,
            Self::RulerUnitDip => crate::screen_ruler::Unit::Dip,
            _ => return None,
        })
    }

    pub(crate) fn theme(self) -> Option<crate::settings::AppTheme> {
        Some(match self {
            Self::ThemeSystem => crate::settings::AppTheme::System,
            Self::ThemeLight => crate::settings::AppTheme::Light,
            Self::ThemeDark => crate::settings::AppTheme::Dark,
            _ => return None,
        })
    }

    pub(crate) fn language(self) -> Option<crate::settings::AppLanguage> {
        Some(match self {
            Self::LanguageSystem => crate::settings::AppLanguage::System,
            Self::LanguageTurkish => crate::settings::AppLanguage::Turkish,
            Self::LanguageEnglish => crate::settings::AppLanguage::English,
            Self::LanguageArabic => crate::settings::AppLanguage::Arabic,
            _ => return None,
        })
    }

    pub(crate) fn resize_encoder(self) -> Option<FallbackEncoder> {
        Some(match self {
            Self::ResizeEncoderJpeg => FallbackEncoder::Jpeg,
            Self::ResizeEncoderPng => FallbackEncoder::Png,
            Self::ResizeEncoderBmp => FallbackEncoder::Bmp,
            Self::ResizeEncoderGif => FallbackEncoder::Gif,
            Self::ResizeEncoderTiff => FallbackEncoder::Tiff,
            _ => return None,
        })
    }

    pub(crate) fn resize_png(self) -> Option<PngInterlace> {
        Some(match self {
            Self::ResizePngDefault => PngInterlace::Default,
            Self::ResizePngOn => PngInterlace::On,
            Self::ResizePngOff => PngInterlace::Off,
            _ => return None,
        })
    }
}

pub(crate) const MODE_ORDER: [AppCommand; 6] = [
    AppCommand::AlwaysOnOff,
    AppCommand::AlwaysOnHalfHour,
    AppCommand::AlwaysOnHour,
    AppCommand::AlwaysOnTwoHours,
    AppCommand::AlwaysOnInterval,
    AppCommand::AlwaysOnForever,
];
pub(crate) const RULER_MODE_ORDER: [AppCommand; 5] = [
    AppCommand::RulerModeNone,
    AppCommand::RulerModeBounds,
    AppCommand::RulerModeSpacing,
    AppCommand::RulerModeHorizontal,
    AppCommand::RulerModeVertical,
];
pub(crate) const RULER_UNIT_ORDER: [AppCommand; 5] = [
    AppCommand::RulerUnitPx,
    AppCommand::RulerUnitIn,
    AppCommand::RulerUnitCm,
    AppCommand::RulerUnitMm,
    AppCommand::RulerUnitDip,
];
pub(crate) const RESIZE_ENCODER_ORDER: [AppCommand; 5] = [
    AppCommand::ResizeEncoderJpeg,
    AppCommand::ResizeEncoderPng,
    AppCommand::ResizeEncoderBmp,
    AppCommand::ResizeEncoderGif,
    AppCommand::ResizeEncoderTiff,
];
pub(crate) const RESIZE_PNG_ORDER: [AppCommand; 3] = [
    AppCommand::ResizePngDefault,
    AppCommand::ResizePngOn,
    AppCommand::ResizePngOff,
];
pub(crate) const THEME_ORDER: [AppCommand; 3] = [
    AppCommand::ThemeSystem,
    AppCommand::ThemeLight,
    AppCommand::ThemeDark,
];
pub(crate) const LANG_ORDER: [AppCommand; 4] = [
    AppCommand::LanguageSystem,
    AppCommand::LanguageTurkish,
    AppCommand::LanguageEnglish,
    AppCommand::LanguageArabic,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_ids_round_trip() {
        let samples = [
            AppCommand::AlwaysOnOff,
            AppCommand::AlwaysOnForever,
            AppCommand::ResizeOpen,
            AppCommand::TogglePin,
            AppCommand::ThemeDark,
            AppCommand::LanguageArabic,
            AppCommand::ApplyResizeFilename,
            AppCommand::CaptureRulerHotkey,
            AppCommand::ApplyShellOpacity,
            AppCommand::AlwaysOnInterval,
            AppCommand::AlwaysOnExpiry,
            AppCommand::ApplyInterval,
            AppCommand::AlwaysOnBindProcess,
            AppCommand::AlwaysOnExtend,
            AppCommand::CaptureAlwaysOnHotkey,
            AppCommand::ToggleAlwaysOnWarn,
            AppCommand::ToggleAcOnly,
            AppCommand::BatteryFloorDown,
            AppCommand::BatteryFloorUp,
            AppCommand::PowerStoreEnsureLoaded,
            AppCommand::PowerStoreAction,
            AppCommand::PowerStoreCheckNow,
            AppCommand::PowerStoreOpen,
            AppCommand::MediaRenamerAction,
            AppCommand::MediaRenamerOpen,
        ];
        for command in samples {
            assert_eq!(AppCommand::from_id(command.id()), Some(command));
        }
        assert_eq!(AppCommand::from_id(RESIZE), Some(AppCommand::ResizeOpen));
        assert_eq!(AppCommand::from_id(9), None);
    }

    #[test]
    fn mode_commands_start_sessions_without_wrapping() {
        let minute = crate::always_on::MINUTE_MS;
        let session = |command: AppCommand, now| command.always_on_session(now, 195);
        assert_eq!(session(AppCommand::AlwaysOnOff, 1_000), Some(Session::Off));
        assert_eq!(
            session(AppCommand::AlwaysOnForever, 1_000),
            Some(Session::Indefinite)
        );
        assert_eq!(
            session(AppCommand::AlwaysOnHour, 1_000),
            Some(Session::interval(1_000, 60, false))
        );
        assert_eq!(
            session(AppCommand::AlwaysOnInterval, 0).map(Session::mode),
            Some(crate::always_on::Mode::Until(195 * minute))
        );
        assert_eq!(
            session(AppCommand::AlwaysOnHalfHour, u64::MAX - 1).map(Session::mode),
            Some(crate::always_on::Mode::Until(u64::MAX))
        );
        for command in [
            AppCommand::AlwaysOnExpiry,
            AppCommand::ApplyInterval,
            AppCommand::ToggleDisplay,
            AppCommand::TogglePin,
            AppCommand::OpacityUp,
        ] {
            assert_eq!(command.always_on_session(0, 60), None);
        }
    }

    #[test]
    fn running_sessions_select_the_command_that_started_them() {
        for command in MODE_ORDER {
            let session = command.always_on_session(0, 45).unwrap();
            assert_eq!(AppCommand::for_session(session), command);
        }
        // A custom interval equal to a preset still shows as the custom choice.
        assert_eq!(
            AppCommand::for_session(Session::interval(0, 60, true)),
            AppCommand::AlwaysOnInterval
        );
        assert_eq!(
            AppCommand::for_session(Session::Expiry { end: 1, target: 1 }),
            AppCommand::AlwaysOnExpiry
        );
        assert_eq!(
            AppCommand::for_session(Session::Process { pid: 8 }),
            AppCommand::AlwaysOnBindProcess
        );
    }

    #[test]
    fn tray_resize_and_shell_resize_share_one_intent() {
        assert_eq!(AppCommand::ResizeOpen.id(), RESIZE_OPEN);
        assert_ne!(RESIZE, RESIZE_OPEN);
        assert_eq!(
            AppCommand::from_id(RESIZE),
            AppCommand::from_id(RESIZE_OPEN)
        );
    }
}
