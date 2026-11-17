//! Small versioned settings file; no runtime framework or network dependency.
use crate::always_top::{
    DEFAULT_FRAME_OPACITY, DEFAULT_FRAME_THICKNESS, Hotkey, clamp_frame_opacity,
    clamp_frame_thickness,
};
use std::{
    io::{self, Read},
    path::Path,
};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AppTheme {
    #[default]
    System,
    Light,
    Dark,
}
impl AppTheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
    fn parse(value: &str) -> io::Result<Self> {
        match value {
            "system" => Ok(Self::System),
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            _ => Err(invalid()),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AppLanguage {
    #[default]
    System,
    Turkish,
    English,
    Arabic,
}
impl AppLanguage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Turkish => "tr",
            Self::English => "en",
            Self::Arabic => "ar",
        }
    }
    fn parse(value: &str) -> io::Result<Self> {
        match value {
            "system" => Ok(Self::System),
            "tr" => Ok(Self::Turkish),
            "en" => Ok(Self::English),
            "ar" => Ok(Self::Arabic),
            _ => Err(invalid()),
        }
    }
}
/// Which tools appear in the rail/tray and may run system-wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Modules {
    pub always_on: bool,
    pub always_top: bool,
    pub screen_ruler: bool,
    pub image_resizer: bool,
    pub power_store: bool,
    pub media_renamer: bool,
}
impl Default for Modules {
    fn default() -> Self {
        Self {
            always_on: true,
            always_top: true,
            screen_ruler: true,
            image_resizer: true,
            power_store: true,
            media_renamer: true,
        }
    }
}
impl Modules {
    pub fn append_settings(self, out: &mut String) {
        out.push_str(&format!(
            "module_always_on={}\nmodule_always_top={}\nmodule_screen_ruler={}\nmodule_image_resizer={}\nmodule_power_store={}\nmodule_media_renamer={}\n",
            self.always_on,
            self.always_top,
            self.screen_ruler,
            self.image_resizer,
            self.power_store,
            self.media_renamer
        ));
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Preferences {
    pub display: bool,
    /// Custom AlwaysOn interval; a duration preference, never a running session.
    pub interval_minutes: u32,
    pub always_on_hotkey: Hotkey,
    /// Notify shortly before a timed AlwaysOn session ends.
    pub always_on_warn: bool,
    pub always_on_battery: crate::always_on::BatteryPolicy,
    pub sound: bool,
    pub startup: bool,
    pub theme: AppTheme,
    pub language: AppLanguage,
    pub show_tray: bool,
    pub shell_opacity: i32,
    pub pin: Hotkey,
    pub opacity_up: Hotkey,
    pub opacity_down: Hotkey,
    pub frame_color: Option<u32>,
    pub frame_opacity: i32,
    pub frame_thickness: i32,
    pub ruler_hotkey: Hotkey,
    pub ruler_mode: crate::screen_ruler::Mode,
    pub ruler_tolerance: u8,
    pub ruler_per_channel: bool,
    pub ruler_feet: bool,
    pub ruler_continuous: bool,
    pub ruler_unit: crate::screen_ruler::Unit,
    pub ruler_color: u32,
    pub resize: crate::image_resizer::ResizeSettings,
    pub power_store: crate::power_store::PowerStorePreferences,
    pub rename: crate::media_renamer::Preferences,
    pub mouse: crate::power_mouse::Preferences,
    /// Home-page tool switches; PowerMouse uses [`Preferences::mouse`].enabled.
    pub modules: Modules,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            display: false,
            interval_minutes: crate::always_on::DEFAULT_INTERVAL_MINUTES,
            always_on_hotkey: Hotkey::always_on_default(),
            always_on_warn: true,
            always_on_battery: crate::always_on::BatteryPolicy::default(),
            sound: true,
            startup: false,
            theme: AppTheme::System,
            language: AppLanguage::System,
            show_tray: true,
            shell_opacity: DEFAULT_SHELL_OPACITY,
            pin: Hotkey::pin_default(),
            opacity_up: Hotkey::opacity_up_default(),
            opacity_down: Hotkey::opacity_down_default(),
            frame_color: None,
            frame_opacity: DEFAULT_FRAME_OPACITY,
            frame_thickness: DEFAULT_FRAME_THICKNESS,
            ruler_hotkey: Hotkey::ruler_default(),
            ruler_mode: crate::screen_ruler::Mode::None,
            ruler_tolerance: crate::screen_ruler::DEFAULT_TOLERANCE,
            ruler_per_channel: false,
            ruler_feet: true,
            ruler_continuous: false,
            ruler_unit: crate::screen_ruler::Unit::Pixel,
            ruler_color: crate::screen_ruler::DEFAULT_LINE_COLOR,
            resize: crate::image_resizer::ResizeSettings::default(),
            power_store: crate::power_store::PowerStorePreferences::default(),
            rename: crate::media_renamer::Preferences::default(),
            mouse: crate::power_mouse::Preferences::default(),
            modules: Modules::default(),
        }
    }
}
fn parse_flag(value: &str) -> io::Result<bool> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(io::Error::other(
            "PowerKit ayar dosyası geçersiz veya sürümü desteklenmiyor",
        )),
    }
}
fn invalid() -> io::Error {
    io::Error::other("PowerKit ayar dosyası geçersiz veya sürümü desteklenmiyor")
}
fn parse_hotkey(value: &str) -> io::Result<Hotkey> {
    let (mods, vk) = value.split_once(',').ok_or_else(invalid)?;
    let mods = mods.parse::<u8>().map_err(|_| invalid())?;
    let vk = vk.parse::<u16>().map_err(|_| invalid())?;
    Hotkey::from_storage(mods, vk).ok_or_else(invalid)
}
fn parse_color(value: &str) -> io::Result<Option<u32>> {
    if value == "accent" {
        return Ok(None);
    }
    if value.len() != 6 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    let rgb = u32::from_str_radix(value, 16).map_err(|_| invalid())?;
    let r = (rgb >> 16) & 0xff;
    let g = (rgb >> 8) & 0xff;
    let b = rgb & 0xff;
    Ok(Some(r | (g << 8) | (b << 16)))
}
fn color_value(color: Option<u32>) -> String {
    match color {
        None => "accent".into(),
        Some(color) => format!(
            "{:02X}{:02X}{:02X}",
            color & 0xff,
            (color >> 8) & 0xff,
            (color >> 16) & 0xff
        ),
    }
}
pub const DEFAULT_SHELL_OPACITY: i32 = 100;
pub const MIN_SHELL_OPACITY: i32 = 50;
pub const MAX_SHELL_OPACITY: i32 = 100;

