//! Applies and undoes rename batches on a worker thread: revalidate, journal
//! first, rename without replace, group rollback, then the final history entry.
//!
//! Every safety rule of a rename is decided here. The platform only gives the
//! primitives through [`MediaFiles`]: a rename that never replaces, a file's
//! fingerprint, and an all-or-nothing write of the history file.
use super::{Loaded, Outcome, Planned, lower};
use crate::ports::MediaFiles;
use powerkit_core::media_renamer::{
    history::{self, Batch, Entry, Kind, State},
    plan::{self, Move},
};
use powerkit_core::primitives::UtcTicks;
use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub fn write_history(files: &dyn MediaFiles, path: &Path, batches: &[Batch]) -> io::Result<()> {
    files.replace_text(path, &history::serialize(batches))
}

/// Reads history and turns batches left `pending` by a crash into `interrupted` ones.
pub fn load(files: &dyn MediaFiles, path: &Path) -> Loaded {
    let unreadable = Loaded {
        batches: Vec::new(),
        intact: false,
    };
    let text = match files.read_text(path, history::MAX_BYTES as u64) {
        Ok(text) => text.unwrap_or_default(),
        Err(_) => return unreadable,
    };
    if !history::is_history(&text) {
        return unreadable;
    }
    let mut batches = history::parse(&text);
    let mut changed = false;
    for batch in &mut batches {
        if batch.state == State::Pending {
            *batch = history::reconcile(batch.clone(), |path| files.fingerprint(path));
            changed = true;
        }
    }
    if changed {
        let _ = write_history(files, path, &batches);
    }
    Loaded {
        batches,
        intact: true,
    }
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
fn temp_name(files: &dyn MediaFiles, dir: &Path, batch: u64, index: usize) -> PathBuf {
    let mut attempt = 0;
    loop {
        let candidate = dir.join(format!(".powerkit-rename-{batch}-{index}-{attempt}.tmp"));
        if !files.exists(&candidate) {
            return candidate;
        }
        attempt += 1;
    }
}
fn steps_for(files: &dyn MediaFiles, moves: &[Planned], batch: u64) -> Vec<plan::Step> {
    let pairs: Vec<Move> = moves
        .iter()
        .map(|m| Move {
            from: m.from.clone(),
            to: m.to.clone(),
        })
        .collect();
    plan::order(&pairs, |index| {
        let dir = moves[index].from.parent().unwrap_or(Path::new(""));
        temp_name(files, dir, batch, index)
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
    files: &dyn MediaFiles,
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
            if files.rename(&current, &moves[index].from).is_ok() {
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
        match files.rename(&step.from, &step.to) {
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
    files: &dyn MediaFiles,
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
    // A finished batch never outgrows its journal entry, so it always fits the budget.
    history::push(&mut history, batch);
    let journal_error = path
        .and_then(|path| write_history(files, path, &history).err())
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

/// Records `pending` in the history and on disk before anything is renamed.
/// `Err` is the outcome of a batch that must not run.
fn open_journal(
    files: &dyn MediaFiles,
    mut history: Vec<Batch>,
    path: Option<&Path>,
    pending: &Batch,
) -> Result<Vec<Batch>, Outcome> {
    if !history::push(&mut history, pending.clone()) {
        return Err(Outcome {
            history,
            history_full: true,
            ..Outcome::default()
        });
    }
    if let Some(path) = path
        && let Err(error) = write_history(files, path, &history)
    {
        history.retain(|batch| batch.id != pending.id);
        return Err(Outcome {
            history,
            journal_error: Some(error.to_string()),
            ..Outcome::default()
        });
    }
    Ok(history)
}

/// Renames `moves`. Nothing is touched when any source changed or any target appeared
/// since the preview, or when the journal cannot be written.
pub fn apply(
    files: &dyn MediaFiles,
    moves: Vec<Planned>,
    history: Vec<Batch>,
    path: Option<&Path>,
    (id, at): (u64, UtcTicks),
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Outcome {
    let sources: HashSet<String> = moves.iter().map(|m| lower(&m.from)).collect();
    let stale = moves.iter().any(|m| {
        files.fingerprint(&m.from) != Some((m.size, m.modified))
            || (files.exists(&m.to) && !sources.contains(&lower(&m.to)))
    });
    if stale {
        return Outcome {
            history,
            stale: true,
            ..Outcome::default()
        };
    }
    let steps = steps_for(files, &moves, id);
    let pending = Batch {
        id,
        at,
        kind: Kind::Rename,
        state: State::Pending,
        failed: 0,
        entries: journal(&moves, &steps),
    };
    let history = match open_journal(files, history, path, &pending) {
        Ok(history) => history,
        Err(outcome) => return outcome,
    };
    let groups: HashSet<usize> = moves.iter().map(|m| m.group).collect();
    let result = run(files, &moves, &steps, cancel, progress);
    let failed_groups = result.failed.len();
    finish(
        files,
        history,
        path,
        pending,
        result,
        groups.len(),
        failed_groups,
    )
}

/// Moves a batch back, skipping files changed since and names taken since.
pub fn undo(
    files: &dyn MediaFiles,
    target: u64,
    history: Vec<Batch>,
    path: Option<&Path>,
    (id, at): (u64, UtcTicks),
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
    let plan = history::undo(
        original,
        |path| files.fingerprint(path),
        |path| files.exists(path),
    );
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
    let steps = steps_for(files, &moves, id);
    let pending = Batch {
        id,
        at,
        kind: Kind::Undo,
        state: State::Pending,
        failed: 0,
        entries: journal(&moves, &steps),
    };
    let mut history = match open_journal(files, history, path, &pending) {
        Ok(history) => history,
        Err(outcome) => return outcome,
    };
    let result = run(files, &moves, &steps, cancel, progress);
    if !result.moved.is_empty()
        && let Some(batch) = history.iter_mut().find(|batch| batch.id == target)
    {
        batch.state = State::Undone;
    }
    let failed = result.failed.len() + skipped;
    let mut outcome = finish(
        files,
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
    use crate::testkit::FakeMedia;

    const NOW: (u64, UtcTicks) = (1, UtcTicks(1));
    const LATER: (u64, UtcTicks) = (2, UtcTicks(2));

    struct Folder {
        media: FakeMedia,
        dir: PathBuf,
        journal: PathBuf,
    }
    impl Folder {
        fn with(names: &[&str]) -> Self {
            let folder = Self {
                media: FakeMedia::default(),
                dir: PathBuf::from("fake-media"),
                journal: PathBuf::from("fake-data").join("history.txt"),
            };
            for name in names {
                folder.media.add(&folder.dir.join(name), None);
            }
            folder
        }
        fn planned(&self, group: usize, from: &str, to: &str) -> Planned {
            let from = self.dir.join(from);
            let (size, modified) = self.media.fingerprint(&from).unwrap();
            Planned {
                group,
                from,
                to: self.dir.join(to),
                size,
                modified,
            }
        }
        /// The file now called `name`, by the time it was created with.
        fn stamp(&self, name: &str) -> UtcTicks {
            self.media.fingerprint(&self.dir.join(name)).unwrap().1
        }
        fn names(&self) -> Vec<String> {
            self.media.names(&self.dir)
        }
        fn apply(&self, moves: Vec<Planned>, history: Vec<Batch>, id: (u64, UtcTicks)) -> Outcome {
            apply(
                &self.media,
                moves,
                history,
                Some(&self.journal),
                id,
                &AtomicBool::new(false),
                &mut |_, _| {},
            )
        }
        fn undo(&self, target: u64, history: Vec<Batch>) -> Outcome {
            undo(
                &self.media,
                target,
                history,
                Some(&self.journal),
                LATER,
                &AtomicBool::new(false),
                &mut |_, _| {},
            )
        }
        fn saved(&self) -> Vec<Batch> {
            load(&self.media, &self.journal).batches
        }
    }

    #[test]
    fn swaps_chains_and_undo_restore_every_name() {
        let folder = Folder::with(&["a.jpg", "b.jpg", "c.jpg"]);
        let (a, b, c) = (
            folder.stamp("a.jpg"),
            folder.stamp("b.jpg"),
            folder.stamp("c.jpg"),
        );
        let moves = vec![
            folder.planned(0, "a.jpg", "b.jpg"),
            folder.planned(1, "b.jpg", "a.jpg"),
            folder.planned(2, "c.jpg", "d.jpg"),
        ];
        let mut ticks = Vec::new();
        let outcome = apply(
            &folder.media,
            moves,
            Vec::new(),
            Some(&folder.journal),
            NOW,
            &AtomicBool::new(false),
            &mut |done, total| ticks.push((done, total)),
        );
        assert!(outcome.failed.is_empty() && !outcome.stale);
        assert_eq!(outcome.moved.len(), 3);
        assert_eq!(
            (
                folder.stamp("b.jpg"),
                folder.stamp("a.jpg"),
                folder.stamp("d.jpg")
            ),
            (a, b, c)
        );
        // The swap needs one extra step through a temporary name.
        assert_eq!(ticks.last(), Some(&(4, 4)));
        let saved = folder.saved();
        assert_eq!(saved, outcome.history);
        assert_eq!(saved[0].state, State::Done);
        assert!(saved[0].entries.iter().all(|entry| entry.via.is_none()));

        let undone = folder.undo(1, saved);
        assert!(undone.failed.is_empty());
        assert_eq!(undone.skipped, 0);
        assert_eq!(folder.names(), ["a.jpg", "b.jpg", "c.jpg"]);
        assert_eq!(
            (
                folder.stamp("a.jpg"),
                folder.stamp("b.jpg"),
                folder.stamp("c.jpg")
            ),
            (a, b, c)
        );
        let saved = folder.saved();
        assert_eq!(saved[0].kind, Kind::Undo);
        assert_eq!(saved[1].state, State::Undone);
        assert!(!saved[1].undoable());
    }

    #[test]
    fn stale_previews_touch_nothing() {
        let folder = Folder::with(&["a.jpg"]);
        let moves = vec![folder.planned(0, "a.jpg", "z.jpg")];
        folder.media.add(&folder.dir.join("z.jpg"), None);
        let outcome = folder.apply(moves, Vec::new(), NOW);
        assert!(outcome.stale && outcome.history.is_empty());

        let moves = vec![folder.planned(0, "a.jpg", "y.jpg")];
        folder.media.touch(&folder.dir.join("a.jpg"));
        assert!(folder.apply(moves, Vec::new(), NOW).stale);
        assert_eq!(folder.names(), ["a.jpg", "z.jpg"]);
        assert!(folder.saved().is_empty(), "no journal was written");
    }

    #[test]
    fn a_failed_member_rolls_its_group_back_and_the_batch_continues() {
        let folder = Folder::with(&["p.heic", "p.mov", "q.jpg"]);
        let moves = vec![
            folder.planned(0, "p.heic", "new.heic"),
            folder.planned(0, "p.mov", "new.mov"),
            folder.planned(1, "q.jpg", "other.jpg"),
        ];
        // A locked file passes revalidation but its rename fails.
        folder.media.lock(&folder.dir.join("p.mov"));
        let outcome = folder.apply(moves, Vec::new(), NOW);
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].0, folder.dir.join("p.mov"));
        assert_eq!(folder.names(), ["other.jpg", "p.heic", "p.mov"]);
        assert_eq!(outcome.history[0].entries.len(), 1);
        assert_eq!(outcome.history[0].failed, 1);
        assert_eq!(outcome.moved.len(), 1);
    }

    #[test]
    fn a_journal_that_cannot_be_written_stops_the_batch_before_any_rename() {
        let folder = Folder::with(&["a.jpg"]);
        let moves = vec![folder.planned(0, "a.jpg", "b.jpg")];
        folder.media.fail_writes(true);
        let outcome = folder.apply(moves, Vec::new(), NOW);
        assert!(outcome.journal_error.is_some());
        assert!(outcome.moved.is_empty() && outcome.history.is_empty());
        assert_eq!(folder.names(), ["a.jpg"]);

        // The same for an undo: the renamed files stay where they are.
        folder.media.fail_writes(false);
        let moves = vec![folder.planned(0, "a.jpg", "b.jpg")];
        let applied = folder.apply(moves, Vec::new(), NOW);
        folder.media.fail_writes(true);
        let undone = folder.undo(1, applied.history.clone());
        assert!(undone.journal_error.is_some() && undone.moved.is_empty());
        assert_eq!(undone.history, applied.history);
        assert_eq!(folder.names(), ["b.jpg"]);
    }

    #[test]
    fn a_journal_lost_after_the_renames_is_reported_with_the_renames_kept() {
        let folder = Folder::with(&["a.jpg"]);
        let moves = vec![folder.planned(0, "a.jpg", "b.jpg")];
        // The pending journal is written, the final one is not.
        folder.media.fail_writes_after(1);
        let outcome = folder.apply(moves, Vec::new(), NOW);
        assert!(outcome.journal_error.is_some());
        assert_eq!(outcome.moved.len(), 1);
        assert_eq!(folder.names(), ["b.jpg"]);
        // What is on disk is the pending journal; the next launch reconciles
        // it into an interrupted batch that still knows the rename.
        folder.media.fail_writes(false);
        let saved = folder.saved();
        assert_eq!(saved[0].state, State::Interrupted);
        assert_eq!(saved[0].entries.len(), 1);
    }

    #[test]
    fn a_cancelled_run_leaves_no_group_half_renamed() {
        let folder = Folder::with(&["p.heic", "p.mov", "q.jpg"]);
        let moves = vec![
            folder.planned(0, "p.heic", "new.heic"),
            folder.planned(0, "p.mov", "new.mov"),
            folder.planned(1, "q.jpg", "other.jpg"),
        ];
        let cancel = AtomicBool::new(false);
        let outcome = apply(
            &folder.media,
            moves,
            Vec::new(),
            Some(&folder.journal),
            NOW,
            &cancel,
            // Cancel arrives after the first member of the pair was renamed.
            &mut |done, _| cancel.store(done == 1, Ordering::Relaxed),
        );
        assert!(outcome.cancelled);
        assert!(outcome.moved.is_empty());
        assert_eq!(folder.names(), ["p.heic", "p.mov", "q.jpg"]);
        assert!(outcome.history[0].entries.is_empty());
        assert_eq!(folder.saved()[0].state, State::Done);
    }

    #[test]
    fn undo_skips_files_edited_since_and_names_taken_since() {
        let folder = Folder::with(&["a.jpg", "b.jpg", "c.jpg"]);
        let moves = vec![
            folder.planned(0, "a.jpg", "x.jpg"),
            folder.planned(1, "b.jpg", "y.jpg"),
            folder.planned(2, "c.jpg", "z.jpg"),
        ];
        let applied = folder.apply(moves, Vec::new(), NOW);
        folder.media.touch(&folder.dir.join("y.jpg"));
        folder.media.add(&folder.dir.join("c.jpg"), None);
        let undone = folder.undo(1, applied.history);
        assert_eq!(undone.skipped, 2);
        assert_eq!(undone.moved.len(), 1);
        assert_eq!(folder.names(), ["a.jpg", "c.jpg", "y.jpg", "z.jpg"]);
        assert_eq!(undone.history[0].failed, 2);
        // A batch that was put back is not offered again.
        assert!(folder.undo(1, undone.history.clone()).moved.is_empty());
    }

    #[test]
    fn foreign_history_files_are_reported_and_left_alone() {
        let folder = Folder::with(&[]);
        folder.media.make_history_foreign();
        let loaded = load(&folder.media, &folder.journal);
        assert!(!loaded.intact && loaded.batches.is_empty());
        assert_eq!(folder.media.writes(), 0);
    }

    #[test]
    fn a_pending_journal_is_reconciled_once_at_load() {
        let folder = Folder::with(&["c.jpg"]);
        let (size, modified) = folder.media.fingerprint(&folder.dir.join("c.jpg")).unwrap();
        let pending = Batch {
            id: 9,
            at: UtcTicks(1),
            kind: Kind::Rename,
            state: State::Pending,
            failed: 0,
            entries: vec![
                // This rename reached the disk before the crash.
                Entry {
                    from: folder.dir.join("a.jpg"),
                    to: folder.dir.join("c.jpg"),
                    via: None,
                    size,
                    modified,
                },
                // This one did not.
                Entry {
                    from: folder.dir.join("m.jpg"),
                    to: folder.dir.join("n.jpg"),
                    via: None,
                    size: 1,
                    modified: UtcTicks(1),
                },
            ],
        };
        write_history(&folder.media, &folder.journal, &[pending]).unwrap();
        let loaded = load(&folder.media, &folder.journal);
        assert!(loaded.intact);
        assert_eq!(loaded.batches[0].state, State::Interrupted);
        assert_eq!(loaded.batches[0].entries.len(), 1);
        assert_eq!(loaded.batches[0].failed, 1);
        let writes = folder.media.writes();
        assert_eq!(load(&folder.media, &folder.journal), loaded);
        assert_eq!(folder.media.writes(), writes, "nothing left to reconcile");
    }

    #[test]
    fn a_missing_history_file_is_an_empty_intact_history() {
        let folder = Folder::with(&[]);
        let loaded = load(&folder.media, &folder.journal);
        assert!(loaded.intact && loaded.batches.is_empty());
    }
}
