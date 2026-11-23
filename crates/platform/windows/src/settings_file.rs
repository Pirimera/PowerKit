//! Reads the settings file with a size bound and replaces it atomically.
//!
//! The text format belongs to `powerkit_core::settings`; this module owns only
//! the file access, so the domain crate stays free of I/O and FFI.
use powerkit_core::settings::{self, Loaded, Preferences};
use std::{
    io::{self, Read, Write},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

/// A missing file is the defaults; an oversized or unsupported file is an error
/// and stays on disk untouched.
pub fn load(path: &Path) -> io::Result<Loaded> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Loaded {
                prefs: Preferences::default(),
                dropped: 0,
            });
        }
        Err(e) => return Err(e),
    };
    let mut text = String::new();
    // One byte past the limit is enough for `decode` to reject the file.
    file.take(settings::MAX_BYTES as u64 + 1)
        .read_to_string(&mut text)?;
    settings::decode(&text)
}

pub fn save(path: &Path, prefs: &Preferences) -> io::Result<()> {
    std::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| io::Error::other("settings path has no folder"))?,
    )?;
    // Exclusive staging prevents another process/session from truncating our write.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temp = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    struct Staging(PathBuf);
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
    let written = file
        .write_all(settings::encode(prefs).as_bytes())
        .and_then(|()| file.sync_all());
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

    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "powerkit-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn failed_replace_preserves_target_and_cleans_staging_file() {
        let dir = scratch("failure");
        std::fs::create_dir_all(&dir).unwrap();
        let destination = dir.join("settings.txt");
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("sentinel"), "unchanged").unwrap();
        assert!(
            save(
                &destination,
                &Preferences {
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

    #[test]
    fn atomic_save_replaces_existing_file_and_roundtrips() {
        let dir = scratch("test");
        let path = dir.join("settings.txt");
        assert_eq!(load(&path).unwrap().prefs, Preferences::default());
        let first = Preferences {
            display: true,
            interval_minutes: 75,
            sound: false,
            frame_color: Some(powerkit_core::primitives::Rgb::from_hex(0x0078D4)),
            ..Preferences::default()
        };
        save(&path, &first).unwrap();
        assert_eq!(load(&path).unwrap().prefs, first);
        let second = Preferences {
            display: false,
            sound: true,
            ..Preferences::default()
        };
        save(&path, &second).unwrap();
        assert_eq!(load(&path).unwrap().prefs, second);
        // No staging file is left beside the settings.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::write(&path, "x".repeat(4097)).unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn a_file_past_the_size_limit_is_rejected_and_left_in_place() {
        let dir = scratch("large");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.txt");
        let mut text = String::from("version=1\n");
        text.push_str(&"future=1\n".repeat(settings::MAX_BYTES / 9 + 1));
        std::fs::write(&path, &text).unwrap();
        assert!(load(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}
