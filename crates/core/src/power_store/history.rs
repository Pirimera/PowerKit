//! Bounded operation history, newest first.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryOutcome {
    Succeeded,
    Cancelled,
    Failed,
}

impl HistoryOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "ok",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "ok" => Self::Succeeded,
            "cancelled" => Self::Cancelled,
            "failed" => Self::Failed,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    /// Local wall-clock text supplied by the platform layer.
    pub time: String,
    pub kind: OperationKind,
    pub key: PackageKey,
    pub name: String,
    pub outcome: HistoryOutcome,
    pub log: String,
}

/// Recent operations, oldest first, bounded by count and per-entry log size.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    entries: VecDeque<HistoryEntry>,
}

impl History {
    pub fn push(&mut self, mut entry: HistoryEntry) {
        truncate_utf8(&mut entry.log, MAX_HISTORY_LOG_BYTES);
        self.entries.push_back(entry);
        while self.entries.len() > MAX_HISTORY_OPERATIONS {
            self.entries.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn newest_first(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter().rev()
    }

    pub fn serialize(&self) -> String {
        let mut text = String::from("powerkit-history=1\n");
        for entry in &self.entries {
            let Some(key) = encode_package_key(&entry.key) else {
                continue;
            };
            let fields = [
                entry.time.as_str(),
                entry.kind.as_str(),
                key.as_str(),
                entry.name.as_str(),
                entry.outcome.as_str(),
                entry.log.as_str(),
            ];
            let line: Vec<String> = fields.iter().map(|field| escape_field(field)).collect();
            text.push_str(&line.join("\t"));
            text.push('\n');
        }
        text
    }

    /// Malformed lines are skipped so one bad entry never hides the rest.
    pub fn parse(text: &str) -> Self {
        let mut history = Self::default();
        let mut lines = text.lines();
        if lines.next() != Some("powerkit-history=1") {
            return history;
        }
        for line in lines {
            let fields: Vec<String> = line.split('\t').map(unescape_field).collect();
            let [time, kind, key, name, outcome, log] = fields.as_slice() else {
                continue;
            };
            let (Some(kind), Some(key), Some(outcome)) = (
                OperationKind::parse(kind),
                parse_package_key(key),
                HistoryOutcome::parse(outcome),
            ) else {
                continue;
            };
            history.push(HistoryEntry {
                time: time.clone(),
                kind,
                key,
                name: name.clone(),
                outcome,
                log: log.clone(),
            });
        }
        history
    }
}

fn truncate_utf8(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}
