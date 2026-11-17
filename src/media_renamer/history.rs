//! Persistent rename history: journal format, pending-batch recovery and undo planning.
use super::plan::Move;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub const HEADER: &str = "powerkit-rename-history=1";
pub const FILE_NAME: &str = "media-renamer-history.txt";
pub const MAX_BATCHES: usize = 50;
pub const MAX_ENTRIES: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Journal written, renames in progress (or PowerKit stopped mid-batch).
    Pending,
    Done,
    /// A pending batch found on launch, reduced to the renames that happened.
    Interrupted,
    Undone,
}
impl State {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Done => "done",
            Self::Interrupted => "interrupted",
            Self::Undone => "undone",
        }
    }
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "pending" => Self::Pending,
            "done" => Self::Done,
            "interrupted" => Self::Interrupted,
            "undone" => Self::Undone,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rename,
    Undo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub from: PathBuf,
    pub to: PathBuf,
    /// Temporary name used to break a swap; the file may sit here after a crash.
    pub via: Option<PathBuf>,
    pub size: u64,
    pub modified: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub id: u64,
    /// UTC FILETIME ticks.
    pub at: i64,
    pub kind: Kind,
    pub state: State,
    pub failed: u32,
    pub entries: Vec<Entry>,
}
impl Batch {
    pub fn undoable(&self) -> bool {
        self.kind == Kind::Rename
            && matches!(self.state, State::Done | State::Interrupted)
            && !self.entries.is_empty()
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}
fn unescape(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        out.push(match chars.next()? {
            '\\' => '\\',
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            _ => return None,
        });
    }
    Some(out)
}
fn path_text(path: &Path) -> String {
    escape(&path.to_string_lossy())
}

/// Newest batch first.
pub fn serialize(batches: &[Batch]) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    for batch in batches.iter().take(MAX_BATCHES) {
        out.push_str(&format!(
            "batch\t{}\t{}\t{}\t{}\t{}\n",
            batch.id,
            batch.at,
            match batch.kind {
                Kind::Rename => "rename",
                Kind::Undo => "undo",
            },
            batch.state.as_str(),
            batch.failed
        ));
        for entry in batch.entries.iter().take(MAX_ENTRIES) {
            out.push_str(&format!(
                "entry\t{}\t{}\t{}\t{}\t{}\n",
                path_text(&entry.from),
                path_text(&entry.to),
                entry.via.as_deref().map(path_text).unwrap_or_default(),
                entry.size,
                entry.modified
            ));
        }
    }
    out
}
fn parse_batch(fields: &[&str]) -> Option<Batch> {
    let [id, at, kind, state, failed] = fields else {
        return None;
    };
    Some(Batch {
        id: id.parse().ok()?,
        at: at.parse().ok()?,
        kind: match *kind {
            "rename" => Kind::Rename,
            "undo" => Kind::Undo,
            _ => return None,
        },
        state: State::parse(state)?,
        failed: failed.parse().ok()?,
        entries: Vec::new(),
    })
}
fn parse_entry(fields: &[&str]) -> Option<Entry> {
    let [from, to, via, size, modified] = fields else {
        return None;
    };
    let from = unescape(from)?;
    let to = unescape(to)?;
    if from.is_empty() || to.is_empty() {
        return None;
    }
    Some(Entry {
        from: from.into(),
        to: to.into(),
        via: (!via.is_empty())
            .then(|| unescape(via).map(PathBuf::from))
            .flatten(),
        size: size.parse().ok()?,
        modified: modified.parse().ok()?,
    })
}
/// Reads history, skipping damaged lines. A foreign or missing header means no history.
pub fn parse(text: &str) -> Vec<Batch> {
    let mut lines = text.lines();
    if lines.next().map(|line| line.trim_start_matches('\u{feff}')) != Some(HEADER) {
        return Vec::new();
    }
    let mut batches: Vec<Batch> = Vec::new();
    let mut ids = HashSet::new();
    let mut accepting = false;
    for line in lines {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.first().copied() {
            Some("batch") => {
                accepting = false;
                if batches.len() >= MAX_BATCHES {
                    continue;
                }
                if let Some(batch) = parse_batch(&fields[1..])
                    && ids.insert(batch.id)
                {
                    batches.push(batch);
                    accepting = true;
                }
            }
            Some("entry") if accepting => {
                let Some(batch) = batches.last_mut() else {
                    continue;
                };
                if batch.entries.len() < MAX_ENTRIES
                    && let Some(entry) = parse_entry(&fields[1..])
                {
                    batch.entries.push(entry);
                }
            }
            _ => {}
        }
    }
    batches
}
/// Adds a batch at the front and drops the oldest beyond the cap.
pub fn push(batches: &mut Vec<Batch>, batch: Batch) {
    batches.retain(|existing| existing.id != batch.id);
    batches.insert(0, batch);
    batches.truncate(MAX_BATCHES);
}

