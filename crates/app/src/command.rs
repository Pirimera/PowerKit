//! The command vocabulary shared by the settings UI, the tray and shortcuts:
//! what the user can ask PowerKit to do without supplying a value. A command
//! is a plain, copyable name; requests that carry a value are [`Intent`]s
//! (see `intent.rs`). How a platform's menu identifies a command is that
//! platform's business.
//!
//! [`Intent`]: crate::intent::Intent
use crate::controller::hotkeys::Slot;
use crate::controller::settings::{Edit, Tool};
use powerkit_core::always_on::{Choice, PRESET_MINUTES, Session};
use powerkit_core::image_resizer::{FallbackEncoder, PngInterlace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    AlwaysOnOff,
    AlwaysOnForever,
    AlwaysOnHalfHour,
    AlwaysOnHour,
    AlwaysOnTwoHours,
    AlwaysOnInterval,
    AlwaysOnExpiry,
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
    PowerStoreRefresh,
    PowerStoreEnsureLoaded,
    PowerStoreCancelPending,
    PowerStoreUpdateAll,
    PowerStoreCancelRunning,
    PowerStoreCheckNow,
    PowerStoreOpen,
    MediaRenamerOpen,
    ToggleMouse,
    /// Show or hide the Actions Ring; a mouse button bound to it sends this.
    PowerMouseRing,
    ToggleModuleAlwaysOn,
    ToggleModuleAlwaysTop,
    ToggleModuleScreenRuler,
    ToggleModuleImageResizer,
    ToggleModulePowerStore,
    ToggleModuleMediaRenamer,
    ResetGeneralSettings,
    /// Tray Shortcuts entries open the page where that shortcut is set.
    OpenAlwaysOnPage,
    OpenAlwaysTopPage,
    OpenScreenRulerPage,
}

impl Command {
    /// `None` for app-wide commands (and the tool switches themselves). The match
    /// has no wildcard, so a new command must pick its tool before it compiles.
    pub fn tool(self) -> Option<Tool> {
        Some(match self {
            Self::Exit
            | Self::OpenSettings
            | Self::ToggleStartup
            | Self::ToggleShowTray
            | Self::ThemeSystem
            | Self::ThemeLight
            | Self::ThemeDark
            | Self::LanguageSystem
            | Self::LanguageTurkish
            | Self::LanguageEnglish
            | Self::LanguageArabic
            | Self::OpenColorSettings
            | Self::ToggleMouse
            | Self::ToggleModuleAlwaysOn
            | Self::ToggleModuleAlwaysTop
            | Self::ToggleModuleScreenRuler
            | Self::ToggleModuleImageResizer
            | Self::ToggleModulePowerStore
            | Self::ToggleModuleMediaRenamer
            | Self::ResetGeneralSettings => return None,
            Self::AlwaysOnOff
            | Self::AlwaysOnForever
            | Self::AlwaysOnHalfHour
            | Self::AlwaysOnHour
            | Self::AlwaysOnTwoHours
            | Self::AlwaysOnInterval
            | Self::AlwaysOnExpiry
            | Self::AlwaysOnBindProcess
            | Self::AlwaysOnExtend
            | Self::CaptureAlwaysOnHotkey
            | Self::ToggleAlwaysOnWarn
            | Self::ToggleAcOnly
            | Self::BatteryFloorDown
            | Self::BatteryFloorUp
            | Self::ToggleDisplay
            | Self::OpenAlwaysOnPage => Tool::AlwaysOn,
            Self::TogglePin
            | Self::ToggleSound
            | Self::OpacityUp
            | Self::OpacityDown
            | Self::CapturePinHotkey
            | Self::CaptureOpacityUpHotkey
            | Self::CaptureOpacityDownHotkey
            | Self::ToggleFrameAccent
            | Self::FrameOpacityDown
            | Self::FrameOpacityUp
            | Self::FrameThicknessDown
            | Self::FrameThicknessUp
            | Self::OpenAlwaysTopPage => Tool::AlwaysTop,
            Self::OpenScreenRulerPage
            | Self::ToggleMeasure
            | Self::CaptureRulerHotkey
            | Self::RulerModeNone
            | Self::RulerModeBounds
            | Self::RulerModeSpacing
            | Self::RulerModeHorizontal
            | Self::RulerModeVertical
            | Self::ToggleRulerContinuous
            | Self::ToggleRulerChannel
            | Self::ToggleRulerFeet
            | Self::RulerToleranceDown
            | Self::RulerToleranceUp
            | Self::RulerUnitPx
            | Self::RulerUnitIn
            | Self::RulerUnitCm
            | Self::RulerUnitMm
            | Self::RulerUnitDip => Tool::ScreenRuler,
            Self::ResizeOpen
            | Self::ToggleResizeShrink
            | Self::ToggleResizeReplace
            | Self::ToggleResizeOrientation
            | Self::ToggleResizeMetadata
            | Self::ToggleResizeKeepDate
            | Self::ResizeJpegDown
            | Self::ResizeJpegUp
            | Self::ResizeEncoderJpeg
            | Self::ResizeEncoderPng
            | Self::ResizeEncoderBmp
            | Self::ResizeEncoderGif
            | Self::ResizeEncoderTiff
            | Self::ResizePngDefault
            | Self::ResizePngOn
            | Self::ResizePngOff
            | Self::ResizeTiffDown
            | Self::ResizeTiffUp => Tool::ImageResizer,
            Self::PowerStoreRefresh
            | Self::PowerStoreEnsureLoaded
            | Self::PowerStoreCancelPending
            | Self::PowerStoreUpdateAll
            | Self::PowerStoreCancelRunning
            | Self::PowerStoreCheckNow
            | Self::PowerStoreOpen => Tool::PowerStore,
            Self::MediaRenamerOpen => Tool::MediaRenamer,
            Self::PowerMouseRing => Tool::PowerMouse,
        })
    }

