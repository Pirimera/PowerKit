//! `WM_COMMAND` ids for the command vocabulary.
//!
//! The vocabulary itself is `powerkit_app::command::Command`, named `Command`
//! here. Win32 carries a command as a `WM_COMMAND` id (tray menus require them,
//! and worker threads post them); [`MenuId`] is the only adapter between the two.
pub const OFF: usize = 10;
pub const FOREVER: usize = 11;
pub const HALF_HOUR: usize = 12;
pub const HOUR: usize = 13;
pub const TWO_HOURS: usize = 14;
pub const DISPLAY: usize = 15;
pub const EXIT: usize = 16;
pub const OPEN: usize = 17;
pub const CUSTOM: usize = 19;
pub const PIN: usize = 20;
pub const SOUND: usize = 21;
pub const OPACITY_UP: usize = 22;
pub const OPACITY_DOWN: usize = 23;
pub const MEASURE: usize = 24;
pub const RESIZE: usize = 25;
pub const BIND: usize = 26;
pub const EXTEND: usize = 27;
pub const HOTKEY_PIN_EDIT: usize = 40;
pub const HOTKEY_OP_UP_EDIT: usize = 41;
pub const HOTKEY_OP_DOWN_EDIT: usize = 42;
pub const FRAME_ACCENT: usize = 43;
pub const FRAME_OPACITY_DOWN: usize = 46;
pub const FRAME_OPACITY_UP: usize = 47;
pub const FRAME_THICK_DOWN: usize = 49;
pub const FRAME_THICK_UP: usize = 50;
pub const RULER_HOTKEY: usize = 53;
pub const ALWAYS_ON_HOTKEY: usize = 54;
pub const RULER_MODE_NONE: usize = 60;
pub const RULER_MODE_BOUNDS: usize = 61;
pub const RULER_MODE_SPACING: usize = 62;
pub const RULER_MODE_HORIZONTAL: usize = 63;
pub const RULER_MODE_VERTICAL: usize = 64;
pub const RULER_CONTINUOUS: usize = 65;
pub const RULER_CHANNEL: usize = 66;
pub const RULER_FEET: usize = 67;
pub const RULER_TOLERANCE_DOWN: usize = 69;
pub const RULER_TOLERANCE_UP: usize = 70;
pub const RULER_UNIT_PX: usize = 72;
pub const RULER_UNIT_IN: usize = 73;
pub const RULER_UNIT_CM: usize = 74;
pub const RULER_UNIT_MM: usize = 75;
pub const RULER_UNIT_DIP: usize = 76;
pub const RESIZE_OPEN: usize = 120;
pub const RESIZE_SHRINK: usize = 121;
pub const RESIZE_REPLACE: usize = 122;
pub const RESIZE_ORIENTATION: usize = 123;
pub const RESIZE_METADATA: usize = 124;
pub const RESIZE_KEEP_DATE: usize = 125;
pub const RESIZE_JPEG_DOWN: usize = 127;
pub const RESIZE_JPEG_UP: usize = 128;
pub const RESIZE_ENC_JPEG: usize = 133;
pub const RESIZE_ENC_PNG: usize = 134;
pub const RESIZE_ENC_BMP: usize = 135;
pub const RESIZE_ENC_GIF: usize = 136;
pub const RESIZE_ENC_TIFF: usize = 137;
pub const RESIZE_PNG_DEFAULT: usize = 139;
pub const RESIZE_PNG_ON: usize = 140;
pub const RESIZE_PNG_OFF: usize = 141;
pub const RESIZE_TIFF_DOWN: usize = 143;
pub const RESIZE_TIFF_UP: usize = 144;
pub const STARTUP: usize = 150;
pub const SHOW_TRAY: usize = 151;
pub const THEME_SYSTEM: usize = 155;
pub const THEME_LIGHT: usize = 156;
pub const THEME_DARK: usize = 157;
pub const LANG_SYSTEM: usize = 158;
pub const LANG_TR: usize = 159;
pub const OPEN_COLOR_SETTINGS: usize = 160;
pub const LANG_EN: usize = 161;
pub const LANG_AR: usize = 162;
pub const EXPIRY: usize = 164;
pub const ALWAYS_ON_WARN: usize = 166;
pub const AC_ONLY: usize = 167;
pub const BATTERY_FLOOR_DOWN: usize = 168;
pub const BATTERY_FLOOR_UP: usize = 169;
pub const POWER_STORE_REFRESH: usize = 170;
pub const POWER_STORE_ENSURE_LOADED: usize = 171;
pub const POWER_STORE_CANCEL_PENDING: usize = 176;
pub const POWER_STORE_UPDATE_ALL: usize = 177;
pub const POWER_STORE_CANCEL_RUNNING: usize = 178;
pub const POWER_STORE_CHECK_NOW: usize = 181;
pub const POWER_STORE_OPEN: usize = 182;
pub const MEDIA_RENAMER_OPEN: usize = 191;
pub const TOGGLE_MOUSE: usize = 200;
pub const POWER_MOUSE_RING: usize = 203;
pub const TOGGLE_MODULE_ALWAYS_ON: usize = 210;
pub const TOGGLE_MODULE_ALWAYS_TOP: usize = 211;
pub const TOGGLE_MODULE_SCREEN_RULER: usize = 212;
pub const TOGGLE_MODULE_IMAGE_RESIZER: usize = 213;
pub const TOGGLE_MODULE_POWER_STORE: usize = 214;
pub const TOGGLE_MODULE_MEDIA_RENAMER: usize = 215;
pub const RESET_GENERAL_SETTINGS: usize = 221;
pub const OPEN_ALWAYS_ON_PAGE: usize = 222;
pub const OPEN_ALWAYS_TOP_PAGE: usize = 223;
pub const OPEN_SCREEN_RULER_PAGE: usize = 224;

