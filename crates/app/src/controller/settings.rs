//! Preference edits: the typed changes the UI and tray can make to the stored
//! preferences, and the one way they are committed.
//!
//! An [`Edit`] carries its own payload and is applied to a copy of the
//! preferences. [`commit`] then registers start-with-the-system when that
//! changed, writes the settings, and undoes the registration if the write
//! fails, so the file and the system never disagree. The caller adopts the
//! returned preferences and applies them to the running tools.
use crate::controller::hotkeys::Slot;
use crate::ports::{Autostart, SettingsStore};
use powerkit_core::always_on::{clamp_battery_floor, clamp_interval};
use powerkit_core::always_top::{clamp_frame_opacity, clamp_frame_thickness};
use powerkit_core::hotkey::Hotkey;
use powerkit_core::image_resizer::{FallbackEncoder, PngInterlace, ResizeSettings};
use powerkit_core::power_mouse::{self, hidpp::WheelMode};
use powerkit_core::primitives::Rgb;
use powerkit_core::screen_ruler::{Mode as RulerMode, Unit as RulerUnit};
use powerkit_core::settings::{AppLanguage, AppTheme, Preferences, clamp_shell_opacity};
use powerkit_core::tray_layout::TrayEdit;
use std::io;

/// A tool the user can switch off on the Home page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    AlwaysOn,
    AlwaysTop,
    ScreenRuler,
    ImageResizer,
    PowerStore,
    MediaRenamer,
    PowerMouse,
}
impl Tool {
    pub const ALL: [Self; 7] = [
        Self::AlwaysOn,
        Self::AlwaysTop,
        Self::ScreenRuler,
        Self::ImageResizer,
        Self::PowerStore,
        Self::MediaRenamer,
        Self::PowerMouse,
    ];
    /// Whether the tool's Home switch is on.
    pub fn enabled(self, prefs: &Preferences) -> bool {
        let modules = prefs.modules;
        match self {
            Self::AlwaysOn => modules.always_on,
            Self::AlwaysTop => modules.always_top,
            Self::ScreenRuler => modules.screen_ruler,
            Self::ImageResizer => modules.image_resizer,
            Self::PowerStore => modules.power_store,
            Self::MediaRenamer => modules.media_renamer,
            Self::PowerMouse => prefs.mouse.enabled,
        }
    }
    fn toggle(self, prefs: &mut Preferences) {
        let switch = match self {
            Self::AlwaysOn => &mut prefs.modules.always_on,
            Self::AlwaysTop => &mut prefs.modules.always_top,
            Self::ScreenRuler => &mut prefs.modules.screen_ruler,
            Self::ImageResizer => &mut prefs.modules.image_resizer,
            Self::PowerStore => &mut prefs.modules.power_store,
            Self::MediaRenamer => &mut prefs.modules.media_renamer,
            Self::PowerMouse => &mut prefs.mouse.enabled,
        };
        *switch = !*switch;
    }
}