pub fn clamp_shell_opacity(value: i32) -> i32 {
    value.clamp(MIN_SHELL_OPACITY, MAX_SHELL_OPACITY)
}

fn parse_bounded(value: &str, clamp: fn(i32) -> i32) -> io::Result<i32> {
    let parsed = value.parse::<i32>().map_err(|_| invalid())?;
    let next = clamp(parsed);
    if next != parsed {
        return Err(invalid());
    }
    Ok(next)
}
fn parse(text: &str) -> io::Result<Preferences> {
    let text = text.trim().replace("\r\n", "\n");
    let mut lines = text.split('\n');
    let Some("version=1") = lines.next() else {
        return Err(invalid());
    };
    let mut prefs = Preferences::default();
    let mut display = None;
    let mut interval = None;
    let mut always_on_hotkey = None;
    let mut always_on_warn = None;
    let mut ac_only = None;
    let mut battery_floor = None;
    let mut sound = None;
    let mut startup = None;
    let mut theme = None;
    let mut language = None;
    let mut show_tray = None;
    let mut shell_opacity = None;
    let mut pin = None;
    let mut opacity_up = None;
    let mut opacity_down = None;
    let mut frame_color = None;
    let mut frame_opacity = None;
    let mut frame_thickness = None;
    let mut ruler_hotkey = None;
    let mut ruler_mode = None;
    let mut ruler_tolerance = None;
    let mut ruler_per_channel = None;
    let mut ruler_feet = None;
    let mut ruler_continuous = None;
    let mut ruler_unit = None;
    let mut ruler_color = None;
    let mut resize_shrink = None;
    let mut resize_replace = None;
    let mut resize_ignore_orientation = None;
    let mut resize_remove_metadata = None;
    let mut resize_jpeg = None;
    let mut resize_png = None;
    let mut resize_tiff = None;
    let mut resize_keep_date = None;
    let mut resize_encoder = None;
    let mut resize_filename = None;
    let mut resize_selected = None;
    let mut resize_custom = None;
    let mut resize_sizes = Vec::new();
    let mut power_store_check = None;
    let mut power_store_notify = None;
    let mut power_store_remote_icons = None;
    let mut power_store = crate::power_store::PowerStorePreferences::default();
    let mut rename_template = None;
    let mut rename_recurse = None;
    let mut rename_mtime_fallback = None;
    let mut rename_lowercase_ext = None;
    let mut mouse_enabled = None;
    let mut mouse = crate::power_mouse::Preferences::default();
    let mut module_always_on = None;
    let mut module_always_top = None;
    let mut module_screen_ruler = None;
    let mut module_image_resizer = None;
    let mut module_power_store = None;
    let mut module_media_renamer = None;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(invalid());
        };
        match key {
            "display" if display.is_none() => display = Some(parse_flag(value)?),
            "always_on_interval" if interval.is_none() => {
                let minutes = value.parse::<i64>().map_err(|_| invalid())?;
                if i64::from(crate::always_on::clamp_interval(minutes)) != minutes {
                    return Err(invalid());
                }
                interval = Some(minutes as u32);
            }
            "always_on_hotkey" if always_on_hotkey.is_none() => {
                always_on_hotkey = Some(parse_hotkey(value)?)
            }
            "always_on_warn" if always_on_warn.is_none() => {
                always_on_warn = Some(parse_flag(value)?)
            }
            "always_on_ac_only" if ac_only.is_none() => ac_only = Some(parse_flag(value)?),
            "always_on_battery_floor" if battery_floor.is_none() => {
                let floor = parse_bounded(value, |v| {
                    i32::from(crate::always_on::clamp_battery_floor(v))
                })?;
                battery_floor = Some(floor as u8);
            }
            "sound" if sound.is_none() => sound = Some(parse_flag(value)?),
            "startup" if startup.is_none() => startup = Some(parse_flag(value)?),
            "theme" if theme.is_none() => theme = Some(AppTheme::parse(value)?),
            "language" if language.is_none() => language = Some(AppLanguage::parse(value)?),
            "show_tray" if show_tray.is_none() => show_tray = Some(parse_flag(value)?),
            // Retired in-app update preferences; still valid in older settings files.
            "auto_update" | "update_notify" | "show_notes" => {
                parse_flag(value)?;
            }
            "shell_opacity" if shell_opacity.is_none() => {
                shell_opacity = Some(parse_bounded(value, clamp_shell_opacity)?)
            }
            "pin" if pin.is_none() => pin = Some(parse_hotkey(value)?),
            "opacity_up" if opacity_up.is_none() => opacity_up = Some(parse_hotkey(value)?),
            "opacity_down" if opacity_down.is_none() => opacity_down = Some(parse_hotkey(value)?),
            "frame_color" if frame_color.is_none() => frame_color = Some(parse_color(value)?),
            "frame_opacity" if frame_opacity.is_none() => {
                frame_opacity = Some(parse_bounded(value, clamp_frame_opacity)?)
            }
            "frame_thickness" if frame_thickness.is_none() => {
                frame_thickness = Some(parse_bounded(value, clamp_frame_thickness)?)
            }
            "ruler_hotkey" if ruler_hotkey.is_none() => ruler_hotkey = Some(parse_hotkey(value)?),
            "ruler_mode" if ruler_mode.is_none() => ruler_mode = Some(parse_mode(value)?),
            "ruler_tolerance" if ruler_tolerance.is_none() => {
                ruler_tolerance = Some(parse_tolerance(value)?)
            }
            "ruler_per_channel" if ruler_per_channel.is_none() => {
                ruler_per_channel = Some(parse_flag(value)?)
            }
            "ruler_feet" if ruler_feet.is_none() => ruler_feet = Some(parse_flag(value)?),
            "ruler_continuous" if ruler_continuous.is_none() => {
                ruler_continuous = Some(parse_flag(value)?)
            }
            "ruler_unit" if ruler_unit.is_none() => ruler_unit = Some(parse_unit(value)?),
            "ruler_color" if ruler_color.is_none() => ruler_color = Some(parse_rgb(value)?),
            "resize_shrink" if resize_shrink.is_none() => resize_shrink = Some(parse_flag(value)?),
            "resize_replace" if resize_replace.is_none() => {
                resize_replace = Some(parse_flag(value)?)
            }
            "resize_ignore_orientation" if resize_ignore_orientation.is_none() => {
                resize_ignore_orientation = Some(parse_flag(value)?)
            }
            "resize_remove_metadata" if resize_remove_metadata.is_none() => {
                resize_remove_metadata = Some(parse_flag(value)?)
            }
            "resize_jpeg" if resize_jpeg.is_none() => {
                let quality = value.parse::<i32>().map_err(|_| invalid())?;
                if !(1..=100).contains(&quality) {
                    return Err(invalid());
                }
                resize_jpeg = Some(quality);
            }
            "resize_png" if resize_png.is_none() => {
                let index = value.parse::<i32>().map_err(|_| invalid())?;
                resize_png = Some(
                    crate::image_resizer::PngInterlace::from_index(index).ok_or_else(invalid)?,
                );
            }
            "resize_tiff" if resize_tiff.is_none() => {
                let index = value.parse::<i32>().map_err(|_| invalid())?;
                resize_tiff = Some(
                    crate::image_resizer::TiffCompression::from_index(index).ok_or_else(invalid)?,
                );
            }
            "resize_keep_date" if resize_keep_date.is_none() => {
                resize_keep_date = Some(parse_flag(value)?)
            }
            "resize_encoder" if resize_encoder.is_none() => {
                resize_encoder =
                    Some(crate::image_resizer::FallbackEncoder::parse(value).ok_or_else(invalid)?);
            }
            "resize_filename" if resize_filename.is_none() => {
                resize_filename = Some(
                    crate::image_resizer::ResizeSettings::parse_filename(value)
                        .ok_or_else(invalid)?,
                );
            }
            "resize_selected" if resize_selected.is_none() => {
                resize_selected = Some(value.parse::<usize>().map_err(|_| invalid())?);
            }
            "resize_custom" if resize_custom.is_none() => {
                resize_custom =
                    Some(crate::image_resizer::Size::parse_setting(value).ok_or_else(invalid)?);
            }
            "resize_size" => {
                if resize_sizes.len() >= 16 {
                    return Err(invalid());
                }
                resize_sizes
                    .push(crate::image_resizer::Size::parse_setting(value).ok_or_else(invalid)?);
            }
            "power_store_check_minutes" if power_store_check.is_none() => {
                let minutes = value.parse::<u32>().map_err(|_| invalid())?;
                if crate::power_store::clamp_check_minutes(minutes) != minutes {
                    return Err(invalid());
                }
                power_store_check = Some(minutes);
            }
            "power_store_ignore" => {
                let key = crate::power_store::parse_package_key(value).ok_or_else(invalid)?;
                if !power_store.filters.ignore(key) {
                    return Err(invalid());
                }
            }
            "power_store_skip" => {
                let (key_text, version) = value.rsplit_once('|').ok_or_else(invalid)?;
                if version.is_empty() {
                    return Err(invalid());
                }
                let key = crate::power_store::parse_package_key(key_text).ok_or_else(invalid)?;
                if !power_store.filters.skip_version(key, version) {
                    return Err(invalid());
                }
            }
            "power_store_notify" if power_store_notify.is_none() => {
                power_store_notify = Some(parse_flag(value)?);
            }
            "power_store_remote_icons" if power_store_remote_icons.is_none() => {
                power_store_remote_icons = Some(parse_flag(value)?);
            }
            "power_store_option" => {
                let (key, options) =
                    crate::power_store::parse_options_line(value).ok_or_else(invalid)?;
                if !power_store.set_options(key, options) {
                    return Err(invalid());
                }
            }
            "rename_template" if rename_template.is_none() => rename_template = Some(value),
            "rename_recurse" if rename_recurse.is_none() => {
                rename_recurse = Some(parse_flag(value)?)
            }
            "rename_mtime_fallback" if rename_mtime_fallback.is_none() => {
                rename_mtime_fallback = Some(parse_flag(value)?)
            }
            "rename_lowercase_ext" if rename_lowercase_ext.is_none() => {
                rename_lowercase_ext = Some(parse_flag(value)?)
            }
            "mouse_enabled" if mouse_enabled.is_none() => mouse_enabled = Some(parse_flag(value)?),
            "module_always_on" if module_always_on.is_none() => {
                module_always_on = Some(parse_flag(value)?)
            }
            "module_always_top" if module_always_top.is_none() => {
                module_always_top = Some(parse_flag(value)?)
            }
            "module_screen_ruler" if module_screen_ruler.is_none() => {
                module_screen_ruler = Some(parse_flag(value)?)
            }
            "module_image_resizer" if module_image_resizer.is_none() => {
                module_image_resizer = Some(parse_flag(value)?)
            }
            "module_power_store" if module_power_store.is_none() => {
                module_power_store = Some(parse_flag(value)?)
            }
            "module_media_renamer" if module_media_renamer.is_none() => {
                module_media_renamer = Some(parse_flag(value)?)
            }
            key if key.starts_with("mouse_") && key != "mouse_enabled" => {
                let (trigger, binding) =
                    crate::power_mouse::Preferences::parse_binding_line(key, value)
                        .ok_or_else(invalid)?;
                if mouse.bindings.contains_key(&trigger) {
                    return Err(invalid());
                }
                mouse.bindings.insert(trigger, binding);
            }
            _ => return Err(invalid()),
        }
    }
    prefs.display = display.ok_or_else(invalid)?;
    if let Some(interval) = interval {
        prefs.interval_minutes = interval;
    }
    if let Some(hotkey) = always_on_hotkey {
        prefs.always_on_hotkey = hotkey;
    }
    if let Some(warn) = always_on_warn {
        prefs.always_on_warn = warn;
    }
    if let Some(ac_only) = ac_only {
        prefs.always_on_battery.ac_only = ac_only;
    }
    if let Some(floor) = battery_floor {
        prefs.always_on_battery.floor = floor;
    }
    if let Some(sound) = sound {
        prefs.sound = sound;
    }
    if let Some(startup) = startup {
        prefs.startup = startup;
    }
    if let Some(theme) = theme {
        prefs.theme = theme;
    }
    if let Some(language) = language {
        prefs.language = language;
    }
    if let Some(show_tray) = show_tray {
        prefs.show_tray = show_tray;
    }
    if let Some(shell_opacity) = shell_opacity {
        prefs.shell_opacity = shell_opacity;
    }
    if let Some(pin) = pin {
        prefs.pin = pin;
    }
    if let Some(opacity_up) = opacity_up {
        prefs.opacity_up = opacity_up;
    }
    if let Some(opacity_down) = opacity_down {
        prefs.opacity_down = opacity_down;
    }
    if let Some(frame_color) = frame_color {
        prefs.frame_color = frame_color;
    }
    if let Some(frame_opacity) = frame_opacity {
        prefs.frame_opacity = frame_opacity;
    }
    if let Some(frame_thickness) = frame_thickness {
        prefs.frame_thickness = frame_thickness;
    }
    if let Some(ruler_hotkey) = ruler_hotkey {
        prefs.ruler_hotkey = ruler_hotkey;
    }
    if let Some(ruler_mode) = ruler_mode {
        prefs.ruler_mode = ruler_mode;
    }
    if let Some(ruler_tolerance) = ruler_tolerance {
        prefs.ruler_tolerance = ruler_tolerance;
    }
    if let Some(ruler_per_channel) = ruler_per_channel {
        prefs.ruler_per_channel = ruler_per_channel;
    }
    if let Some(ruler_feet) = ruler_feet {
        prefs.ruler_feet = ruler_feet;
    }
    if let Some(ruler_continuous) = ruler_continuous {
        prefs.ruler_continuous = ruler_continuous;
    }
    if let Some(ruler_unit) = ruler_unit {
        prefs.ruler_unit = ruler_unit;
    }
    if let Some(ruler_color) = ruler_color {
        prefs.ruler_color = ruler_color;
    }
    if let Some(shrink) = resize_shrink {
        prefs.resize.shrink_only = shrink;
    }
    if let Some(replace) = resize_replace {
        prefs.resize.replace = replace;
    }
    if let Some(ignore) = resize_ignore_orientation {
        prefs.resize.ignore_orientation = ignore;
    }
    if let Some(remove) = resize_remove_metadata {
        prefs.resize.remove_metadata = remove;
    }
    if let Some(quality) = resize_jpeg {
        prefs.resize.jpeg_quality = quality;
    }
    if let Some(png) = resize_png {
        prefs.resize.png = png;
    }
    if let Some(tiff) = resize_tiff {
        prefs.resize.tiff = tiff;
    }
    if let Some(keep) = resize_keep_date {
        prefs.resize.keep_date = keep;
    }
    if let Some(encoder) = resize_encoder {
        prefs.resize.fallback = encoder;
    }
    if let Some(filename) = resize_filename {
        prefs.resize.filename = filename;
    }
    if !resize_sizes.is_empty() {
        prefs.resize.sizes = resize_sizes;
    }
    if let Some(custom) = resize_custom {
        prefs.resize.custom = custom;
    }
    if let Some(selected) = resize_selected {
        if selected > prefs.resize.sizes.len() {
            return Err(invalid());
        }
        prefs.resize.selected = selected;
    }
    if let Some(minutes) = power_store_check {
        power_store.check_minutes = minutes;
    }
    if let Some(notify) = power_store_notify {
        power_store.notify = notify;
    }
    if let Some(remote_icons) = power_store_remote_icons {
        power_store.remote_icons = remote_icons;
    }
    prefs.power_store = power_store;
    if let Some(template) = rename_template {
        prefs.rename.set_template(template);
    }
    if let Some(recurse) = rename_recurse {
        prefs.rename.recurse = recurse;
    }
    if let Some(fallback) = rename_mtime_fallback {
        prefs.rename.mtime_fallback = fallback;
    }
    if let Some(lowercase) = rename_lowercase_ext {
        prefs.rename.lowercase_ext = lowercase;
    }
    if let Some(enabled) = mouse_enabled {
        mouse.enabled = enabled;
    }
    prefs.mouse = mouse;
    if let Some(enabled) = module_always_on {
        prefs.modules.always_on = enabled;
    }
    if let Some(enabled) = module_always_top {
        prefs.modules.always_top = enabled;
    }
    if let Some(enabled) = module_screen_ruler {
        prefs.modules.screen_ruler = enabled;
    }
    if let Some(enabled) = module_image_resizer {
        prefs.modules.image_resizer = enabled;
    }
    if let Some(enabled) = module_power_store {
        prefs.modules.power_store = enabled;
    }
    if let Some(enabled) = module_media_renamer {
        prefs.modules.media_renamer = enabled;
    }
    Ok(prefs)
}
fn parse_mode(value: &str) -> io::Result<crate::screen_ruler::Mode> {
    let index = value.parse::<i32>().map_err(|_| invalid())?;
    crate::screen_ruler::Mode::from_index(index).ok_or_else(invalid)
}
fn parse_unit(value: &str) -> io::Result<crate::screen_ruler::Unit> {
    let index = value.parse::<i32>().map_err(|_| invalid())?;
    crate::screen_ruler::Unit::from_index(index).ok_or_else(invalid)
}
fn parse_tolerance(value: &str) -> io::Result<u8> {
    let parsed = value.parse::<i32>().map_err(|_| invalid())?;
    u8::try_from(parsed).map_err(|_| invalid())
}
fn parse_rgb(value: &str) -> io::Result<u32> {
    parse_color(value)?.ok_or_else(invalid)
}
pub fn load(path: &Path) -> io::Result<Preferences> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Preferences::default()),
        Err(e) => return Err(e),
    };
    let mut text = String::new();
    file.take(65_537).read_to_string(&mut text)?;
    if text.len() > 65_536 {
        return Err(io::Error::other("PowerKit ayar dosyası çok büyük"));
    }
    parse(&text)
}
#[cfg(windows)]
pub fn save(path: &Path, prefs: Preferences) -> io::Result<()> {
    use std::{io::Write, os::windows::ffi::OsStrExt};
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    std::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| io::Error::other("Ayar klasörü yok"))?,
    )?;
    // Exclusive staging prevents another process/session from truncating our write.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temp = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    struct Staging(std::path::PathBuf);
    impl Drop for Staging {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let _staging = Staging(temp.clone());
    let mut resize = String::new();
    prefs.resize.append_settings(&mut resize);
    let mut power_store = String::new();
    prefs.power_store.append_settings(&mut power_store);
    let mut rename = String::new();
    prefs.rename.append_settings(&mut rename);
    let mut mouse = String::new();
    prefs.mouse.append_settings(&mut mouse);
    let mut modules = String::new();
    prefs.modules.append_settings(&mut modules);
    let written = write!(
        file,
        "version=1\ndisplay={}\nalways_on_interval={}\nalways_on_hotkey={},{}\nalways_on_warn={}\nalways_on_ac_only={}\nalways_on_battery_floor={}\nsound={}\nstartup={}\ntheme={}\nlanguage={}\nshow_tray={}\nshell_opacity={}\npin={},{}\nopacity_up={},{}\nopacity_down={},{}\nframe_color={}\nframe_opacity={}\nframe_thickness={}\nruler_hotkey={},{}\nruler_mode={}\nruler_tolerance={}\nruler_per_channel={}\nruler_feet={}\nruler_continuous={}\nruler_unit={}\nruler_color={}\n{resize}{power_store}{rename}{mouse}{modules}",
        prefs.display,
        prefs.interval_minutes,
        prefs.always_on_hotkey.mods_mask(),
        prefs.always_on_hotkey.vk,
        prefs.always_on_warn,
        prefs.always_on_battery.ac_only,
        prefs.always_on_battery.floor,
        prefs.sound,
        prefs.startup,
        prefs.theme.as_str(),
        prefs.language.as_str(),
        prefs.show_tray,
        prefs.shell_opacity,
        prefs.pin.mods_mask(),
        prefs.pin.vk,
        prefs.opacity_up.mods_mask(),
        prefs.opacity_up.vk,
        prefs.opacity_down.mods_mask(),
        prefs.opacity_down.vk,
        color_value(prefs.frame_color),
        prefs.frame_opacity,
        prefs.frame_thickness,
        prefs.ruler_hotkey.mods_mask(),
        prefs.ruler_hotkey.vk,
        prefs.ruler_mode.index(),
        prefs.ruler_tolerance,
        prefs.ruler_per_channel,
        prefs.ruler_feet,
        prefs.ruler_continuous,
        prefs.ruler_unit.index(),
        color_value(Some(prefs.ruler_color))
    )
    .and_then(|_| file.sync_all());
    drop(file);
    written?;
    let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: live nul-terminated paths; replacing a complete file on the same volume.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_incomplete_and_duplicate_settings_are_rejected() {
        for text in [
            "",
            "display=true",
            "version=2\ndisplay=true",
            "version=1\ndisplay=yes",
            "version=1\ndisplay=true\ndisplay=false",
            "version=1\nsound=false",
            "version=1\ndisplay=true\nframe=true",
            "version=1\ndisplay=true\npin=0,84",
            "version=1\ndisplay=true\nframe_opacity=0",
            "version=1\ndisplay=true\nframe_color=ZZZZZZ",
            "version=1\ndisplay=true\nruler_mode=9",
            "version=1\ndisplay=true\nruler_color=accent",
            "version=1\ndisplay=true\nalways_on_interval=0",
            "version=1\ndisplay=true\nalways_on_interval=6000",
            "version=1\ndisplay=true\nalways_on_interval=90\nalways_on_interval=90",
            "version=1\ndisplay=true\nalways_on_battery_floor=25",
            "version=1\ndisplay=true\nalways_on_battery_floor=100",
            "version=1\ndisplay=true\nalways_on_warn=maybe",
            "version=1\ndisplay=true\nalways_on_hotkey=0,65",
        ] {
            assert!(parse(text).is_err());
        }
        assert_eq!(
            parse("version=1\r\ndisplay=true\r\n").unwrap(),
            Preferences {
                display: true,
                ..Preferences::default()
            }
        );
        assert_eq!(
            parse("version=1\ndisplay=false\nsound=false").unwrap(),
            Preferences {
                display: false,
                sound: false,
                ..Preferences::default()
            }
        );
        assert_eq!(
            parse(
                "version=1\ndisplay=true\nstartup=true\ntheme=dark\nlanguage=tr\nshow_tray=false\nauto_update=false\nupdate_notify=false\nshow_notes=true\n",
            )
            .unwrap(),
            Preferences {
                display: true,
                startup: true,
                theme: AppTheme::Dark,
                language: AppLanguage::Turkish,
                show_tray: false,
                ..Preferences::default()
            }
        );
        assert!(parse("version=1\ndisplay=true\ntheme=purple").is_err());
        assert!(parse("version=1\ndisplay=true\nauto_update=maybe").is_err());
        assert_eq!(
            parse("version=1\ndisplay=false\nalways_on_interval=195")
                .unwrap()
                .interval_minutes,
            195
        );
        let always_on = parse(
            "version=1\ndisplay=false\nalways_on_warn=false\nalways_on_ac_only=true\nalways_on_battery_floor=30\nalways_on_hotkey=11,65\n",
        )
        .unwrap();
        assert!(!always_on.always_on_warn);
        assert_eq!(
            always_on.always_on_battery,
            crate::always_on::BatteryPolicy {
                ac_only: true,
                floor: 30
            }
        );
        assert_eq!(always_on.always_on_hotkey, Hotkey::always_on_default());
        let custom = parse(
            "version=1\ndisplay=true\nsound=false\npin=3,84\nopacity_up=3,187\nopacity_down=3,189\nframe_color=0078D4\nframe_opacity=80\nframe_thickness=8\n",
        )
        .unwrap();
        assert_eq!(custom.frame_opacity, 80);
        assert_eq!(custom.frame_thickness, 8);
        assert_eq!(custom.frame_color, Some(0x00D4_7800));
        assert_eq!(custom.pin, Hotkey::pin_default());
        let ruler = parse(
            "version=1\ndisplay=true\nruler_mode=1\nruler_tolerance=45\nruler_unit=2\nruler_color=FF4500\nruler_continuous=true\nruler_feet=false\nruler_per_channel=true\nruler_hotkey=11,77\n",
        )
        .unwrap();
        assert_eq!(ruler.ruler_mode, crate::screen_ruler::Mode::Bounds);
        assert_eq!(ruler.ruler_unit, crate::screen_ruler::Unit::Centimetre);
        assert_eq!(ruler.ruler_tolerance, 45);
        assert!(ruler.ruler_continuous);
        assert!(!ruler.ruler_feet);
        assert!(ruler.ruler_per_channel);
        assert_eq!(ruler.ruler_color, crate::screen_ruler::DEFAULT_LINE_COLOR);
        assert_eq!(ruler.ruler_hotkey, Hotkey::ruler_default());
        let mut body = String::from("version=1\ndisplay=true\n");
        Preferences::default().resize.append_settings(&mut body);
        let loaded = parse(&body).unwrap();
        assert_eq!(loaded.resize, Preferences::default().resize);
        let power_store = parse(
            "version=1\ndisplay=true\npower_store_check_minutes=60\npower_store_ignore=winget|winget|Ignored.App\npower_store_skip=winget|winget|Skipped.App|2.0.0\n",
        )
        .unwrap()
        .power_store;
        assert_eq!(power_store.check_minutes, 60);
        assert!(power_store.filters.blocks_update(
            &crate::power_store::PackageKey::new(
                crate::power_store::ManagerKind::WinGet,
                "winget",
                "Ignored.App"
            ),
            "9.9.9"
        ));
        assert!(power_store.filters.blocks_update(
            &crate::power_store::PackageKey::new(
                crate::power_store::ManagerKind::WinGet,
                "winget",
                "Skipped.App"
            ),
            "2.0.0"
        ));
        assert!(parse("version=1\ndisplay=true\npower_store_check_minutes=1").is_err());
        assert!(parse("version=1\ndisplay=true\npower_store_ignore=nope").is_err());
        let configured = parse(
            "version=1\ndisplay=true\npower_store_notify=false\npower_store_option=winget|winget|Git.Git|machine|arm64||--silent\n",
        )
        .unwrap()
        .power_store;
        assert!(!configured.notify);
        let git = configured.options_for(&crate::power_store::PackageKey::new(
            crate::power_store::ManagerKind::WinGet,
            "winget",
            "Git.Git",
        ));
        assert_eq!(git.scope.as_deref(), Some("machine"));
        assert_eq!(git.extra_args.as_deref(), Some("--silent"));
        assert!(
            parse("version=1\ndisplay=true\npower_store_option=winget|winget|Git.Git|root||||")
                .is_err()
        );
        assert!(parse("version=1\ndisplay=true\nresize_jpeg=0").is_err());
        assert!(parse("version=1\ndisplay=true\nresize_filename=").is_err());
        assert!(parse("version=1\ndisplay=true\nresize_size=bad").is_err());
        let custom = parse(
            "version=1\ndisplay=true\nresize_shrink=true\nresize_filename=%1-%5\nresize_jpeg=80\nresize_selected=4\n",
        )
        .unwrap();
        assert!(custom.resize.shrink_only);
        assert_eq!(custom.resize.filename, "%1-%5");
        assert_eq!(custom.resize.jpeg_quality, 80);
        assert_eq!(custom.resize.selected, 4);
        let rename = parse(
            "version=1\ndisplay=true\nrename_template={yyyy}={MM}\nrename_recurse=true\nrename_lowercase_ext=true\n",
        )
        .unwrap()
        .rename;
        assert_eq!(rename.template, "{yyyy}={MM}");
        assert!(rename.recurse && rename.lowercase_ext && !rename.mtime_fallback);
        let fallback = parse("version=1\ndisplay=true\nrename_template={nope}\n").unwrap();
        assert_eq!(
            fallback.rename.template,
            crate::media_renamer::template::DEFAULT
        );
        assert!(parse("version=1\ndisplay=true\nrename_recurse=1\nrename_recurse=1").is_err());
        let mouse = parse(
            "version=1\ndisplay=true\nmouse_enabled=true\nmouse_middle=do:copy\nmouse_f13=long:paste|undo\nmouse_back=gesture:mute_volume|volume_up|volume_down|prev_tab|next_tab\n",
        )
        .unwrap()
        .mouse;
        assert!(mouse.enabled);
        assert_eq!(
            mouse.binding(crate::power_mouse::Trigger::Middle),
            crate::power_mouse::Binding::Single(crate::power_mouse::Action::Copy)
        );
        assert_eq!(
            mouse.binding(crate::power_mouse::Trigger::F13),
            crate::power_mouse::Binding::LongPress {
                short: crate::power_mouse::Action::Paste,
                long: crate::power_mouse::Action::Undo,
            }
        );
        assert!(matches!(
            mouse.binding(crate::power_mouse::Trigger::Back),
            crate::power_mouse::Binding::Gesture { .. }
        ));
        assert!(parse("version=1\ndisplay=true\nmouse_middle=do:nope").is_err());
        assert!(
            parse("version=1\ndisplay=true\nmouse_tilt_left=gesture:none|none|none|none|none")
                .is_err()
        );
        assert!(
            parse("version=1\ndisplay=true\nmouse_middle=do:copy\nmouse_middle=do:paste\n")
                .is_err()
        );
        let mut round_trip = String::from("version=1\ndisplay=true\n");
        mouse.append_settings(&mut round_trip);
        assert_eq!(parse(&round_trip).unwrap().mouse, mouse);
    }
    #[cfg(windows)]
    #[test]
    fn failed_replace_preserves_target_and_cleans_staging_file() {
        let dir = std::env::temp_dir().join(format!("powerkit-failure-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let destination = dir.join("settings.txt");
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("sentinel"), "unchanged").unwrap();
        assert!(
            save(
                &destination,
                Preferences {
                    display: true,
                    ..Preferences::default()
                }
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("sentinel")).unwrap(),
            "unchanged"
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_file(destination.join("sentinel")).unwrap();
        std::fs::remove_dir(destination).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn atomic_save_replaces_existing_file_and_roundtrips() {
        let dir = std::env::temp_dir().join(format!(
            "powerkit-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("settings.txt");
        assert_eq!(load(&path).unwrap(), Preferences::default());
        save(
            &path,
            Preferences {
                display: true,
                interval_minutes: 75,
                always_on_warn: false,
                always_on_battery: crate::always_on::BatteryPolicy {
                    ac_only: true,
                    floor: 20,
                },
                sound: false,
                frame_color: Some(0x00D4_7800),
                frame_opacity: 80,
                frame_thickness: 8,
                ..Preferences::default()
            },
        )
        .unwrap();
        assert_eq!(
            load(&path).unwrap(),
            Preferences {
                display: true,
                interval_minutes: 75,
                always_on_warn: false,
                always_on_battery: crate::always_on::BatteryPolicy {
                    ac_only: true,
                    floor: 20,
                },
                sound: false,
                frame_color: Some(0x00D4_7800),
                frame_opacity: 80,
                frame_thickness: 8,
                ..Preferences::default()
            }
        );
        save(
            &path,
            Preferences {
                display: false,
                sound: true,
                ..Preferences::default()
            },
        )
        .unwrap();
        assert_eq!(
            load(&path).unwrap(),
            Preferences {
                display: false,
                sound: true,
                ..Preferences::default()
            }
        );
        std::fs::write(&path, "x".repeat(4097)).unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}
