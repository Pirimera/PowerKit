//! Small versioned settings file; no runtime framework or network dependency.
use crate::always_top::{
    DEFAULT_FRAME_OPACITY, DEFAULT_FRAME_THICKNESS, Hotkey, clamp_frame_opacity,
    clamp_frame_thickness,
};
use std::{
    io::{self, Read},
    path::Path,
};
#[derive(Clone, Debug, PartialEq)]
pub struct Preferences {
    pub display: bool,
    pub sound: bool,
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
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            display: false,
            sound: true,
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
    let mut sound = None;
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
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(invalid());
        };
        match key {
            "display" if display.is_none() => display = Some(parse_flag(value)?),
            "sound" if sound.is_none() => sound = Some(parse_flag(value)?),
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
            _ => return Err(invalid()),
        }
    }
    prefs.display = display.ok_or_else(invalid)?;
    if let Some(sound) = sound {
        prefs.sound = sound;
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
    let written = write!(
        file,
        "version=1\ndisplay={}\nsound={}\npin={},{}\nopacity_up={},{}\nopacity_down={},{}\nframe_color={}\nframe_opacity={}\nframe_thickness={}\nruler_hotkey={},{}\nruler_mode={}\nruler_tolerance={}\nruler_per_channel={}\nruler_feet={}\nruler_continuous={}\nruler_unit={}\nruler_color={}\n{resize}",
        prefs.display,
        prefs.sound,
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