/// One change to the stored preferences that depends on nothing but them.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    // Application
    ToggleStartup,
    ToggleShowTray,
    ToggleSound,
    Theme(AppTheme),
    Language(AppLanguage),
    /// Settings-window acrylic tint, in percent.
    ShellOpacity(i32),
    ToggleTool(Tool),
    /// Tray menu order and visibility edits, applied in order.
    Tray(Vec<TrayEdit>),
    /// Restore the Settings page values.
    ResetGeneral,
    /// Store a recorded chord. `hotkeys::record` builds this after checking
    /// that no other slot holds the chord.
    Hotkey(Slot, Hotkey),
    // AlwaysOn
    ToggleEndNotice,
    ToggleAcOnly,
    /// Move the battery floor by this many percent.
    StepBatteryFloor(i32),
    /// The custom interval in minutes, clamped to its valid range.
    Interval(u32),
    // AlwaysTop
    /// Switch between the system accent and a fixed frame color. `accent` is
    /// the current system accent, which becomes the fixed color.
    ToggleFrameAccent {
        accent: Rgb,
    },
    FrameColor(Rgb),
    StepFrameOpacity(i32),
    StepFrameThickness(i32),
    // ScreenRuler
    ToggleRulerContinuous,
    ToggleRulerPerChannel,
    ToggleRulerFeet,
    StepRulerTolerance(i32),
    RulerColor(Rgb),
    RulerMode(RulerMode),
    RulerUnit(RulerUnit),
    // ImageResizer
    ToggleResizeShrinkOnly,
    ToggleResizeReplace,
    ToggleResizeIgnoreOrientation,
    ToggleResizeRemoveMetadata,
    ToggleResizeKeepDate,
    StepResizeJpegQuality(i32),
    StepResizeTiff(i32),
    ResizeEncoder(FallbackEncoder),
    ResizePng(PngInterlace),
    /// An already validated filename pattern.
    ResizeFilename(String),
    /// Keep the choices of the last resize for the next one: the size, the
    /// custom size and the three switches of the resize dialog.
    RememberResize(ResizeSettings),
    // PowerMouse
    /// A binding the trigger cannot hold leaves the preferences unchanged.
    MouseBinding {
        trigger: power_mouse::Trigger,
        binding: power_mouse::Binding,
    },
    /// List a function key as a trigger that does nothing yet. A key that is
    /// already listed keeps its binding.
    MouseAddFunctionKey(power_mouse::Trigger),
    MouseRemoveFunctionKey(power_mouse::Trigger),
    /// Drop every binding and stop managing the device settings; the device
    /// keeps the values it has.
    MouseResetDefaults,
    /// `None` stops managing the setting.
    MouseDpi(Option<u16>),
    MouseWheelMode(Option<WheelMode>),
    MouseHiresWheel(bool),
}
impl Edit {
    /// Applies the edit to `prefs`. Steps clamp to their valid range.
    pub fn apply(self, prefs: &mut Preferences) {
        match self {
            Self::ToggleStartup => prefs.startup = !prefs.startup,
            Self::ToggleShowTray => prefs.show_tray = !prefs.show_tray,
            Self::ToggleSound => prefs.sound = !prefs.sound,
            Self::Theme(theme) => prefs.theme = theme,
            Self::Language(language) => prefs.language = language,
            Self::ShellOpacity(percent) => prefs.shell_opacity = clamp_shell_opacity(percent),
            Self::ToggleTool(tool) => tool.toggle(prefs),
            Self::Tray(edits) => {
                for edit in edits {
                    prefs.tray.apply(edit);
                }
            }
            Self::ResetGeneral => prefs.reset_general(),
            Self::Hotkey(slot, chord) => slot.set_chord(prefs, chord),
            Self::ToggleEndNotice => prefs.always_on_warn = !prefs.always_on_warn,
            Self::ToggleAcOnly => {
                prefs.always_on_battery.ac_only = !prefs.always_on_battery.ac_only;
            }
            Self::StepBatteryFloor(delta) => {
                let floor = i32::from(prefs.always_on_battery.floor).saturating_add(delta);
                prefs.always_on_battery.floor = clamp_battery_floor(floor);
            }
            Self::ToggleFrameAccent { accent } => {
                prefs.frame_color = match prefs.frame_color {
                    None => Some(accent),
                    Some(_) => None,
                };
            }
            Self::FrameColor(color) => prefs.frame_color = Some(color),
            Self::StepFrameOpacity(delta) => {
                prefs.frame_opacity =
                    clamp_frame_opacity(prefs.frame_opacity.saturating_add(delta));
            }
            Self::StepFrameThickness(delta) => {
                prefs.frame_thickness =
                    clamp_frame_thickness(prefs.frame_thickness.saturating_add(delta));
            }
            Self::ToggleRulerContinuous => prefs.ruler_continuous = !prefs.ruler_continuous,
            Self::ToggleRulerPerChannel => prefs.ruler_per_channel = !prefs.ruler_per_channel,
            Self::ToggleRulerFeet => prefs.ruler_feet = !prefs.ruler_feet,
            Self::StepRulerTolerance(delta) => {
                let tolerance = i32::from(prefs.ruler_tolerance).saturating_add(delta);
                prefs.ruler_tolerance = tolerance.clamp(0, i32::from(u8::MAX)) as u8;
            }
            Self::RulerColor(color) => prefs.ruler_color = color,
            Self::RulerMode(mode) => prefs.ruler_mode = mode,
            Self::RulerUnit(unit) => prefs.ruler_unit = unit,
            Self::ToggleResizeShrinkOnly => prefs.resize.shrink_only = !prefs.resize.shrink_only,
            Self::ToggleResizeReplace => prefs.resize.replace = !prefs.resize.replace,
            Self::ToggleResizeIgnoreOrientation => {
                prefs.resize.ignore_orientation = !prefs.resize.ignore_orientation;
            }
            Self::ToggleResizeRemoveMetadata => {
                prefs.resize.remove_metadata = !prefs.resize.remove_metadata;
            }
            Self::ToggleResizeKeepDate => prefs.resize.keep_date = !prefs.resize.keep_date,
            Self::StepResizeJpegQuality(delta) => {
                prefs.resize.jpeg_quality = prefs
                    .resize
                    .jpeg_quality
                    .saturating_add(delta)
                    .clamp(1, 100);
            }
            Self::StepResizeTiff(step) => prefs.resize.tiff = prefs.resize.tiff.next(step),
            Self::ResizeEncoder(encoder) => prefs.resize.fallback = encoder,
            Self::ResizePng(png) => prefs.resize.png = png,
            Self::ResizeFilename(filename) => prefs.resize.filename = filename,
            Self::Interval(minutes) => {
                prefs.interval_minutes = clamp_interval(i64::from(minutes));
            }
            Self::RememberResize(last) => {
                prefs.resize.selected = last.selected;
                prefs.resize.custom = last.custom;
                prefs.resize.shrink_only = last.shrink_only;
                prefs.resize.replace = last.replace;
                prefs.resize.ignore_orientation = last.ignore_orientation;
            }
            Self::MouseBinding { trigger, binding } => {
                prefs.mouse.set_binding(trigger, binding);
            }
            Self::MouseAddFunctionKey(trigger) => {
                if trigger.is_function_key() && !prefs.mouse.bindings.contains_key(&trigger) {
                    prefs.mouse.set_binding(
                        trigger,
                        power_mouse::Binding::Single(power_mouse::Action::None),
                    );
                }
            }
            Self::MouseRemoveFunctionKey(trigger) => {
                if trigger.is_function_key() {
                    prefs
                        .mouse
                        .set_binding(trigger, power_mouse::Binding::PassThrough);
                }
            }
            Self::MouseResetDefaults => {
                prefs.mouse.bindings.clear();
                prefs.mouse.dpi = None;
                prefs.mouse.wheel_mode = None;
                prefs.mouse.hires_wheel = None;
            }
            Self::MouseDpi(dpi) => prefs.mouse.dpi = dpi,
            Self::MouseWheelMode(mode) => prefs.mouse.wheel_mode = mode,
            Self::MouseHiresWheel(on) => {
                // An untouched switch reads as off; only a real change is stored.
                if prefs.mouse.hires_wheel.unwrap_or(false) != on {
                    prefs.mouse.hires_wheel = Some(on);
                }
            }
        }
    }