/// Size and last-write ticks of an existing file.
pub type Fingerprint = (u64, i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Where {
    Target,
    Via,
}
/// Where an entry's file currently is, if it is still the file that was renamed.
pub fn locate(entry: &Entry, look: &mut impl FnMut(&Path) -> Option<Fingerprint>) -> Option<Where> {
    let expected = (entry.size, entry.modified);
    if look(&entry.to) == Some(expected) {
        return Some(Where::Target);
    }
    if let Some(via) = &entry.via
        && look(via) == Some(expected)
    {
        return Some(Where::Via);
    }
    None
}
/// A pending batch found on launch keeps only the renames that reached disk.
pub fn reconcile(mut batch: Batch, mut look: impl FnMut(&Path) -> Option<Fingerprint>) -> Batch {
    let before = batch.entries.len();
    batch
        .entries
        .retain(|entry| locate(entry, &mut look).is_some());
    batch.state = State::Interrupted;
    let missing = u32::try_from(before - batch.entries.len()).unwrap_or(u32::MAX);
    batch.failed = batch.failed.max(missing);
    batch
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    /// The renamed file is gone or was replaced or edited since.
    Changed,
    /// Something else now uses the original name.
    OriginalTaken,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UndoPlan {
    pub moves: Vec<Move>,
    /// Index into the batch entries of each move.
    pub entries: Vec<usize>,
    pub skipped: Vec<(PathBuf, Skip)>,
}
/// Moves that put a batch back, newest rename first. Never plans an overwrite.
pub fn undo(
    batch: &Batch,
    mut look: impl FnMut(&Path) -> Option<Fingerprint>,
    mut exists: impl FnMut(&Path) -> bool,
) -> UndoPlan {
    let mut plan = UndoPlan::default();
    let mut located = Vec::new();
    for (index, entry) in batch.entries.iter().enumerate().rev() {
        match locate(entry, &mut look) {
            Some(place) => located.push((index, place)),
            None => plan.skipped.push((entry.to.clone(), Skip::Changed)),
        }
    }
    let lower = |path: &Path| path.to_string_lossy().to_lowercase();
    let vacating: HashSet<String> = located
        .iter()
        .map(|&(index, place)| {
            let entry = &batch.entries[index];
            match place {
                Where::Target => lower(&entry.to),
                Where::Via => lower(entry.via.as_deref().unwrap_or(&entry.to)),
            }
        })
        .collect();
    let mut claimed = HashSet::new();
    for (index, place) in located {
        let entry = &batch.entries[index];
        let current = match place {
            Where::Target => entry.to.clone(),
            Where::Via => entry.via.clone().unwrap_or_else(|| entry.to.clone()),
        };
        let original = lower(&entry.from);
        let case_only = lower(&current) == original;
        let free = case_only || vacating.contains(&original) || !exists(&entry.from);
        if !free || !claimed.insert(original) {
            plan.skipped.push((current, Skip::OriginalTaken));
            continue;
        }
        plan.moves.push(Move {
            from: current,
            to: entry.from.clone(),
        });
        plan.entries.push(index);
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    fn entry(from: &str, to: &str, size: u64) -> Entry {
        Entry {
            from: from.into(),
            to: to.into(),
            via: None,
            size,
            modified: 7,
        }
    }
    fn batch(id: u64, entries: Vec<Entry>) -> Batch {
        Batch {
            id,
            at: 1,
            kind: Kind::Rename,
            state: State::Done,
            failed: 0,
            entries,
        }
    }
    #[test]
    fn history_round_trips_with_escapes() {
        let mut first = batch(1, vec![entry(r"C:\a\x\ty.jpg", r"C:\a\b.jpg", 3)]);
        first.entries[0].via = Some(r"C:\a\.t".into());
        let second = Batch {
            kind: Kind::Undo,
            state: State::Pending,
            failed: 2,
            ..batch(2, vec![entry("C:\\a\\n\\z", "C:\\a\\é.jpg", 9)])
        };
        let text = serialize(&[second.clone(), first.clone()]);
        assert_eq!(parse(&text), [second, first]);
    }
    #[test]
    fn damaged_lines_are_skipped() {
        let text = format!(
            "{HEADER}\nentry\ta\tb\t\t1\t1\nbatch\t5\t1\trename\tdone\t0\nentry\ta\tb\t\tx\t1\nentry\ta\\q\tb\t\t1\t1\nentry\ta\tb\t\t1\t1\nbatch\t5\t1\trename\tdone\t0\nentry\tc\td\t\t1\t1\nbatch\tbad\nnoise\n"
        );
        let batches = parse(&text);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].entries.len(), 1);
        assert!(parse("other=1\nbatch\t1\t1\trename\tdone\t0\n").is_empty());
    }
    #[test]
    fn push_caps_history() {
        let mut batches = Vec::new();
        for id in 0..60 {
            push(&mut batches, batch(id, Vec::new()));
        }
        assert_eq!(batches.len(), MAX_BATCHES);
        assert_eq!(batches[0].id, 59);
        assert_eq!(parse(&serialize(&batches)).len(), MAX_BATCHES);
    }
    #[test]
    fn pending_batches_keep_only_applied_renames() {
        let mut pending = batch(
            1,
            vec![entry("a", "A", 1), entry("b", "B", 2), entry("c", "C", 3)],
        );
        pending.entries[2].via = Some("T".into());
        pending.state = State::Pending;
        let disk: HashMap<&str, Fingerprint> = [("A", (1, 7)), ("b", (2, 7)), ("T", (3, 7))].into();
        let fixed = reconcile(pending, |path| disk.get(path.to_str().unwrap()).copied());
        assert_eq!(fixed.state, State::Interrupted);
        assert_eq!(fixed.entries.len(), 2);
        assert_eq!(fixed.failed, 1);
        assert!(fixed.undoable());
    }
    #[test]
    fn undo_skips_changed_files_and_taken_names() {
        let done = batch(
            1,
            vec![
                entry("a", "n1", 1),
                entry("b", "n2", 2),
                entry("c", "n3", 3),
                entry("d", "n4", 4),
            ],
        );
        let disk: HashMap<&str, Fingerprint> = [
            ("n1", (1, 7)),
            ("n2", (99, 7)),
            ("n3", (3, 7)),
            ("c", (5, 5)),
            ("n4", (4, 7)),
        ]
        .into();
        let plan = undo(
            &done,
            |path| disk.get(path.to_str().unwrap()).copied(),
            |path| disk.contains_key(path.to_str().unwrap()),
        );
        let moves: Vec<(&str, &str)> = plan
            .moves
            .iter()
            .map(|m| (m.from.to_str().unwrap(), m.to.to_str().unwrap()))
            .collect();
        assert_eq!(moves, [("n4", "d"), ("n1", "a")]);
        assert_eq!(plan.entries, [3, 0]);
        assert_eq!(
            plan.skipped,
            [
                (PathBuf::from("n2"), Skip::Changed),
                (PathBuf::from("n3"), Skip::OriginalTaken)
            ]
        );
    }
    #[test]
    fn undo_reverses_chains_and_case_only_renames() {
        let chain = batch(
            1,
            vec![entry("B", "C", 2), entry("A", "B", 1), entry("x", "X", 3)],
        );
        let disk: HashMap<&str, Fingerprint> = [("C", (2, 7)), ("B", (1, 7)), ("X", (3, 7))].into();
        let plan = undo(
            &chain,
            |path| disk.get(path.to_str().unwrap()).copied(),
            |path| {
                disk.keys()
                    .any(|key| key.eq_ignore_ascii_case(path.to_str().unwrap()))
            },
        );
        assert_eq!(plan.moves.len(), 3);
        assert!(plan.skipped.is_empty());
    }
}
