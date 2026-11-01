//! Small versioned settings file; no runtime framework or network dependency.
use crate::always_top::{
    DEFAULT_FRAME_OPACITY, DEFAULT_FRAME_THICKNESS, Hotkey, clamp_frame_opacity,
    clamp_frame_thickness,
};
use std::{
    io::{self, Read},
    path::Path,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub display: bool,
    pub sound: bool,
    pub pin: Hotkey,
    pub opacity_up: Hotkey,
    pub opacity_down: Hotkey,
    pub frame_color: Option<u32>,
    pub frame_opacity: i32,
    pub frame_thickness: i32,
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
    Ok(prefs)
}
pub fn load(path: &Path) -> io::Result<Preferences> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Preferences::default()),
        Err(e) => return Err(e),
    };
    let mut text = String::new();
    file.take(4097).read_to_string(&mut text)?;
    if text.len() > 4096 {
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
    let written = write!(
        file,
        "version=1\ndisplay={}\nsound={}\npin={},{}\nopacity_up={},{}\nopacity_down={},{}\nframe_color={}\nframe_opacity={}\nframe_thickness={}\n",
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
        prefs.frame_thickness
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
