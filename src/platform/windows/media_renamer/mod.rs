//! MediaRenamer workspace owned by the UI thread. Scanning, probing, renaming and the
//! history load run on worker threads and report through channels; each worker posts
//! `WAKE` to the app window so the UI thread drains them promptly.
mod commit;
pub(super) mod handoff;
mod job;
pub(super) mod pick;
mod probe;
mod scan;
#[cfg(test)]
mod tests;
mod view;

pub(super) use view::{
    BatchView, DetailsView, DrawerView, MemberView, MetadataView, ProbeView, RowView, Snapshot,
};

use super::clock;
use crate::media_renamer::{
    Kind, MAX_BATCH, Preferences,
    exif::ExifTag,
    history::Batch,
    plan::{self, Group, Member, Probe, Row, State},
    template::{self, Template, TemplateError},
};
use job::Job;
use std::{
    collections::{HashMap, HashSet},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    thread::JoinHandle,
    time::Duration,
};

/// Win32 paths from the `\\?\` form start here: the classic limit is 260 units, and the
/// verbatim form is used a little earlier so 8.3 names of long folders still fit.
const LONG_PATH_UNITS: usize = 248;

/// NUL-terminated UTF-16 for a Win32 call. Long absolute paths get the `\\?\` form, so
/// renames and WIC reads work beyond MAX_PATH without relying on the app manifest.
pub(super) fn wide_path(path: &Path) -> Vec<u16> {
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    let mut out = if units.len() < LONG_PATH_UNITS
        || !path.is_absolute()
        || units.starts_with(&utf16(r"\\?\"))
    {
        units
    } else if let Some(rest) = units.strip_prefix(utf16(r"\\").as_slice()) {
        let mut out = utf16(r"\\?\UNC\");
        out.extend_from_slice(rest);
        out
    } else {
        let mut out = utf16(r"\\?\");
        out.extend(units);
        out
    };
    out.push(0);
    out
}
fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

pub(super) const WAKE: u32 = job::WAKE;
const PROBE_CHUNK: usize = 32;
const SAMPLE_ENTRIES: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum StateFilter {
    #[default]
    All,
    Ready,
    Unchanged,
    NoDate,
    Collision,
    Problem,
}
impl StateFilter {
    pub(super) const ALL: [Self; 6] = [
        Self::All,
        Self::Ready,
        Self::Unchanged,
        Self::NoDate,
        Self::Collision,
        Self::Problem,
    ];
    pub(super) fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }
    pub(super) fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
    pub(super) fn matches(self, row: &Row) -> bool {
        match self {
            Self::All => true,
            Self::Ready => row.state == State::Ready,
            Self::Unchanged => row.state == State::Unchanged,
            Self::NoDate => row.state == State::NoDate,
            Self::Collision => row.collided,
            Self::Problem => matches!(
                row.state,
                State::OnlineOnly | State::Unreadable | State::CodecMissing(_) | State::Conflict
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Action {
    AddFiles,
    AddFolder,
    Add(Vec<PathBuf>),
    Clear,
    Apply,
    /// Re-checks the listed files after a stale preview.
    Refresh,
    CancelWork,
    CancelScan,
    SetTemplate(String),
    SetMtimeFallback(bool),
    SetRecurse(bool),
    SetFilterText(String),
    SetStateFilter(StateFilter),
    ClearFilter,
    SetSelected(usize, bool),
    SetSelectAllVisible(bool),
    OpenDetails(usize),
    ToggleHistory,
    ToggleSettings,
    CloseDrawer,
    Undo(u64),
    ClearHistory,
    DismissNotice,
    SaveDefaultTemplate,
    SetDefaultRecurse(bool),
    SetDefaultMtimeFallback(bool),
    SetLowercaseExt(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Notice {
    Applied {
        renamed: usize,
        failures: Vec<String>,
        cancelled: bool,
    },
    Undone {
        restored: usize,
        skipped: usize,
        failures: Vec<String>,
    },
    Stale,
    JournalFailed(String),
    HistoryCleared,
    HistoryUnreadable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Busy {
    #[default]
    Idle,
    Scanning {
        done: usize,
        total: usize,
    },
    Applying {
        done: usize,
        total: usize,
    },
    Undoing {
        done: usize,
        total: usize,
    },
    Loading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrayStatus {
    Idle,
    Scanning,
    Ready(usize),
    Applying,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Drawer {
    #[default]
    Hidden,
    Details((String, String)),
    History,
    Settings,
}

/// One listed file to check again: its key, its path, and what the preview recorded.
struct Target {
    key: String,
    path: PathBuf,
    kind: Kind,
    known: (u64, i64),
    /// False for online-only files, which must not be read because reading downloads them.
    readable: bool,
}

/// Every metadata value of the files in the open Details drawer. It is read on a worker,
/// one file at a time, only while the drawer is open.
struct Metadata {
    key: (String, String),
    files: Vec<MetadataFile>,
    /// Running while any file is still being read; reports `(file index, its tags)`.
    job: Option<Job<(usize, Vec<ExifTag>)>>,
}
struct MetadataFile {
    name: String,
    /// `None` until the worker has read the file.
    tags: Option<Vec<ExifTag>>,
}

enum ScanEvent {
    Listed(scan::Listing),
    Probed {
        results: Vec<(String, Probe)>,
        done: usize,
        total: usize,
    },
    /// Folder contents read for a refresh, so collision checks match the disk again.
    Disk(HashSet<String>),
    Rechecked {
        results: Vec<(String, scan::Recheck)>,
        done: usize,
        total: usize,
    },
    Done,
}
enum WorkEvent {
    Progress(usize, usize),
    Applied(commit::Outcome),
    Undone(commit::Outcome),
    Loaded(commit::Loaded),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum WorkKind {
    Apply,
    Undo,
    Load,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(super) struct Session {
    hwnd: isize,
    groups: Vec<Group>,
    index: HashMap<String, (usize, usize)>,
    deselected: HashSet<(String, String)>,
    rows: Vec<Row>,
    disk: HashSet<String>,
    template_text: String,
    template: Result<Template, TemplateError>,
    last_valid: Template,
    mtime_fallback: bool,
    recurse: bool,
    lowercase_ext: bool,
    filter_text: String,
    state_filter: StateFilter,
    drawer: Drawer,
    metadata: Option<Metadata>,
    notice: Option<Notice>,
    skipped: usize,
    batch_capped: bool,
    queued: Vec<(Vec<PathBuf>, bool)>,
    scan: Option<Job<ScanEvent>>,
    scan_progress: (usize, usize),
    work: Option<(WorkKind, Job<WorkEvent>)>,
    work_progress: (usize, usize),
    history: Vec<Batch>,
    history_path: Option<PathBuf>,
    /// False when the history file exists but could not be read as history. Renames and
    /// undo would overwrite it, so they stay off until the file is looked at.
    history_intact: bool,
}
impl Default for Session {
    fn default() -> Self {
        Self::new(0, &Preferences::default())
    }
}
impl Session {
    pub(super) fn new(hwnd: isize, prefs: &Preferences) -> Self {
        let template = Template::parse(&prefs.template);
        let fallback = Template::parse(template::DEFAULT).expect("default template parses");
        Self {
            hwnd,
            groups: Vec::new(),
            index: HashMap::new(),
            deselected: HashSet::new(),
            rows: Vec::new(),
            disk: HashSet::new(),
            template_text: prefs.template.clone(),
            last_valid: template.clone().unwrap_or(fallback),
            template,
            mtime_fallback: prefs.mtime_fallback,
            recurse: prefs.recurse,
            lowercase_ext: prefs.lowercase_ext,
            filter_text: String::new(),
            state_filter: StateFilter::All,
            drawer: Drawer::Hidden,
            metadata: None,
            notice: None,
            skipped: 0,
            batch_capped: false,
            queued: Vec::new(),
            scan: None,
            scan_progress: (0, 0),
            work: None,
            work_progress: (0, 0),
            history: Vec::new(),
            history_path: None,
            history_intact: true,
        }
    }
    pub(super) fn template_text(&self) -> &str {
        &self.template_text
    }
    pub(super) fn set_lowercase_ext(&mut self, lowercase: bool) {
        if self.lowercase_ext != lowercase {
            self.lowercase_ext = lowercase;
            self.replan();
        }
    }
    pub(super) fn busy(&self) -> bool {
        self.work.is_some()
    }
    /// True when Explorer input would discard renames the user has not applied.
    pub(super) fn has_unapplied(&self) -> bool {
        self.rows
            .iter()
            .any(|row| row.selected && row.state == State::Ready)
            || self.scan.is_some()
    }
    pub(super) fn tray_status(&self) -> TrayStatus {
        if self
            .work
            .as_ref()
            .is_some_and(|(kind, _)| *kind != WorkKind::Load)
        {
            TrayStatus::Applying
        } else if self.scan.is_some() {
            TrayStatus::Scanning
        } else {
            match plan::summary(&self.groups, &self.rows).ready {
                0 => TrayStatus::Idle,
                ready => TrayStatus::Ready(ready),
            }
        }
    }

    pub(super) fn load_history(&mut self, path: PathBuf) {
        self.history_path = Some(path.clone());
        if self.work.is_some() {
            return;
        }
        let job = Job::spawn(self.hwnd, move |mut reporter, _| {
            reporter.send(WorkEvent::Loaded(commit::load(&path)), true);
        });
        self.work = Some((WorkKind::Load, job));
    }
    /// Adds files or folders; a scan already running queues the next one.
    pub(super) fn add(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty()
            || self
                .work
                .as_ref()
                .is_some_and(|(kind, _)| *kind != WorkKind::Load)
        {
            return;
        }
        if self.scan.is_some() {
            self.queued.push((paths, self.recurse));
            return;
        }
        self.start_scan(paths, self.recurse);
    }
    /// Explorer input replaces the workspace.
    pub(super) fn replace(&mut self, paths: Vec<PathBuf>) {
        if self
            .work
            .as_ref()
            .is_some_and(|(kind, _)| *kind != WorkKind::Load)
        {
            return;
        }
        self.clear();
        self.add(paths);
    }
    fn start_scan(&mut self, paths: Vec<PathBuf>, recurse: bool) {
        let known: HashSet<String> = self.index.keys().cloned().collect();
        let media = self
            .groups
            .iter()
            .flat_map(|g| &g.members)
            .filter(|m| m.kind.is_media())
            .count();
        let room = MAX_BATCH.saturating_sub(media);
        if room == 0 {
            self.batch_capped = true;
            return;
        }
        self.scan_progress = (0, 0);
        self.scan = Some(Job::spawn(self.hwnd, move |mut reporter, cancel| {
            let listing = scan::list(&paths, recurse, &known, room, cancel);
            let targets: Vec<(String, PathBuf, Kind)> = listing
                .members
                .iter()
                .filter(|m| m.probe == Probe::Pending)
                .map(|m| (scan::lower(&m.path), m.path.clone(), m.kind))
                .collect();
            reporter.send(ScanEvent::Listed(listing), true);
            let prober = probe::Prober::new();
            let total = targets.len();
            let mut results = Vec::with_capacity(PROBE_CHUNK);
            for (done, (key, path, kind)) in targets.into_iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                results.push((key, prober.probe(&path, kind)));
                if results.len() == PROBE_CHUNK {
                    let results = std::mem::take(&mut results);
                    reporter.send(
                        ScanEvent::Probed {
                            results,
                            done: done + 1,
                            total,
                        },
                        false,
                    );
                }
            }
            reporter.send(
                ScanEvent::Probed {
                    results,
                    done: total,
                    total,
                },
                false,
            );
            reporter.send(ScanEvent::Done, true);
        }));
    }
    pub(super) fn clear(&mut self) {
        if self
            .work
            .as_ref()
            .is_some_and(|(kind, _)| *kind != WorkKind::Load)
        {
            return;
        }
        if let Some(scan) = self.scan.take() {
            scan.stop();
        }
        self.queued.clear();
        self.groups.clear();
        self.index.clear();
        self.deselected.clear();
        self.rows.clear();
        self.disk.clear();
        self.skipped = 0;
        self.batch_capped = false;
        if matches!(self.drawer, Drawer::Details(_)) {
            self.set_drawer(Drawer::Hidden);
        }
        self.notice = None;
    }
    /// Switches the drawer; leaving the details drawer stops reading metadata.
    fn set_drawer(&mut self, drawer: Drawer) {
        self.stop_metadata();
        self.drawer = drawer;
    }
    fn stop_metadata(&mut self) {
        if let Some(Metadata { job: Some(job), .. }) = self.metadata.take() {
            job.stop();
        }
    }
    /// Reads every metadata value of the group's files on a worker. Sidecars carry no
    /// image metadata, and online-only files must not be read because that downloads them.
    fn start_metadata(&mut self, key: (String, String)) {
        self.stop_metadata();
        let Some(group) = self.groups.iter().find(|group| group.key() == key) else {
            return;
        };
        let mut files = Vec::new();
        let mut targets = Vec::new();
        for member in &group.members {
            if !member.kind.is_media() || member.probe == Probe::OnlineOnly {
                continue;
            }
            targets.push((files.len(), member.path.clone(), member.kind));
            files.push(MetadataFile {
                name: file_name(&member.path),
                tags: None,
            });
        }
        let job = (!targets.is_empty()).then(|| {
            Job::spawn(self.hwnd, move |mut reporter, cancel| {
                let prober = probe::Prober::new();
                for (index, path, kind) in targets {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    reporter.send((index, prober.dump(&path, kind)), true);
                }
            })
        });
        self.metadata = Some(Metadata { key, files, job });
    }
    /// Drains metadata results; returns whether the drawer changed.
    fn poll_metadata(&mut self) -> bool {
        let Some(metadata) = &mut self.metadata else {
            return false;
        };
        let Some(job) = &metadata.job else {
            return false;
        };
        // Read before draining: a worker that finishes between the two steps has already
        // queued its last result, which the drain below still picks up.
        let finished = job.thread.as_ref().is_none_or(JoinHandle::is_finished);
        let mut changed = false;
        while let Ok((index, tags)) = job.receiver.try_recv() {
            if let Some(file) = metadata.files.get_mut(index) {
                file.tags = Some(tags);
                changed = true;
            }
        }
        if finished {
            metadata.job = None;
            changed = true;
        }
        changed
    }
    /// Re-checks every listed file on disk. Changed files are read again, missing files
    /// leave the workspace and the preview is planned again; nothing is renamed here.
    pub(super) fn refresh(&mut self) {
        if self.work.is_some() || self.scan.is_some() {
            return;
        }
        let targets: Vec<Target> = self
            .groups
            .iter()
            .flat_map(|group| &group.members)
            .map(|member| Target {
                key: scan::lower(&member.path),
                path: member.path.clone(),
                kind: member.kind,
                known: (member.size, member.modified),
                readable: member.probe != Probe::OnlineOnly,
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        let dirs: Vec<PathBuf> = self
            .groups
            .iter()
            .map(|group| group.dir.clone())
            .collect::<HashSet<PathBuf>>()
            .into_iter()
            .collect();
        self.notice = None;
        self.scan_progress = (0, targets.len());
        self.scan = Some(Job::spawn(self.hwnd, move |mut reporter, cancel| {
            reporter.send(ScanEvent::Disk(scan::disk_of(&dirs)), false);
            let prober = probe::Prober::new();
            let total = targets.len();
            let mut results = Vec::with_capacity(PROBE_CHUNK);
            for (done, target) in targets.into_iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let recheck = scan::recheck(
                    &target.path,
                    target.kind,
                    target.known,
                    target.readable,
                    &prober,
                );
                results.push((target.key, recheck));
                if results.len() == PROBE_CHUNK {
                    let results = std::mem::take(&mut results);
                    reporter.send(
                        ScanEvent::Rechecked {
                            results,
                            done: done + 1,
                            total,
                        },
                        false,
                    );
                }
            }
            reporter.send(
                ScanEvent::Rechecked {
                    results,
                    done: total,
                    total,
                },
                false,
            );
            reporter.send(ScanEvent::Done, true);
        }));
    }
    /// Applies a refresh: changed files take their new size, time and probe, and files
    /// that no longer exist leave the workspace.
    fn apply_rechecks(&mut self, results: Vec<(String, scan::Recheck)>) {
        let mut members = self.members();
        let position: HashMap<String, usize> = members
            .iter()
            .enumerate()
            .map(|(index, member)| (scan::lower(&member.path), index))
            .collect();
        let mut gone = HashSet::new();
        for (key, recheck) in results {
            let Some(&index) = position.get(&key) else {
                continue;
            };
            match recheck {
                scan::Recheck::Unchanged => {}
                scan::Recheck::Gone => {
                    gone.insert(index);
                }
                scan::Recheck::Changed {
                    size,
                    modified,
                    modified_local,
                    probe,
                } => {
                    let member = &mut members[index];
                    member.size = size;
                    member.modified = modified;
                    member.modified_local = modified_local;
                    member.probe = probe;
                }
            }
        }
        let mut index = 0;
        members.retain(|_| {
            let keep = !gone.contains(&index);
            index += 1;
            keep
        });
        self.rebuild(members);
        // A changed file may carry different metadata now.
        if let Drawer::Details(key) = self.drawer.clone() {
            self.start_metadata(key);
        }
    }
    pub(super) fn cancel_scan(&mut self) {
        self.queued.clear();
        if let Some(scan) = &self.scan {
            scan.stop();
        }
    }
    pub(super) fn cancel_work(&mut self) {
        if let Some((_, job)) = &self.work {
            job.stop();
        }
    }
    pub(super) fn shutdown(&mut self) {
        if let Some(Metadata { job: Some(job), .. }) = self.metadata.take() {
            job.join(Duration::from_secs(2));
        }
        if let Some(scan) = self.scan.take() {
            scan.join(Duration::from_secs(2));
        }
        if let Some((_, work)) = self.work.take() {
            work.join(Duration::from_secs(10));
        }
    }

    fn rebuild(&mut self, members: Vec<Member>) {
        self.groups = plan::group(members);
        self.index.clear();
        for (g, group) in self.groups.iter().enumerate() {
            for (m, member) in group.members.iter().enumerate() {
                self.index.insert(scan::lower(&member.path), (g, m));
            }
        }
        self.replan();
    }
    fn members(&mut self) -> Vec<Member> {
        std::mem::take(&mut self.groups)
            .into_iter()
            .flat_map(|group| group.members)
            .collect()
    }
    fn replan(&mut self) {
        let template = self.template.as_ref().unwrap_or(&self.last_valid);
        let selected: Vec<bool> = self
            .groups
            .iter()
            .map(|group| !self.deselected.contains(&group.key()))
            .collect();
        let disk = &self.disk;
        self.rows = plan::plan(
            &self.groups,
            &selected,
            &plan::Options {
                template,
                mtime_fallback: self.mtime_fallback,
                lowercase_ext: self.lowercase_ext,
            },
            |path| disk.contains(&scan::lower(path)),
        );
    }
    /// Points workspace members at their new names after an apply or undo.
    fn follow(&mut self, moved: &[(PathBuf, PathBuf)]) {
        if moved.is_empty() {
            return;
        }
        let map: HashMap<String, &PathBuf> = moved
            .iter()
            .map(|(from, to)| (scan::lower(from), to))
            .collect();
        for (from, to) in moved {
            self.disk.remove(&scan::lower(from));
            self.disk.insert(scan::lower(to));
        }
        let mut members = self.members();
        for member in &mut members {
            if let Some(to) = map.get(&scan::lower(&member.path))
                && let Some((stem, suffix, _)) = plan::split_name(&file_name(to))
            {
                member.path = (*to).clone();
                member.stem = stem;
                member.suffix = suffix;
            }
        }
        self.rebuild(members);
    }

    /// Drains worker events; returns whether anything visible changed.
    pub(super) fn poll(&mut self) -> bool {
        let mut changed = false;
        let mut scan_done = false;
        // Probe results only change names through the plan, so one replan after the
        // drained batch is enough; replanning per chunk made scans quadratic.
        let mut replan = false;
        let mut rechecks = Vec::new();
        if let Some(scan) = &self.scan {
            let mut events = Vec::new();
            while let Ok(event) = scan.receiver.try_recv() {
                events.push(event);
            }
            if events.is_empty() && scan.thread.as_ref().is_none_or(JoinHandle::is_finished) {
                scan_done = true;
            }
            for event in events {
                changed = true;
                match event {
                    ScanEvent::Listed(listing) => {
                        self.disk.extend(listing.disk);
                        self.skipped += listing.skipped;
                        self.batch_capped |= listing.truncated;
                        let mut members = self.members();
                        members.extend(listing.members);
                        self.scan_progress = (0, members.len());
                        self.rebuild(members);
                    }
                    ScanEvent::Probed {
                        results,
                        done,
                        total,
                    } => {
                        for (key, probe) in results {
                            if let Some(&(g, m)) = self.index.get(&key) {
                                self.groups[g].members[m].probe = probe;
                            }
                        }
                        self.scan_progress = (done, total);
                        replan = true;
                    }
                    // Replaces the set: files deleted since the preview must not block a name.
                    // Targets only ever sit in group folders, which this listing covers.
                    ScanEvent::Disk(paths) => self.disk = paths,
                    ScanEvent::Rechecked {
                        results,
                        done,
                        total,
                    } => {
                        rechecks.extend(results);
                        self.scan_progress = (done, total);
                    }
                    ScanEvent::Done => scan_done = true,
                }
            }
        }
        if !rechecks.is_empty() {
            self.apply_rechecks(rechecks);
        } else if replan {
            self.replan();
        }
        if scan_done && self.scan.take().is_some() {
            changed = true;
            if !self.queued.is_empty() {
                let (paths, recurse) = self.queued.remove(0);
                self.start_scan(paths, recurse);
            }
        }
        changed |= self.poll_metadata();
        let mut finished = false;
        let mut outcome = None;
        if let Some((kind, work)) = &self.work {
            while let Ok(event) = work.receiver.try_recv() {
                changed = true;
                match event {
                    WorkEvent::Progress(done, total) => self.work_progress = (done, total),
                    WorkEvent::Loaded(loaded) => {
                        self.history = loaded.batches;
                        self.history_intact = loaded.intact;
                        finished = true;
                    }
                    WorkEvent::Applied(result) | WorkEvent::Undone(result) => {
                        outcome = Some((*kind, result));
                        finished = true;
                    }
                }
            }
            if !finished && work.thread.as_ref().is_none_or(JoinHandle::is_finished) {
                finished = true;
            }
        }
        if finished {
            self.work = None;
            changed = true;
        }
        if let Some((kind, result)) = outcome {
            self.finish_work(kind, result);
        }
        changed
    }
    fn finish_work(&mut self, kind: WorkKind, result: commit::Outcome) {
        self.history = result.history;
        self.follow(&result.moved);
        let failures = result
            .failed
            .iter()
            .map(|(path, error)| format!("{}: {error}", file_name(path)))
            .collect();
        self.notice = Some(if result.stale {
            Notice::Stale
        } else if let Some(error) = result.journal_error.filter(|_| result.moved.is_empty()) {
            Notice::JournalFailed(error)
        } else if kind == WorkKind::Undo {
            Notice::Undone {
                restored: result.moved.len(),
                skipped: result.skipped,
                failures,
            }
        } else {
            Notice::Applied {
                renamed: result.moved.len(),
                failures,
                cancelled: result.cancelled,
            }
        });
    }
    fn batch_id(&self) -> (u64, i64) {
        let at = clock::now_utc();
        let newest = self.history.iter().map(|batch| batch.id).max().unwrap_or(0);
        ((at.max(0) as u64).max(newest + 1), at)
    }
    /// Refuses history writes while the file is unreadable, so none can replace it.
    fn history_blocked(&mut self) -> bool {
        if self.history_intact {
            return false;
        }
        self.notice = Some(Notice::HistoryUnreadable);
        true
    }
    pub(super) fn apply(&mut self) {
        if self.work.is_some() || self.scan.is_some() || self.template.is_err() {
            return;
        }
        if self.history_blocked() {
            return;
        }
        let mut moves = Vec::new();
        for row in self
            .rows
            .iter()
            .filter(|r| r.selected && r.state == State::Ready)
        {
            let group = &self.groups[row.group];
            for (member, target) in group.members.iter().zip(&row.targets) {
                if member.path.file_name() != target.file_name() {
                    moves.push(commit::Planned {
                        group: row.group,
                        from: member.path.clone(),
                        to: target.clone(),
                        size: member.size,
                        modified: member.modified,
                    });
                }
            }
        }
        if moves.is_empty() {
            return;
        }
        let history = self.history.clone();
        let path = self.history_path.clone();
        let id = self.batch_id();
        self.notice = None;
        self.work_progress = (0, moves.len());
        let job = Job::spawn(self.hwnd, move |mut reporter, cancel| {
            let result = commit::apply(
                moves,
                history,
                path.as_deref(),
                id,
                cancel,
                &mut |done, total| reporter.send(WorkEvent::Progress(done, total), false),
            );
            reporter.send(WorkEvent::Applied(result), true);
        });
        self.work = Some((WorkKind::Apply, job));
    }
    pub(super) fn undo(&mut self, target: u64) {
        if self.work.is_some() || self.history_blocked() {
            return;
        }
        let history = self.history.clone();
        let path = self.history_path.clone();
        let id = self.batch_id();
        self.notice = None;
        self.work_progress = (0, 0);
        let job = Job::spawn(self.hwnd, move |mut reporter, cancel| {
            let result = commit::undo(
                target,
                history,
                path.as_deref(),
                id,
                cancel,
                &mut |done, total| reporter.send(WorkEvent::Progress(done, total), false),
            );
            reporter.send(WorkEvent::Undone(result), true);
        });
        self.work = Some((WorkKind::Undo, job));
    }
    pub(super) fn clear_history(&mut self) {
        if self.work.is_some() || self.history_blocked() {
            return;
        }
        self.history.clear();
        if let Some(path) = &self.history_path {
            let _ = commit::write_history(path, &[]);
        }
        self.notice = Some(Notice::HistoryCleared);
    }

    fn visible(&self) -> impl Iterator<Item = &Row> {
        let needle = self.filter_text.trim().to_lowercase();
        self.rows.iter().filter(move |row| {
            if !self.state_filter.matches(row) {
                return false;
            }
            if needle.is_empty() {
                return true;
            }
            let group = &self.groups[row.group];
            group
                .members
                .iter()
                .map(|m| file_name(&m.path))
                .chain(row.targets.iter().map(|t| file_name(t)))
                .any(|name| name.to_lowercase().contains(&needle))
        })
    }
    /// Handles workspace actions; preference and picker actions are routed by the caller.
    pub(super) fn act(&mut self, action: Action) {
        match action {
            Action::Add(paths) => self.add(paths),
            Action::Clear => self.clear(),
            Action::Apply => self.apply(),
            Action::Refresh => self.refresh(),
            Action::CancelWork => self.cancel_work(),
            Action::CancelScan => self.cancel_scan(),
            Action::SetTemplate(text) => {
                self.template = Template::parse(&text);
                if let Ok(template) = &self.template {
                    self.last_valid = template.clone();
                }
                self.template_text = text;
                self.replan();
            }
            Action::SetMtimeFallback(on) => {
                self.mtime_fallback = on;
                self.replan();
            }
            Action::SetRecurse(on) => self.recurse = on,
            Action::SetFilterText(text) => self.filter_text = text,
            Action::SetStateFilter(filter) => self.state_filter = filter,
            Action::ClearFilter => {
                self.filter_text.clear();
                self.state_filter = StateFilter::All;
            }
            Action::SetSelected(group, selected) => {
                if let Some(group) = self.groups.get(group) {
                    let key = group.key();
                    if selected {
                        self.deselected.remove(&key);
                    } else {
                        self.deselected.insert(key);
                    }
                    self.replan();
                }
            }
            Action::SetSelectAllVisible(selected) => {
                let keys: Vec<(String, String)> = self
                    .visible()
                    .filter(|row| row.state == State::Ready)
                    .map(|row| self.groups[row.group].key())
                    .collect();
                for key in keys {
                    if selected {
                        self.deselected.remove(&key);
                    } else {
                        self.deselected.insert(key);
                    }
                }
                self.replan();
            }
            Action::OpenDetails(group) => {
                if let Some(group) = self.groups.get(group) {
                    let key = group.key();
                    self.set_drawer(Drawer::Details(key.clone()));
                    self.start_metadata(key);
                }
            }
            Action::ToggleHistory => {
                let drawer = if self.drawer == Drawer::History {
                    Drawer::Hidden
                } else {
                    Drawer::History
                };
                self.set_drawer(drawer);
            }
            Action::ToggleSettings => {
                let drawer = if self.drawer == Drawer::Settings {
                    Drawer::Hidden
                } else {
                    Drawer::Settings
                };
                self.set_drawer(drawer);
            }
            Action::CloseDrawer => self.set_drawer(Drawer::Hidden),
            Action::Undo(id) => self.undo(id),
            Action::ClearHistory => self.clear_history(),
            Action::DismissNotice => self.notice = None,
            Action::AddFiles
            | Action::AddFolder
            | Action::SaveDefaultTemplate
            | Action::SetDefaultRecurse(_)
            | Action::SetDefaultMtimeFallback(_)
            | Action::SetLowercaseExt(_) => {}
        }
    }
}
