//! Applies and undoes rename batches on a worker thread: revalidate, journal first,
//! `MoveFileExW` without replace, group rollback, then the final history entry.
use crate::media_renamer::{
    history::{self, Batch, Entry, Fingerprint, Kind, State},
    plan::{self, Move},
};
use std::{
    collections::HashSet,
    io,
    os::windows::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

const MAX_HISTORY_BYTES: u64 = 64 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Planned {
    pub group: usize,
    pub from: PathBuf,
    pub to: PathBuf,
    pub size: u64,
    pub modified: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Outcome {
    pub history: Vec<Batch>,
    /// Applied renames as `(old path, new path)`.
    pub moved: Vec<(PathBuf, PathBuf)>,
    pub failed: Vec<(PathBuf, String)>,
    pub skipped: usize,
    pub stale: bool,
    pub cancelled: bool,
    pub journal_error: Option<String>,
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
/// Renames without ever replacing an existing file.
fn rename(from: &Path, to: &Path) -> io::Result<()> {
    let (from, to) = (wide(from), wide(to));
    // SAFETY: both paths are live NUL-terminated strings for the duration of the call.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
pub(super) fn fingerprint(path: &Path) -> Option<Fingerprint> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    metadata
        .is_file()
        .then(|| (metadata.file_size(), metadata.last_write_time() as i64))
}
fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}
fn lower(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

pub(super) fn write_history(path: &Path, batches: &[Batch]) -> io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let written = std::fs::File::create(&temp).and_then(|mut file| {
        file.write_all(history::serialize(batches).as_bytes())?;
        file.sync_all()
    });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    let (from, to) = (wide(&temp), wide(path));
    // SAFETY: live NUL-terminated paths; replaces the complete history file on one volume.
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
/// Reads history and turns batches left `pending` by a crash into `interrupted` ones.
pub(super) fn load(path: &Path) -> Vec<Batch> {
    let text = std::fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.len() <= MAX_HISTORY_BYTES)
        .and_then(|_| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    let mut batches = history::parse(&text);
    let mut changed = false;
    for batch in &mut batches {
        if batch.state == State::Pending {
            *batch = history::reconcile(batch.clone(), fingerprint);
            changed = true;
        }
    }
    if changed {
        let _ = write_history(path, &batches);
    }
    batches
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Place {
    Source,
    Temp(PathBuf),
    Target,
}
struct Run {
    entries: Vec<Entry>,
    moved: Vec<(PathBuf, PathBuf)>,
    failed: Vec<(PathBuf, String)>,
    cancelled: bool,
}
fn temp_name(dir: &Path, batch: u64, index: usize) -> PathBuf {
    let mut attempt = 0;
    loop {
        let candidate = dir.join(format!(".powerkit-rename-{batch}-{index}-{attempt}.tmp"));
        if !exists(&candidate) {
            return candidate;
        }
        attempt += 1;
    }
}
fn steps_for(moves: &[Planned], batch: u64) -> Vec<plan::Step> {
    let pairs: Vec<Move> = moves
        .iter()
        .map(|m| Move {
            from: m.from.clone(),
            to: m.to.clone(),
        })
        .collect();
    plan::order(&pairs, |index| {
        let dir = moves[index].from.parent().unwrap_or(Path::new(""));
        temp_name(dir, batch, index)
    })
}
/// Journal entries for a batch about to run; parked swap members record their temp name.
fn journal(moves: &[Planned], steps: &[plan::Step]) -> Vec<Entry> {
    let mut via = vec![None; moves.len()];
    for step in steps {
        if step.to != moves[step.index].to {
            via[step.index] = Some(step.to.clone());
        }
    }
    moves
        .iter()
        .zip(via)
        .map(|(m, via)| Entry {
            from: m.from.clone(),
            to: m.to.clone(),
            via,
            size: m.size,
            modified: m.modified,
        })
        .collect()
}
fn run(
    moves: &[Planned],
    steps: &[plan::Step],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Run {
    let mut place = vec![Place::Source; moves.len()];
    let mut done_order: Vec<usize> = Vec::new();
    let mut failed_groups: HashSet<usize> = HashSet::new();
    let mut failed = Vec::new();
    let mut cancelled = false;
    let rollback = |group: usize, place: &mut [Place], done_order: &[usize]| {
        for &index in done_order.iter().rev() {
            if moves[index].group != group {
                continue;
            }
            let current = match &place[index] {
                Place::Source => continue,
                Place::Temp(temp) => temp.clone(),
                Place::Target => moves[index].to.clone(),
            };
            if rename(&current, &moves[index].from).is_ok() {
                place[index] = Place::Source;
            }
        }
    };
    for (count, step) in steps.iter().enumerate() {
        let group = moves[step.index].group;
        if failed_groups.contains(&group) {
            continue;
        }
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        match rename(&step.from, &step.to) {
            Ok(()) => {
                place[step.index] = if step.to == moves[step.index].to {
                    Place::Target
                } else {
                    Place::Temp(step.to.clone())
                };
                done_order.push(step.index);
            }
            Err(error) => {
                failed.push((moves[step.index].from.clone(), error.to_string()));
                failed_groups.insert(group);
                rollback(group, &mut place, &done_order);
            }
        }
        progress(count + 1, steps.len());
    }
    // A cancelled run leaves no group half renamed and no file parked under a temp name.
    let unfinished: HashSet<usize> = moves
        .iter()
        .enumerate()
        .filter(|(index, _)| place[*index] != Place::Target)
        .map(|(_, m)| m.group)
        .collect();
    for group in unfinished {
        if !failed_groups.contains(&group) {
            rollback(group, &mut place, &done_order);
        }
    }
    let mut entries = Vec::new();
    let mut moved = Vec::new();
    for (index, m) in moves.iter().enumerate() {
        let via = match &place[index] {
            Place::Source => continue,
            Place::Target => {
                moved.push((m.from.clone(), m.to.clone()));
                None
            }
            Place::Temp(temp) => Some(temp.clone()),
        };
        entries.push(Entry {
            from: m.from.clone(),
            to: m.to.clone(),
            via,
            size: m.size,
            modified: m.modified,
        });
    }
    Run {
        entries,
        moved,
        failed,
        cancelled,
    }
}
fn finish(
    mut history: Vec<Batch>,
    path: Option<&Path>,
    mut batch: Batch,
    result: Run,
    groups: usize,
    failed_groups: usize,
) -> Outcome {
    batch.state = State::Done;
    batch.failed = u32::try_from(failed_groups.min(groups)).unwrap_or(u32::MAX);
    batch.entries = result.entries;
    history::push(&mut history, batch);
    let journal_error = path
        .and_then(|path| write_history(path, &history).err())
        .map(|error| error.to_string());
    Outcome {
        history,
        moved: result.moved,
        failed: result.failed,
        cancelled: result.cancelled,
        journal_error,
        ..Outcome::default()
    }
}

/// Renames `moves`. Nothing is touched when any source changed or any target appeared
/// since the preview, or when the journal cannot be written.
pub(super) fn apply(
    moves: Vec<Planned>,
    mut history: Vec<Batch>,
    path: Option<&Path>,
    (id, at): (u64, i64),
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Outcome {
    let sources: HashSet<String> = moves.iter().map(|m| lower(&m.from)).collect();
    let stale = moves.iter().any(|m| {
        fingerprint(&m.from) != Some((m.size, m.modified))
            || (exists(&m.to) && !sources.contains(&lower(&m.to)))
    });
    if stale {
        return Outcome {
            history,
            stale: true,
            ..Outcome::default()
        };
    }
    let steps = steps_for(&moves, id);
    let pending = Batch {
        id,
        at,
        kind: Kind::Rename,
        state: State::Pending,
        failed: 0,
        entries: journal(&moves, &steps),
    };
    history::push(&mut history, pending.clone());
    if let Some(path) = path
        && let Err(error) = write_history(path, &history)
    {
        history.retain(|batch| batch.id != id);
        return Outcome {
            history,
            journal_error: Some(error.to_string()),
            ..Outcome::default()
        };
    }
    let groups: HashSet<usize> = moves.iter().map(|m| m.group).collect();
    let result = run(&moves, &steps, cancel, progress);
    let failed_groups = result.failed.len();
    finish(history, path, pending, result, groups.len(), failed_groups)
}

/// Moves a batch back, skipping files changed since and names taken since.
pub(super) fn undo(
    target: u64,
    mut history: Vec<Batch>,
    path: Option<&Path>,
    (id, at): (u64, i64),
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Outcome {
    let Some(original) = history
        .iter()
        .find(|batch| batch.id == target && batch.undoable())
    else {
        return Outcome {
            history,
            ..Outcome::default()
        };
    };
    let plan = history::undo(original, fingerprint, exists);
    let skipped = plan.skipped.len();
    let moves: Vec<Planned> = plan
        .moves
        .iter()
        .zip(&plan.entries)
        .enumerate()
        .map(|(group, (m, &entry))| Planned {
            group,
            from: m.from.clone(),
            to: m.to.clone(),
            size: original.entries[entry].size,
            modified: original.entries[entry].modified,
        })
        .collect();
    let steps = steps_for(&moves, id);
    let pending = Batch {
        id,
        at,
        kind: Kind::Undo,
        state: State::Pending,
        failed: 0,
        entries: journal(&moves, &steps),
    };
    history::push(&mut history, pending.clone());
    if let Some(path) = path
        && let Err(error) = write_history(path, &history)
    {
        history.retain(|batch| batch.id != id);
        return Outcome {
            history,
            journal_error: Some(error.to_string()),
            ..Outcome::default()
        };
    }
    let result = run(&moves, &steps, cancel, progress);
    if !result.moved.is_empty()
        && let Some(batch) = history.iter_mut().find(|batch| batch.id == target)
    {
        batch.state = State::Undone;
    }
    let failed = result.failed.len() + skipped;
    let mut outcome = finish(
        history,
        path,
        pending,
        result,
        moves.len() + skipped,
        failed,
    );
    outcome.skipped = skipped;
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;
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
            (1, 1),
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
        let saved = load(&history_path);
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].state, State::Done);
        assert!(saved[0].entries.iter().all(|entry| entry.via.is_none()));
        let undone = undo(
            1,
            saved,
            Some(&history_path),
            (2, 2),
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
        let saved = load(&history_path);
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
        let outcome = apply(moves, Vec::new(), None, (1, 1), &cancel, &mut |_, _| {});
        assert!(outcome.stale);
        assert!(outcome.history.is_empty());
        assert_eq!(read(&dir, "z.jpg"), "Z");
        let mut edited = vec![planned(&dir, 0, "a.jpg", "y.jpg")];
        edited[0].size += 1;
        assert!(apply(edited, Vec::new(), None, (1, 1), &cancel, &mut |_, _| {}).stale);
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
        let outcome = apply(moves, Vec::new(), None, (1, 1), &cancel, &mut |_, _| {});
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
        let outcome = apply(moves, Vec::new(), None, (1, 1), &cancel, &mut |_, _| {});
        std::fs::write(dir.join("y.jpg"), "edited and longer").unwrap();
        let undone = undo(1, outcome.history, None, (2, 2), &cancel, &mut |_, _| {});
        assert_eq!(undone.skipped, 1);
        assert_eq!(names(&dir), ["a.jpg", "y.jpg"]);
        std::fs::remove_dir_all(&dir).unwrap();
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
            (1, 1),
            &cancel,
            &mut |_, _| {},
        );
        assert!(outcome.cancelled);
        assert!(outcome.history[0].entries.is_empty());
        let (size, modified) = fingerprint(&dir.join("a.jpg")).unwrap();
        std::fs::rename(dir.join("a.jpg"), dir.join("c.jpg")).unwrap();
        let pending = Batch {
            id: 9,
            at: 1,
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
                    modified: 1,
                },
            ],
        };
        write_history(&history_path, &[pending]).unwrap();
        let loaded = load(&history_path);
        assert_eq!(loaded[0].state, State::Interrupted);
        assert_eq!(loaded[0].entries.len(), 1);
        assert_eq!(load(&history_path), loaded);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