    /// The shortcut slot a "record this shortcut" command edits.
    pub fn hotkey_slot(self) -> Option<Slot> {
        Slot::ALL
            .into_iter()
            .find(|slot| Self::capture(*slot) == self)
    }

    /// The command that records `slot`; the settings shell keys its shortcut
    /// cards by it.
    pub fn capture(slot: Slot) -> Self {
        match slot {
            Slot::Pin => Self::CapturePinHotkey,
            Slot::OpacityUp => Self::CaptureOpacityUpHotkey,
            Slot::OpacityDown => Self::CaptureOpacityDownHotkey,
            Slot::Ruler => Self::CaptureRulerHotkey,
            Slot::AlwaysOn => Self::CaptureAlwaysOnHotkey,
        }
    }

    /// The preference edit a command stands for, when it needs nothing beyond
    /// the command itself. Commands whose value comes from the settings shell
    /// (a picked color, a typed filename) build their edit where that value is
    /// taken.
    pub fn edit(self) -> Option<Edit> {
        use powerkit_core::always_on::BATTERY_FLOOR_STEP;
        Some(match self {
            Self::ToggleStartup => Edit::ToggleStartup,
            Self::ToggleShowTray => Edit::ToggleShowTray,
            Self::ToggleSound => Edit::ToggleSound,
            Self::ResetGeneralSettings => Edit::ResetGeneral,
            Self::ToggleModuleAlwaysOn => Edit::ToggleTool(Tool::AlwaysOn),
            Self::ToggleModuleAlwaysTop => Edit::ToggleTool(Tool::AlwaysTop),
            Self::ToggleModuleScreenRuler => Edit::ToggleTool(Tool::ScreenRuler),
            Self::ToggleModuleImageResizer => Edit::ToggleTool(Tool::ImageResizer),
            Self::ToggleModulePowerStore => Edit::ToggleTool(Tool::PowerStore),
            Self::ToggleModuleMediaRenamer => Edit::ToggleTool(Tool::MediaRenamer),
            Self::ToggleMouse => Edit::ToggleTool(Tool::PowerMouse),
            Self::ToggleAlwaysOnWarn => Edit::ToggleEndNotice,
            Self::ToggleAcOnly => Edit::ToggleAcOnly,
            Self::BatteryFloorDown => Edit::StepBatteryFloor(-i32::from(BATTERY_FLOOR_STEP)),
            Self::BatteryFloorUp => Edit::StepBatteryFloor(i32::from(BATTERY_FLOOR_STEP)),
            Self::FrameOpacityDown => Edit::StepFrameOpacity(-10),
            Self::FrameOpacityUp => Edit::StepFrameOpacity(10),
            Self::FrameThicknessDown => Edit::StepFrameThickness(-1),
            Self::FrameThicknessUp => Edit::StepFrameThickness(1),
            Self::ToggleRulerContinuous => Edit::ToggleRulerContinuous,
            Self::ToggleRulerChannel => Edit::ToggleRulerPerChannel,
            Self::ToggleRulerFeet => Edit::ToggleRulerFeet,
            Self::RulerToleranceDown => Edit::StepRulerTolerance(-1),
            Self::RulerToleranceUp => Edit::StepRulerTolerance(1),
            Self::ToggleResizeShrink => Edit::ToggleResizeShrinkOnly,
            Self::ToggleResizeReplace => Edit::ToggleResizeReplace,
            Self::ToggleResizeOrientation => Edit::ToggleResizeIgnoreOrientation,
            Self::ToggleResizeMetadata => Edit::ToggleResizeRemoveMetadata,
            Self::ToggleResizeKeepDate => Edit::ToggleResizeKeepDate,
            Self::ResizeJpegDown => Edit::StepResizeJpegQuality(-1),
            Self::ResizeJpegUp => Edit::StepResizeJpegQuality(1),
            Self::ResizeTiffDown => Edit::StepResizeTiff(-1),
            Self::ResizeTiffUp => Edit::StepResizeTiff(1),
            command => {
                if let Some(theme) = command.theme() {
                    Edit::Theme(theme)
                } else if let Some(language) = command.language() {
                    Edit::Language(language)
                } else if let Some(mode) = command.ruler_mode() {
                    Edit::RulerMode(mode)
                } else if let Some(unit) = command.ruler_unit() {
                    Edit::RulerUnit(unit)
                } else if let Some(encoder) = command.resize_encoder() {
                    Edit::ResizeEncoder(encoder)
                } else {
                    Edit::ResizePng(command.resize_png()?)
                }
            }
        })
    }

