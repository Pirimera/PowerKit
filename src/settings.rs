//! Small versioned settings file; no runtime framework or network dependency.
use std::{
    io::{self, Read},
    path::Path,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub display: bool,
    pub sound: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            display: false,
            sound: true,
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
fn parse(text: &str) -> io::Result<Preferences> {
    let text = text.trim().replace("\r\n", "\n");
    let mut lines = text.split('\n');
    let Some("version=1") = lines.next() else {
        return Err(io::Error::other(
            "PowerKit ayar dosyası geçersiz veya sürümü desteklenmiyor",
        ));
    };
    let mut display = None;
    let mut sound = None;
    for line in lines {
        let Some((key, value)) = line.split_once('=') else {
            return Err(io::Error::other(
                "PowerKit ayar dosyası geçersiz veya sürümü desteklenmiyor",
            ));
        };
        match key {
            "display" if display.is_none() => display = Some(parse_flag(value)?),
            "sound" if sound.is_none() => sound = Some(parse_flag(value)?),
            _ => {
                return Err(io::Error::other(
                    "PowerKit ayar dosyası geçersiz veya sürümü desteklenmiyor",
                ));
            }
        }
    }
    Ok(Preferences {
        display: display.ok_or_else(|| {
            io::Error::other("PowerKit ayar dosyası geçersiz veya sürümü desteklenmiyor")
        })?,
        sound: sound.unwrap_or(true),
    })
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
        "version=1\ndisplay={}\nsound={}\n",
        prefs.display, prefs.sound
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
        ] {
            assert!(parse(text).is_err());
        }
        assert_eq!(
            parse("version=1\r\ndisplay=true\r\n").unwrap(),
            Preferences {
                display: true,
                sound: true
            }
        );
        assert_eq!(
            parse("version=1\ndisplay=false\nsound=false").unwrap(),
            Preferences {
                display: false,
                sound: false
            }
        );
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
                    sound: true
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
            },
        )
        .unwrap();
        assert_eq!(
            load(&path).unwrap(),
            Preferences {
                display: true,
                sound: false
            }
        );
        save(
            &path,
            Preferences {
                display: false,
                sound: true,
            },
        )
        .unwrap();
        assert_eq!(
            load(&path).unwrap(),
            Preferences {
                display: false,
                sound: true
            }
        );
        std::fs::write(&path, "x".repeat(4097)).unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}