pub use powerkit_app::command::Command;

/// A command's `WM_COMMAND` id and back.
pub trait MenuId: Sized {
    fn id(self) -> usize;
    fn from_id(id: usize) -> Option<Self>;
}
impl MenuId for Command {
    fn id(self) -> usize {
        match self {
            Self::AlwaysOnOff => OFF,
            Self::AlwaysOnForever => FOREVER,
            Self::AlwaysOnHalfHour => HALF_HOUR,
            Self::AlwaysOnHour => HOUR,
            Self::AlwaysOnTwoHours => TWO_HOURS,
            Self::AlwaysOnInterval => CUSTOM,
            Self::AlwaysOnExpiry => EXPIRY,
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
            Self::PowerStoreRefresh => POWER_STORE_REFRESH,
            Self::PowerStoreEnsureLoaded => POWER_STORE_ENSURE_LOADED,
            Self::PowerStoreCancelPending => POWER_STORE_CANCEL_PENDING,
            Self::PowerStoreUpdateAll => POWER_STORE_UPDATE_ALL,
            Self::PowerStoreCancelRunning => POWER_STORE_CANCEL_RUNNING,
            Self::PowerStoreCheckNow => POWER_STORE_CHECK_NOW,
            Self::PowerStoreOpen => POWER_STORE_OPEN,
            Self::MediaRenamerOpen => MEDIA_RENAMER_OPEN,
            Self::ToggleMouse => TOGGLE_MOUSE,
            Self::PowerMouseRing => POWER_MOUSE_RING,
            Self::ToggleModuleAlwaysOn => TOGGLE_MODULE_ALWAYS_ON,
            Self::ToggleModuleAlwaysTop => TOGGLE_MODULE_ALWAYS_TOP,
            Self::ToggleModuleScreenRuler => TOGGLE_MODULE_SCREEN_RULER,
            Self::ToggleModuleImageResizer => TOGGLE_MODULE_IMAGE_RESIZER,
            Self::ToggleModulePowerStore => TOGGLE_MODULE_POWER_STORE,
            Self::ToggleModuleMediaRenamer => TOGGLE_MODULE_MEDIA_RENAMER,
            Self::ResetGeneralSettings => RESET_GENERAL_SETTINGS,
            Self::OpenAlwaysOnPage => OPEN_ALWAYS_ON_PAGE,
            Self::OpenAlwaysTopPage => OPEN_ALWAYS_TOP_PAGE,
            Self::OpenScreenRulerPage => OPEN_SCREEN_RULER_PAGE,
        }
    }