    /// The AlwaysOn choice a mode command names; `None` for other commands.
    pub fn always_on_choice(self) -> Option<Choice> {
        let [half, hour, two] = PRESET_MINUTES;
        Some(match self {
            Self::AlwaysOnOff => Choice::Off,
            Self::AlwaysOnForever => Choice::Indefinite,
            Self::AlwaysOnHalfHour => Choice::Preset(half),
            Self::AlwaysOnHour => Choice::Preset(hour),
            Self::AlwaysOnTwoHours => Choice::Preset(two),
            Self::AlwaysOnInterval => Choice::CustomInterval,
            _ => return None,
        })
    }

    /// The mode command that represents a running session in radio groups.
    pub fn for_session(session: Session) -> Self {
        Self::for_choice(Choice::of(session))
    }

    /// The mode command for an AlwaysOn choice.
    pub fn for_choice(choice: Choice) -> Self {
        let [half, hour, two] = PRESET_MINUTES;
        match choice {
            Choice::Off => Self::AlwaysOnOff,
            Choice::Indefinite => Self::AlwaysOnForever,
            Choice::Preset(minutes) if minutes == half => Self::AlwaysOnHalfHour,
            Choice::Preset(minutes) if minutes == hour => Self::AlwaysOnHour,
            Choice::Preset(minutes) if minutes == two => Self::AlwaysOnTwoHours,
            Choice::Preset(_) | Choice::CustomInterval => Self::AlwaysOnInterval,
            Choice::Expiry => Self::AlwaysOnExpiry,
            Choice::Process => Self::AlwaysOnBindProcess,
        }
    }

    pub fn ruler_mode(self) -> Option<powerkit_core::screen_ruler::Mode> {
        Some(match self {
            Self::RulerModeNone => powerkit_core::screen_ruler::Mode::None,
            Self::RulerModeBounds => powerkit_core::screen_ruler::Mode::Bounds,
            Self::RulerModeSpacing => powerkit_core::screen_ruler::Mode::Spacing,
            Self::RulerModeHorizontal => powerkit_core::screen_ruler::Mode::Horizontal,
            Self::RulerModeVertical => powerkit_core::screen_ruler::Mode::Vertical,
            _ => return None,
        })
    }

