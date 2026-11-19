//! Atomic text replacement for PowerStore files (history, exports and the icon database).
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Writes beside the target and renames over it so a crash never leaves half a file.
pub fn write_replacing(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Unique per write, so the history save and an export never share a staging file.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let staging = path.with_extension(format!(
        "{}.{}.powerkit-tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = std::fs::File::create_new(&staging).and_then(|mut file| {
        file.write_all(text.as_bytes())?;
        // Data must be on disk before the rename publishes it.
        file.sync_all()
    });
    written
        .and_then(|()| std::fs::rename(&staging, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&staging);
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_keeps_the_old_file_until_the_new_text_is_complete() {
        let dir = std::env::temp_dir().join(format!("powerkit-file-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("icon-database.json");
        write_replacing(&path, "first").unwrap();
        write_replacing(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        let leftovers = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".powerkit-tmp")
            })
            .count();
        assert_eq!(leftovers, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