    /// Edits that write nothing when they leave the preferences as they were.
    /// Every other edit is an explicit user change and is always written.
    fn skips_when_unchanged(&self) -> bool {
        matches!(
            self,
            Self::Tray(_)
                | Self::ResetGeneral
                | Self::Interval(_)
                | Self::RememberResize(_)
                | Self::MouseBinding { .. }
                | Self::MouseAddFunctionKey(_)
                | Self::MouseRemoveFunctionKey(_)
                | Self::MouseResetDefaults
                | Self::MouseDpi(_)
                | Self::MouseWheelMode(_)
                | Self::MouseHiresWheel(_)
        )
    }
}

/// Committed preferences and what else follows from the change.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub prefs: Preferences,
    /// The UI language changed; shell integration labels need refreshing.
    pub language: bool,
    /// The tray icon was switched on or off.
    pub show_tray: bool,
}

/// Applies `edit` to a copy of `current` and stores the result.
///
/// Start-with-the-system is registered before the settings are written and put
/// back if the write fails. `Ok(None)` means nothing was written because the
/// edit changed nothing; `current` is never modified.
pub fn commit(
    host: &mut (impl SettingsStore + Autostart),
    current: &Preferences,
    edit: Edit,
) -> io::Result<Option<Change>> {
    let skip_unchanged = edit.skips_when_unchanged();
    let mut prefs = current.clone();
    edit.apply(&mut prefs);
    if skip_unchanged && prefs == *current {
        return Ok(None);
    }
    let startup_changed = prefs.startup != current.startup;
    if startup_changed {
        host.set_autostart(prefs.startup)?;
    }
    if let Err(error) = host.save(&prefs) {
        if startup_changed {
            let _ = host.set_autostart(current.startup);
        }
        return Err(error);
    }
    Ok(Some(Change {
        language: prefs.language != current.language,
        show_tray: prefs.show_tray != current.show_tray,
        prefs,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FakeHost;
    use powerkit_core::tray_layout::TrayTool;

    fn edited(edit: Edit) -> Preferences {
        let mut prefs = Preferences::default();
        edit.apply(&mut prefs);
        prefs
    }
    fn committed(host: &mut FakeHost, current: &Preferences, edit: Edit) -> Change {
        commit(host, current, edit).unwrap().unwrap()
    }

    #[test]
    fn mouse_edits_change_only_what_they_name_and_write_nothing_otherwise() {
        use powerkit_core::power_mouse::{Action, Binding, Trigger};
        let mut host = FakeHost::default();
        let defaults = Preferences::default();
        // A function key is listed once and keeps the binding it has.
        let listed = committed(
            &mut host,
            &defaults,
            Edit::MouseAddFunctionKey(Trigger::F13),
        )
        .prefs;
        assert_eq!(
            listed.mouse.bindings.get(&Trigger::F13),
            Some(&Binding::Single(Action::None))
        );
        assert_eq!(
            commit(&mut host, &listed, Edit::MouseAddFunctionKey(Trigger::F13)).unwrap(),
            None
        );
        let removed = committed(
            &mut host,
            &listed,
            Edit::MouseRemoveFunctionKey(Trigger::F13),
        )
        .prefs;
        assert!(!removed.mouse.bindings.contains_key(&Trigger::F13));
        // Only function keys can be added or removed this way.
        let ring = Trigger::RING[0];
        assert_eq!(
            commit(&mut host, &defaults, Edit::MouseAddFunctionKey(ring)).unwrap(),
            None
        );
        assert_eq!(
            commit(&mut host, &defaults, Edit::MouseRemoveFunctionKey(ring)).unwrap(),
            None
        );
        // Device settings: the same value again is not written.
        let dpi = committed(&mut host, &defaults, Edit::MouseDpi(Some(1600))).prefs;
        assert_eq!(dpi.mouse.dpi, Some(1600));
        assert_eq!(
            commit(&mut host, &dpi, Edit::MouseDpi(Some(1600))).unwrap(),
            None
        );
        // An untouched high-resolution switch already reads as off.
        assert_eq!(
            commit(&mut host, &defaults, Edit::MouseHiresWheel(false)).unwrap(),
            None
        );
        let hires = committed(&mut host, &dpi, Edit::MouseHiresWheel(true)).prefs;
        assert_eq!(hires.mouse.hires_wheel, Some(true));
        // Reset hands the device settings back and drops the bindings.
        let reset = committed(&mut host, &hires, Edit::MouseResetDefaults).prefs;
        assert_eq!(reset.mouse, defaults.mouse);
    }

    #[test]
    fn the_interval_is_clamped_and_the_last_resize_keeps_the_other_settings() {
        let mut host = FakeHost::default();
        let defaults = Preferences::default();
        let longer = committed(&mut host, &defaults, Edit::Interval(u32::MAX)).prefs;
        assert_eq!(
            longer.interval_minutes,
            powerkit_core::always_on::clamp_interval(i64::from(u32::MAX))
        );
        assert_eq!(
            commit(
                &mut host,
                &defaults,
                Edit::Interval(defaults.interval_minutes)
            )
            .unwrap(),
            None
        );
        let mut last = defaults.resize.clone();
        last.shrink_only = !last.shrink_only;
        last.filename = "ignored".into();
        last.jpeg_quality = 1;
        let kept = committed(&mut host, &defaults, Edit::RememberResize(last.clone())).prefs;
        assert_eq!(kept.resize.shrink_only, last.shrink_only);
        // The dialog does not own the encoder settings or the filename.
        assert_eq!(kept.resize.filename, defaults.resize.filename);
        assert_eq!(kept.resize.jpeg_quality, defaults.resize.jpeg_quality);
    }

    #[test]
    fn toggles_flip_exactly_their_own_switch() {
        let defaults = Preferences::default();
        for (edit, read) in [
            (
                Edit::ToggleSound,
                (|p: &Preferences| p.sound) as fn(&Preferences) -> bool,
            ),
            (Edit::ToggleStartup, |p| p.startup),
            (Edit::ToggleShowTray, |p| p.show_tray),
            (Edit::ToggleEndNotice, |p| p.always_on_warn),
            (Edit::ToggleAcOnly, |p| p.always_on_battery.ac_only),
            (Edit::ToggleRulerContinuous, |p| p.ruler_continuous),
            (Edit::ToggleRulerPerChannel, |p| p.ruler_per_channel),
            (Edit::ToggleRulerFeet, |p| p.ruler_feet),
            (Edit::ToggleResizeShrinkOnly, |p| p.resize.shrink_only),
            (Edit::ToggleResizeReplace, |p| p.resize.replace),
            (Edit::ToggleResizeIgnoreOrientation, |p| {
                p.resize.ignore_orientation
            }),
            (Edit::ToggleResizeRemoveMetadata, |p| {
                p.resize.remove_metadata
            }),
            (Edit::ToggleResizeKeepDate, |p| p.resize.keep_date),
        ] {
            let once = edited(edit.clone());
            assert_ne!(read(&once), read(&defaults), "{edit:?}");
            let mut twice = once.clone();
            edit.clone().apply(&mut twice);
            // Flipping back restores every preference, so nothing else moved.
            assert_eq!(twice, defaults, "{edit:?}");
        }
    }

    #[test]
    fn every_tool_switch_toggles_and_reports_its_state() {
        for tool in [
            Tool::AlwaysOn,
            Tool::AlwaysTop,
            Tool::ScreenRuler,
            Tool::ImageResizer,
            Tool::PowerStore,
            Tool::MediaRenamer,
            Tool::PowerMouse,
        ] {
            let defaults = Preferences::default();
            let toggled = edited(Edit::ToggleTool(tool));
            assert_ne!(tool.enabled(&toggled), tool.enabled(&defaults), "{tool:?}");
            let mut back = toggled.clone();
            Edit::ToggleTool(tool).apply(&mut back);
            assert_eq!(back, defaults, "{tool:?}");
        }
    }

    #[test]
    fn steps_clamp_at_both_ends() {
        assert_eq!(edited(Edit::StepRulerTolerance(-1_000)).ruler_tolerance, 0);
        assert_eq!(
            edited(Edit::StepRulerTolerance(i32::MAX)).ruler_tolerance,
            u8::MAX
        );
        assert_eq!(
            edited(Edit::StepResizeJpegQuality(-1_000))
                .resize
                .jpeg_quality,
            1
        );
        assert_eq!(
            edited(Edit::StepResizeJpegQuality(1_000))
                .resize
                .jpeg_quality,
            100
        );
        assert_eq!(
            edited(Edit::StepBatteryFloor(-1_000))
                .always_on_battery
                .floor,
            0
        );
        let floor = edited(Edit::StepBatteryFloor(i32::MAX))
            .always_on_battery
            .floor;
        assert_eq!(floor, powerkit_core::always_on::MAX_BATTERY_FLOOR);
        let opacity = edited(Edit::StepFrameOpacity(-1_000)).frame_opacity;
        assert_eq!(opacity, clamp_frame_opacity(i32::MIN));
        let thickness = edited(Edit::StepFrameThickness(1_000)).frame_thickness;
        assert_eq!(thickness, clamp_frame_thickness(i32::MAX));
        assert_eq!(
            edited(Edit::ShellOpacity(-5)).shell_opacity,
            clamp_shell_opacity(-5)
        );
    }

    #[test]
    fn the_frame_accent_switch_fixes_the_current_accent_and_releases_it() {
        let accent = Rgb::from_hex(0x0078D4);
        let fixed = edited(Edit::ToggleFrameAccent { accent });
        assert_eq!(fixed.frame_color, Some(accent));
        let mut released = fixed;
        Edit::ToggleFrameAccent {
            accent: Rgb::default(),
        }
        .apply(&mut released);
        assert_eq!(released.frame_color, None);
        let picked = Rgb::from_hex(0xFF4500);
        assert_eq!(edited(Edit::FrameColor(picked)).frame_color, Some(picked));
        assert_eq!(edited(Edit::RulerColor(picked)).ruler_color, picked);
    }

    #[test]
    fn a_commit_writes_the_edit_and_leaves_the_current_value_alone() {
        let mut host = FakeHost::default();
        let current = Preferences::default();
        let change = committed(&mut host, &current, Edit::ToggleSound);
        assert_eq!(change.prefs.sound, !current.sound);
        assert_eq!(host.saved.len(), 1);
        assert_eq!(host.saved[0], change.prefs);
        assert!(!change.language && !change.show_tray);
        // Start with the system was not touched.
        assert!(host.autostart.is_empty());
        assert_eq!(current, Preferences::default());
    }

    #[test]
    fn a_step_at_its_limit_is_still_written_but_an_empty_tray_edit_is_not() {
        let mut host = FakeHost::default();
        let current = edited(Edit::StepRulerTolerance(-1_000));
        assert!(
            commit(&mut host, &current, Edit::StepRulerTolerance(-1))
                .unwrap()
                .is_some()
        );
        assert_eq!(host.saved.len(), 1);
        assert_eq!(
            commit(&mut host, &current, Edit::Tray(Vec::new())).unwrap(),
            None
        );
        assert_eq!(
            commit(&mut host, &current, Edit::ResetGeneral).unwrap(),
            None
        );
        assert_eq!(host.saved.len(), 1);
    }

    #[test]
    fn startup_is_registered_before_the_file_and_undone_when_saving_fails() {
        let mut host = FakeHost::default();
        let current = Preferences::default();
        let change = committed(&mut host, &current, Edit::ToggleStartup);
        assert_eq!(host.autostart, [!current.startup]);
        assert_eq!(host.saved.len(), 1);
        // Saving fails: the registration is put back and nothing is adopted.
        host.fail_save = true;
        assert!(commit(&mut host, &change.prefs, Edit::ToggleStartup).is_err());
        assert_eq!(
            host.autostart,
            [!current.startup, current.startup, !current.startup]
        );
        assert_eq!(host.saved.len(), 1);
    }

    #[test]
    fn a_registration_that_fails_writes_nothing() {
        let mut host = FakeHost::default();
        host.fail_autostart = true;
        assert!(commit(&mut host, &Preferences::default(), Edit::ToggleStartup).is_err());
        assert!(host.saved.is_empty());
    }

    #[test]
    fn reset_restores_the_settings_page_and_reports_what_else_changed() {
        let mut host = FakeHost::default();
        let mut current = Preferences {
            startup: true,
            language: AppLanguage::English,
            theme: AppTheme::Dark,
            show_tray: false,
            sound: false,
            ..Preferences::default()
        };
        current.tray.apply(TrayEdit::MoveUp(TrayTool::AlwaysOn));
        let change = committed(&mut host, &current, Edit::ResetGeneral);
        let defaults = Preferences::default();
        assert_eq!(change.prefs.startup, defaults.startup);
        assert_eq!(change.prefs.theme, defaults.theme);
        assert_eq!(change.prefs.tray, defaults.tray);
        // Not a Settings page value.
        assert_eq!(change.prefs.sound, current.sound);
        assert!(change.language && change.show_tray);
        assert_eq!(host.autostart, [defaults.startup]);
        // A failed save undoes the registration here too.
        host.fail_save = true;
        assert!(commit(&mut host, &current, Edit::ResetGeneral).is_err());
        assert_eq!(
            host.autostart,
            [defaults.startup, defaults.startup, current.startup]
        );
    }

    #[test]
    fn language_and_tray_changes_are_reported_only_when_they_happen() {
        let mut host = FakeHost::default();
        let current = Preferences::default();
        let other = if current.language == AppLanguage::English {
            AppLanguage::Turkish
        } else {
            AppLanguage::English
        };
        assert!(committed(&mut host, &current, Edit::Language(other)).language);
        assert!(!committed(&mut host, &current, Edit::Language(current.language)).language);
        assert!(committed(&mut host, &current, Edit::ToggleShowTray).show_tray);
        let moved = committed(
            &mut host,
            &current,
            Edit::Tray(vec![TrayEdit::MoveUp(TrayTool::AlwaysOn)]),
        );
        assert!(!moved.show_tray);
    }
}