    pub fn ruler_unit(self) -> Option<powerkit_core::screen_ruler::Unit> {
        Some(match self {
            Self::RulerUnitPx => powerkit_core::screen_ruler::Unit::Pixel,
            Self::RulerUnitIn => powerkit_core::screen_ruler::Unit::Inch,
            Self::RulerUnitCm => powerkit_core::screen_ruler::Unit::Centimetre,
            Self::RulerUnitMm => powerkit_core::screen_ruler::Unit::Millimetre,
            Self::RulerUnitDip => powerkit_core::screen_ruler::Unit::Dip,
            _ => return None,
        })
    }

    pub fn theme(self) -> Option<powerkit_core::settings::AppTheme> {
        Some(match self {
            Self::ThemeSystem => powerkit_core::settings::AppTheme::System,
            Self::ThemeLight => powerkit_core::settings::AppTheme::Light,
            Self::ThemeDark => powerkit_core::settings::AppTheme::Dark,
            _ => return None,
        })
    }

    pub fn language(self) -> Option<powerkit_core::settings::AppLanguage> {
        Some(match self {
            Self::LanguageSystem => powerkit_core::settings::AppLanguage::System,
            Self::LanguageTurkish => powerkit_core::settings::AppLanguage::Turkish,
            Self::LanguageEnglish => powerkit_core::settings::AppLanguage::English,
            Self::LanguageArabic => powerkit_core::settings::AppLanguage::Arabic,
            _ => return None,
        })
    }

    pub fn resize_encoder(self) -> Option<FallbackEncoder> {
        Some(match self {
            Self::ResizeEncoderJpeg => FallbackEncoder::Jpeg,
            Self::ResizeEncoderPng => FallbackEncoder::Png,
            Self::ResizeEncoderBmp => FallbackEncoder::Bmp,
            Self::ResizeEncoderGif => FallbackEncoder::Gif,
            Self::ResizeEncoderTiff => FallbackEncoder::Tiff,
            _ => return None,
        })
    }

    pub fn resize_png(self) -> Option<PngInterlace> {
        Some(match self {
            Self::ResizePngDefault => PngInterlace::Default,
            Self::ResizePngOn => PngInterlace::On,
            Self::ResizePngOff => PngInterlace::Off,
            _ => return None,
        })
    }
}

