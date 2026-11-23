//! The file primitives a rename batch is built from: `MoveFileExW` without
//! replace, a file's fingerprint and an all-or-nothing write of the history
//! file. The order of a batch (revalidate, journal first, group rollback) is
//! `powerkit_app::controller::media_renamer::commit`; the tests here run it over real
//! files.
use super::wide_path;
use powerkit_core::media_renamer::history::Fingerprint;
use std::{io, os::windows::fs::MetadataExt, path::Path};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

/// Renames without ever replacing an existing file.
pub(super) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    let (from, to) = (wide_path(from), wide_path(to));
    // SAFETY: both paths are live NUL-terminated strings for the duration of the call.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
pub(super) fn fingerprint(path: &Path) -> Option<Fingerprint> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    metadata.is_file().then(|| {
        (
            metadata.file_size(),
            super::super::clock::file_time(metadata.last_write_time()),
        )
    })
}
pub(super) fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Writes `text` beside `path` and moves it over the file in one step, so a
/// failure leaves the previous file as it was.
pub(super) fn replace_text(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let written = std::fs::File::create(&temp).and_then(|mut file| {
        file.write_all(text.as_bytes())?;
        file.sync_all()
    });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    let (from, to) = (wide_path(&temp), wide_path(path));
    // SAFETY: live NUL-terminated paths; replaces the complete file on one volume.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        let error = io::Error::last_os_error();
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}
/// The text of a file no larger than `limit`; `Ok(None)` when it is missing.
pub(super) fn read_text(path: &Path, limit: u64) -> io::Result<Option<String>> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.len() > limit => Err(io::Error::other("the file is too large")),
        Ok(_) => std::fs::read_to_string(path).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::super::Files;
    use super::*;
    use powerkit_app::controller::media_renamer::{Outcome, Planned, commit};
    use powerkit_core::media_renamer::history::{Batch, Entry, Kind, State};
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    fn apply(
        moves: Vec<Planned>,
        history: Vec<Batch>,
        path: Option<&Path>,
        id: (u64, powerkit_core::primitives::UtcTicks),
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Outcome {
        commit::apply(&Files, moves, history, path, id, cancel, progress)
    }
    fn undo(
        target: u64,
        history: Vec<Batch>,
        path: Option<&Path>,
        id: (u64, powerkit_core::primitives::UtcTicks),
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Outcome {
        commit::undo(&Files, target, history, path, id, cancel, progress)
    }
    fn load(path: &Path) -> powerkit_app::controller::media_renamer::Loaded {
        commit::load(&Files, path)
    }
    fn write_history(path: &Path, batches: &[Batch]) -> io::Result<()> {
        commit::write_history(&Files, path, batches)
    }
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "powerkit-rename-commit-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn planned(dir: &Path, group: usize, from: &str, to: &str) -> Planned {
        let from = dir.join(from);
        let (size, modified) = fingerprint(&from).unwrap();
        Planned {
            group,
            from,
            to: dir.join(to),
            size,
            modified,
        }
    }
    fn read(dir: &Path, name: &str) -> String {
        std::fs::read_to_string(dir.join(name)).unwrap()
    }
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
    #[test]
    fn swaps_chains_and_undo_restore_every_name() {
        let dir = temp("swap");
        for (name, body) in [("a.jpg", "A"), ("b.jpg", "B"), ("c.jpg", "C")] {
            std::fs::write(dir.join(name), body).unwrap();
        }
        let history_path = dir.join("history.txt");
        let moves = vec![
            planned(&dir, 0, "a.jpg", "b.jpg"),
            planned(&dir, 1, "b.jpg", "a.jpg"),
            planned(&dir, 2, "c.jpg", "d.jpg"),
        ];
        let cancel = AtomicBool::new(false);
        let outcome = apply(
            moves,
            Vec::new(),
            Some(&history_path),
            (1, powerkit_core::primitives::UtcTicks(1)),
            &cancel,
            &mut |_, _| {},
        );
        assert!(outcome.failed.is_empty() && !outcome.stale);
        assert_eq!(
            (read(&dir, "b.jpg"), read(&dir, "a.jpg")),
            ("A".into(), "B".into())
        );
        assert_eq!(read(&dir, "d.jpg"), "C");
        assert_eq!(outcome.moved.len(), 3);
        let saved = load(&history_path).batches;
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].state, State::Done);
        assert!(saved[0].entries.iter().all(|entry| entry.via.is_none()));
        let undone = undo(
            1,
            saved,
            Some(&history_path),
            (2, powerkit_core::primitives::UtcTicks(2)),
            &cancel,
            &mut |_, _| {},
        );
        assert!(undone.failed.is_empty());
        assert_eq!(
            (
                read(&dir, "a.jpg"),
                read(&dir, "b.jpg"),
                read(&dir, "c.jpg")
            ),
            ("A".into(), "B".into(), "C".into())
        );
        let saved = load(&history_path).batches;
        assert_eq!(saved[0].kind, Kind::Undo);
        assert_eq!(saved[1].state, State::Undone);
        assert!(!saved[1].undoable());
        assert_eq!(names(&dir), ["a.jpg", "b.jpg", "c.jpg", "history.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn stale_previews_touch_nothing() {
        let dir = temp("stale");
        std::fs::write(dir.join("a.jpg"), "A").unwrap();
        let moves = vec![planned(&dir, 0, "a.jpg", "z.jpg")];
        std::fs::write(dir.join("z.jpg"), "Z").unwrap();
        let cancel = AtomicBool::new(false);
        let outcome = apply(
            moves,
            Vec::new(),
            None,
            (1, powerkit_core::primitives::UtcTicks(1)),
            &cancel,
            &mut |_, _| {},
        );
        assert!(outcome.stale);
        assert!(outcome.history.is_empty());
        assert_eq!(read(&dir, "z.jpg"), "Z");
        let mut edited = vec![planned(&dir, 0, "a.jpg", "y.jpg")];
        edited[0].size += 1;
        assert!(
            apply(
                edited,
                Vec::new(),
                None,
                (1, powerkit_core::primitives::UtcTicks(1)),
                &cancel,
                &mut |_, _| {}
            )
            .stale
        );
        assert_eq!(names(&dir), ["a.jpg", "z.jpg"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn a_failed_member_rolls_its_group_back_and_the_batch_continues() {
        let dir = temp("group");
        for name in ["p.heic", "p.mov", "q.jpg"] {
            std::fs::write(dir.join(name), name).unwrap();
        }
        let moves = vec![
            planned(&dir, 0, "p.heic", "new.heic"),
            planned(&dir, 0, "p.mov", "new.mov"),
            planned(&dir, 1, "q.jpg", "other.jpg"),
        ];
        // An exclusive handle passes revalidation but makes the rename itself fail.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(dir.join("p.mov"))
            .unwrap();
        let cancel = AtomicBool::new(false);
        let outcome = apply(
            moves,
            Vec::new(),
            None,
            (1, powerkit_core::primitives::UtcTicks(1)),
            &cancel,
            &mut |_, _| {},
        );
        drop(lock);
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(names(&dir), ["other.jpg", "p.heic", "p.mov"]);
        assert_eq!(outcome.history[0].entries.len(), 1);
        assert_eq!(outcome.history[0].failed, 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn undo_skips_files_edited_after_the_rename() {
        let dir = temp("edited");
        for name in ["a.jpg", "b.jpg"] {
            std::fs::write(dir.join(name), name).unwrap();
        }
        let moves = vec![
            planned(&dir, 0, "a.jpg", "x.jpg"),
            planned(&dir, 1, "b.jpg", "y.jpg"),
        ];
        let cancel = AtomicBool::new(false);
        let outcome = apply(
            moves,
            Vec::new(),
            None,
            (1, powerkit_core::primitives::UtcTicks(1)),
            &cancel,
            &mut |_, _| {},
        );
        std::fs::write(dir.join("y.jpg"), "edited and longer").unwrap();
        let undone = undo(
            1,
            outcome.history,
            None,
            (2, powerkit_core::primitives::UtcTicks(2)),
            &cancel,
            &mut |_, _| {},
        );
        assert_eq!(undone.skipped, 1);
        assert_eq!(names(&dir), ["a.jpg", "y.jpg"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn foreign_history_files_are_reported_and_left_alone() {
        let dir = temp("foreign");
        let path = dir.join("h.txt");
        std::fs::write(&path, "not ours\n").unwrap();
        let loaded = load(&path);
        assert!(!loaded.intact && loaded.batches.is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not ours\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn renames_beyond_max_path_without_the_manifest_opt_in() {
        let root = temp("long");
        let mut dir = root.clone();
        for _ in 0..6 {
            dir = dir.join("a-rather-long-synthetic-folder-name-to-pass-the-classic-limit");
        }
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("IMG_0001.jpg");
        std::fs::write(&from, b"long").unwrap();
        assert!(from.as_os_str().len() > 260);
        let (size, modified) = fingerprint(&from).unwrap();
        let moves = vec![Planned {
            group: 0,
            to: dir.join("2024-05-01_10-00-00.jpg"),
            from,
            size,
            modified,
        }];
        let cancel = AtomicBool::new(false);
        let outcome = apply(
            moves,
            Vec::new(),
            None,
            (1, powerkit_core::primitives::UtcTicks(1)),
            &cancel,
            &mut |_, _| {},
        );
        assert!(outcome.failed.is_empty() && !outcome.stale);
        assert_eq!(names(&dir), ["2024-05-01_10-00-00.jpg"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn cancelled_runs_and_pending_journals_recover() {
        let dir = temp("cancel");
        std::fs::write(dir.join("a.jpg"), "A").unwrap();
        let moves = vec![planned(&dir, 0, "a.jpg", "b.jpg")];
        let cancel = AtomicBool::new(true);
        let history_path = dir.join("h.txt");
        let outcome = apply(
            moves,
            Vec::new(),
            Some(&history_path),
            (1, powerkit_core::primitives::UtcTicks(1)),
            &cancel,
            &mut |_, _| {},
        );
        assert!(outcome.cancelled);
        assert!(outcome.history[0].entries.is_empty());
        let (size, modified) = fingerprint(&dir.join("a.jpg")).unwrap();
        std::fs::rename(dir.join("a.jpg"), dir.join("c.jpg")).unwrap();
        let pending = Batch {
            id: 9,
            at: powerkit_core::primitives::UtcTicks(1),
            kind: Kind::Rename,
            state: State::Pending,
            failed: 0,
            entries: vec![
                Entry {
                    from: dir.join("a.jpg"),
                    to: dir.join("c.jpg"),
                    via: None,
                    size,
                    modified,
                },
                Entry {
                    from: dir.join("m.jpg"),
                    to: dir.join("n.jpg"),
                    via: None,
                    size: 1,
                    modified: powerkit_core::primitives::UtcTicks(1),
                },
            ],
        };
        write_history(&history_path, &[pending]).unwrap();
        let loaded = load(&history_path);
        assert!(loaded.intact);
        assert_eq!(loaded.batches[0].state, State::Interrupted);
        assert_eq!(loaded.batches[0].entries.len(), 1);
        assert_eq!(load(&history_path), loaded);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