    fn from_id(id: usize) -> Option<Self> {
        Some(match id {
            OFF => Self::AlwaysOnOff,
            FOREVER => Self::AlwaysOnForever,
            HALF_HOUR => Self::AlwaysOnHalfHour,
            HOUR => Self::AlwaysOnHour,
            TWO_HOURS => Self::AlwaysOnTwoHours,
            CUSTOM => Self::AlwaysOnInterval,
            EXPIRY => Self::AlwaysOnExpiry,
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
            POWER_STORE_REFRESH => Self::PowerStoreRefresh,
            POWER_STORE_ENSURE_LOADED => Self::PowerStoreEnsureLoaded,
            POWER_STORE_CANCEL_PENDING => Self::PowerStoreCancelPending,
            POWER_STORE_UPDATE_ALL => Self::PowerStoreUpdateAll,
            POWER_STORE_CANCEL_RUNNING => Self::PowerStoreCancelRunning,
            POWER_STORE_CHECK_NOW => Self::PowerStoreCheckNow,
            POWER_STORE_OPEN => Self::PowerStoreOpen,
            MEDIA_RENAMER_OPEN => Self::MediaRenamerOpen,
            TOGGLE_MOUSE => Self::ToggleMouse,
            POWER_MOUSE_RING => Self::PowerMouseRing,
            TOGGLE_MODULE_ALWAYS_ON => Self::ToggleModuleAlwaysOn,
            TOGGLE_MODULE_ALWAYS_TOP => Self::ToggleModuleAlwaysTop,
            TOGGLE_MODULE_SCREEN_RULER => Self::ToggleModuleScreenRuler,
            TOGGLE_MODULE_IMAGE_RESIZER => Self::ToggleModuleImageResizer,
            TOGGLE_MODULE_POWER_STORE => Self::ToggleModulePowerStore,
            TOGGLE_MODULE_MEDIA_RENAMER => Self::ToggleModuleMediaRenamer,
            RESET_GENERAL_SETTINGS => Self::ResetGeneralSettings,
            OPEN_ALWAYS_ON_PAGE => Self::OpenAlwaysOnPage,
            OPEN_ALWAYS_TOP_PAGE => Self::OpenAlwaysTopPage,
            OPEN_SCREEN_RULER_PAGE => Self::OpenScreenRulerPage,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use powerkit_app::command::MODE_ORDER;

    #[test]
    fn the_always_on_choices_keep_their_menu_ids() {
        assert_eq!(
            MODE_ORDER.map(Command::id),
            [OFF, HALF_HOUR, HOUR, TWO_HOURS, CUSTOM, FOREVER]
        );
    }

    #[test]
    fn command_ids_round_trip() {
        let samples = [
            Command::AlwaysOnOff,
            Command::AlwaysOnForever,
            Command::ResizeOpen,
            Command::TogglePin,
            Command::ThemeDark,
            Command::LanguageArabic,
            Command::CaptureRulerHotkey,
            Command::AlwaysOnInterval,
            Command::AlwaysOnExpiry,
            Command::AlwaysOnBindProcess,
            Command::AlwaysOnExtend,
            Command::CaptureAlwaysOnHotkey,
            Command::ToggleAlwaysOnWarn,
            Command::ToggleAcOnly,
            Command::BatteryFloorDown,
            Command::BatteryFloorUp,
            Command::PowerStoreEnsureLoaded,
            Command::PowerStoreCheckNow,
            Command::PowerStoreOpen,
            Command::MediaRenamerOpen,
            Command::ResetGeneralSettings,
            Command::OpenAlwaysOnPage,
            Command::OpenAlwaysTopPage,
            Command::OpenScreenRulerPage,
        ];
        for command in samples {
            assert_eq!(Command::from_id(command.id()), Some(command));
        }
        assert_eq!(Command::from_id(RESIZE), Some(Command::ResizeOpen));
        assert_eq!(Command::from_id(9), None);
    }

    #[test]
    fn tray_resize_and_shell_resize_share_one_intent() {
        assert_eq!(Command::ResizeOpen.id(), RESIZE_OPEN);
        assert_ne!(RESIZE, RESIZE_OPEN);
        assert_eq!(Command::from_id(RESIZE), Command::from_id(RESIZE_OPEN));
    }
}