pub const MODE_ORDER: [Command; 6] = [
    Command::AlwaysOnOff,
    Command::AlwaysOnHalfHour,
    Command::AlwaysOnHour,
    Command::AlwaysOnTwoHours,
    Command::AlwaysOnInterval,
    Command::AlwaysOnForever,
];
pub const RULER_MODE_ORDER: [Command; 5] = [
    Command::RulerModeNone,
    Command::RulerModeBounds,
    Command::RulerModeSpacing,
    Command::RulerModeHorizontal,
    Command::RulerModeVertical,
];
pub const RULER_UNIT_ORDER: [Command; 5] = [
    Command::RulerUnitPx,
    Command::RulerUnitIn,
    Command::RulerUnitCm,
    Command::RulerUnitMm,
    Command::RulerUnitDip,
];
pub const RESIZE_ENCODER_ORDER: [Command; 5] = [
    Command::ResizeEncoderJpeg,
    Command::ResizeEncoderPng,
    Command::ResizeEncoderBmp,
    Command::ResizeEncoderGif,
    Command::ResizeEncoderTiff,
];
pub const RESIZE_PNG_ORDER: [Command; 3] = [
    Command::ResizePngDefault,
    Command::ResizePngOn,
    Command::ResizePngOff,
];
pub const THEME_ORDER: [Command; 3] = [
    Command::ThemeSystem,
    Command::ThemeLight,
    Command::ThemeDark,
];
pub const LANG_ORDER: [Command; 4] = [
    Command::LanguageSystem,
    Command::LanguageTurkish,
    Command::LanguageEnglish,
    Command::LanguageArabic,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_commands_start_sessions_without_wrapping() {
        let minute = powerkit_core::always_on::MINUTE_MS;
        let session = |command: Command, now| command.always_on_choice()?.start(now, 195);
        assert_eq!(session(Command::AlwaysOnOff, 1_000), Some(Session::Off));
        assert_eq!(
            session(Command::AlwaysOnForever, 1_000),
            Some(Session::Indefinite)
        );
        assert_eq!(
            session(Command::AlwaysOnHour, 1_000),
            Some(Session::interval(1_000, 60, false))
        );
        assert_eq!(
            session(Command::AlwaysOnInterval, 0).map(Session::mode),
            Some(powerkit_core::always_on::Mode::Until(195 * minute))
        );
        assert_eq!(
            session(Command::AlwaysOnHalfHour, u64::MAX - 1).map(Session::mode),
            Some(powerkit_core::always_on::Mode::Until(u64::MAX))
        );
        for command in [
            Command::AlwaysOnExpiry,
            Command::ToggleDisplay,
            Command::TogglePin,
            Command::OpacityUp,
        ] {
            assert_eq!(command.always_on_choice(), None);
        }
    }

    #[test]
    fn running_sessions_select_the_command_that_started_them() {
        for command in MODE_ORDER {
            let session = command.always_on_choice().unwrap().start(0, 45).unwrap();
            assert_eq!(Command::for_session(session), command);
        }
        // A custom interval equal to a preset still shows as the custom choice.
        assert_eq!(
            Command::for_session(Session::interval(0, 60, true)),
            Command::AlwaysOnInterval
        );
        assert_eq!(
            Command::for_session(Session::Expiry {
                end: 1,
                target: powerkit_core::primitives::UtcTicks(1)
            }),
            Command::AlwaysOnExpiry
        );
        assert_eq!(
            Command::for_session(Session::Process { pid: 8 }),
            Command::AlwaysOnBindProcess
        );
    }

    #[test]
    fn tool_switches_are_never_gated_by_their_own_tool() {
        for command in [
            Command::ToggleModuleAlwaysOn,
            Command::ToggleModuleAlwaysTop,
            Command::ToggleModuleScreenRuler,
            Command::ToggleModuleImageResizer,
            Command::ToggleModulePowerStore,
            Command::ToggleModuleMediaRenamer,
            Command::ToggleMouse,
            Command::Exit,
            Command::OpenSettings,
        ] {
            assert_eq!(command.tool(), None, "{command:?}");
        }
        for command in MODE_ORDER {
            assert_eq!(command.tool(), Some(Tool::AlwaysOn));
        }
        assert_eq!(Command::ResizeOpen.tool(), Some(Tool::ImageResizer));
        assert_eq!(Command::PowerMouseRing.tool(), Some(Tool::PowerMouse));
    }

    #[test]
    fn capture_commands_and_shortcut_slots_map_both_ways() {
        for slot in Slot::ALL {
            let command = Command::capture(slot);
            assert_eq!(command.hotkey_slot(), Some(slot));
            // Recording belongs to the tool that owns the shortcut.
            assert_eq!(command.tool(), Some(slot.tool()));
        }
        assert_eq!(Command::TogglePin.hotkey_slot(), None);
    }
    #[test]
    fn preference_commands_map_to_edits_and_others_do_not() {
        assert_eq!(Command::ToggleSound.edit(), Some(Edit::ToggleSound));
        assert_eq!(
            Command::ToggleMouse.edit(),
            Some(Edit::ToggleTool(Tool::PowerMouse))
        );
        assert_eq!(
            Command::ThemeDark.edit(),
            Some(Edit::Theme(powerkit_core::settings::AppTheme::Dark))
        );
        assert_eq!(
            Command::RulerUnitMm.edit(),
            Some(Edit::RulerUnit(
                powerkit_core::screen_ruler::Unit::Millimetre
            ))
        );
        assert_eq!(
            Command::BatteryFloorDown.edit(),
            Some(Edit::StepBatteryFloor(-10))
        );
        // Every member of an exclusive group is an edit.
        for command in THEME_ORDER
            .into_iter()
            .chain(LANG_ORDER)
            .chain(RULER_MODE_ORDER)
            .chain(RULER_UNIT_ORDER)
            .chain(RESIZE_ENCODER_ORDER)
            .chain(RESIZE_PNG_ORDER)
        {
            assert!(command.edit().is_some(), "{command:?}");
        }
        // Commands that act, need a value from the shell or start a session.
        for command in [
            Command::Exit,
            Command::OpenSettings,
            Command::TogglePin,
            Command::ToggleDisplay,
            Command::AlwaysOnForever,
        ] {
            assert_eq!(command.edit(), None, "{command:?}");
        }
    }
}
