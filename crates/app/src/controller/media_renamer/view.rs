//! Render-ready views of the MediaRenamer session for a UI.
use super::{Busy, Drawer, Notice, SAMPLE_ENTRIES, Session, StateFilter, WorkKind, file_name};
use powerkit_core::media_renamer::{
    Kind, Preferences,
    exif::ExifTag,
    history::{Batch, Kind as BatchKind, State as BatchState},
    plan::{self, Probe, Row, State, Summary},
    template::TemplateError,
    time::{self as capture_time, TimeSource},
};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct RowView {
    pub group: usize,
    pub kind: Kind,
    pub selected: bool,
    pub selectable: bool,
    pub current: String,
    pub folder: String,
    pub members: usize,
    pub new_name: String,
    pub time: String,
    pub source: Option<TimeSource>,
    pub state: State,
    pub collided: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeView {
    Pending,
    Read,
    OnlineOnly,
    Unreadable,
    CodecMissing(&'static str),
}
#[derive(Clone, Debug, PartialEq)]
pub struct MemberView {
    pub kind: Kind,
    pub name: String,
    pub new_name: String,
    pub primary: bool,
    pub companion: bool,
    pub probe: ProbeView,
}
#[derive(Clone, Debug, PartialEq)]
pub enum MetadataView {
    Reading,
    /// All values found; empty when the file carries no metadata.
    Tags(Vec<ExifTag>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct MetadataFileView {
    pub name: String,
    pub metadata: MetadataView,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DetailsView {
    pub kind: Kind,
    pub title: String,
    pub folder: String,
    pub members: Vec<MemberView>,
    pub metadata: Vec<MetadataFileView>,
    pub time: String,
    pub offset: Option<String>,
    pub source: Option<TimeSource>,
    pub make: String,
    pub model: String,
    pub state: State,
    pub collided: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BatchView {
    pub id: u64,
    pub at: String,
    pub kind: BatchKind,
    pub state: BatchState,
    pub files: usize,
    pub failed: u32,
    pub undoable: bool,
    pub sample: Vec<(String, String)>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub enum DrawerView {
    #[default]
    Hidden,
    Details(Box<DetailsView>),
    History(Vec<BatchView>),
    Settings,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub template: String,
    pub template_error: Option<TemplateError>,
    pub mtime_fallback: bool,
    pub recurse: bool,
    pub default_template: String,
    pub default_recurse: bool,
    pub default_mtime_fallback: bool,
    pub lowercase_ext: bool,
    pub filter_text: String,
    pub state_filter: StateFilter,
    pub filter_counts: [usize; 6],
    pub summary: Summary,
    /// Shared with the virtualized preview list, which builds rows after `view` returns.
    pub rows: Rc<[RowView]>,
    pub visible: usize,
    pub all_visible_selected: bool,
    pub any_selectable: bool,
    pub busy: Busy,
    pub queued: usize,
    pub notice: Option<Notice>,
    pub drawer: DrawerView,
    pub can_apply: bool,
    pub skipped: usize,
    pub batch_capped: bool,
    pub history_count: usize,
}

impl Session {
    fn row_view(&self, row: &Row) -> RowView {
        let group = &self.groups[row.group];
        let primary = &group.members[group.primary()];
        let folder = file_name(&group.dir);
        let folder = if folder.is_empty() {
            group.dir.display().to_string()
        } else {
            folder
        };
        RowView {
            group: row.group,
            kind: primary.kind,
            selected: row.selected,
            selectable: row.state == State::Ready,
            current: file_name(&primary.path),
            folder,
            members: group.members.len(),
            new_name: row
                .targets
                .get(group.primary())
                .map(|path| file_name(path))
                .unwrap_or_default(),
            time: row.capture.map(|c| c.time.display()).unwrap_or_default(),
            source: row.capture.map(|c| c.source),
            state: row.state,
            collided: row.collided,
        }
    }
    fn details(&self, key: &(String, String)) -> Option<DetailsView> {
        let (index, group) = self
            .groups
            .iter()
            .enumerate()
            .find(|(_, group)| group.key() == *key)?;
        let row = self.rows.iter().find(|row| row.group == index)?;
        let primary = group.primary();
        let metadata = match &group.members[primary].probe {
            Probe::Read(metadata) => Some(metadata),
            _ => None,
        };
        Some(DetailsView {
            kind: group.members[primary].kind,
            title: file_name(&group.members[primary].path),
            folder: group.dir.display().to_string(),
            members: group
                .members
                .iter()
                .enumerate()
                .map(|(m, member)| MemberView {
                    kind: member.kind,
                    name: file_name(&member.path),
                    new_name: row.targets.get(m).map(|t| file_name(t)).unwrap_or_default(),
                    primary: m == primary,
                    companion: member.companion,
                    probe: match &member.probe {
                        Probe::Pending => ProbeView::Pending,
                        Probe::Read(_) => ProbeView::Read,
                        Probe::OnlineOnly => ProbeView::OnlineOnly,
                        Probe::Unreadable => ProbeView::Unreadable,
                        Probe::CodecMissing(name) => ProbeView::CodecMissing(name),
                    },
                })
                .collect(),
            metadata: self.metadata_view(key),
            time: row.capture.map(|c| c.time.display()).unwrap_or_default(),
            offset: row
                .capture
                .and_then(|c| c.offset)
                .map(capture_time::format_offset),
            source: row.capture.map(|c| c.source),
            make: metadata.and_then(|m| m.make.clone()).unwrap_or_default(),
            model: metadata.and_then(|m| m.model.clone()).unwrap_or_default(),
            state: row.state,
            collided: row.collided,
        })
    }
    fn metadata_view(&self, key: &(String, String)) -> Vec<MetadataFileView> {
        let Some(metadata) = self.metadata.as_ref().filter(|m| m.key == *key) else {
            return Vec::new();
        };
        metadata
            .files
            .iter()
            .map(|file| MetadataFileView {
                name: file.name.clone(),
                metadata: match &file.tags {
                    Some(tags) => MetadataView::Tags(tags.clone()),
                    // A worker stopped early leaves a file unread; it has nothing to show.
                    None if metadata.job.is_none() => MetadataView::Tags(Vec::new()),
                    None => MetadataView::Reading,
                },
            })
            .collect()
    }
    fn batch_view(&self, batch: &Batch) -> BatchView {
        BatchView {
            id: batch.id,
            at: self.files.local_stamp(batch.at),
            kind: batch.kind,
            state: batch.state,
            files: batch.entries.len(),
            failed: batch.failed,
            undoable: batch.undoable(),
            sample: batch
                .entries
                .iter()
                .take(SAMPLE_ENTRIES)
                .map(|entry| (file_name(&entry.from), file_name(&entry.to)))
                .collect(),
        }
    }
    pub fn snapshot(&self, prefs: &Preferences) -> Snapshot {
        let summary = plan::summary(&self.groups, &self.rows);
        let mut filter_counts = [0usize; 6];
        for row in &self.rows {
            for (slot, filter) in filter_counts.iter_mut().zip(StateFilter::ALL) {
                if filter.matches(row) {
                    *slot += 1;
                }
            }
        }
        let mut visible = 0;
        let mut all_selected = true;
        let mut any_selectable = false;
        let mut rows = Vec::new();
        for row in self.visible() {
            visible += 1;
            if row.state == State::Ready {
                any_selectable = true;
                all_selected &= row.selected;
            }
            rows.push(self.row_view(row));
        }
        let busy = match &self.work {
            Some((WorkKind::Apply, _)) => Busy::Applying {
                done: self.work_progress.0,
                total: self.work_progress.1,
            },
            Some((WorkKind::Undo, _)) => Busy::Undoing {
                done: self.work_progress.0,
                total: self.work_progress.1,
            },
            Some((WorkKind::Load, _)) => Busy::Loading,
            None if self.scan.is_some() => Busy::Scanning {
                done: self.scan_progress.0,
                total: self.scan_progress.1,
            },
            None => Busy::Idle,
        };
        let drawer = match &self.drawer {
            Drawer::Hidden => DrawerView::Hidden,
            Drawer::Details(key) => self.details(key).map_or(DrawerView::Hidden, |view| {
                DrawerView::Details(Box::new(view))
            }),
            Drawer::History => DrawerView::History(
                self.history
                    .iter()
                    .map(|batch| self.batch_view(batch))
                    .collect(),
            ),
            Drawer::Settings => DrawerView::Settings,
        };
        Snapshot {
            template: self.template_text.clone(),
            template_error: self.template.as_ref().err().cloned(),
            mtime_fallback: self.mtime_fallback,
            recurse: self.recurse,
            default_template: prefs.template.clone(),
            default_recurse: prefs.recurse,
            default_mtime_fallback: prefs.mtime_fallback,
            lowercase_ext: prefs.lowercase_ext,
            filter_text: self.filter_text.clone(),
            state_filter: self.state_filter,
            filter_counts,
            can_apply: busy == Busy::Idle
                && self.template.is_ok()
                && summary.ready > 0
                && summary.pending == 0,
            summary,
            rows: rows.into(),
            visible,
            all_visible_selected: any_selectable && all_selected,
            any_selectable,
            busy,
            queued: self.queued.len(),
            notice: self.notice.clone(),
            drawer,
            skipped: self.skipped,
            batch_capped: self.batch_capped,
            history_count: self.history.len(),
        }
    }
}
