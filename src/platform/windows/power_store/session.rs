//! PowerStore UI session: unified package list, Discover search mode, serial operation
//! queue, periodic update checks, side-drawer state, history and export/import.
//! Owned by the UI thread; manager calls run on worker threads (WinGet via broker child,
//! Scoop/Chocolatey via CLI).
use super::super::clock;
use super::broker::CancelHandle;
use super::catalog::{self, InstalledList, ManagerStatus};
use super::file_write::write_replacing;
use super::icon_db;
use super::icons;
use super::manager_probe::{self, ManagerProbe};
use super::winget::ManagerError;
use crate::power_store::{
    ExportPackage, History, HistoryEntry, HistoryOutcome, InstallOptions, InstalledCache,
    LIST_WORKER_STOPPED, ListMode, ListViewQuery, ManagerKind, ManagerSettings, OperationKind,
    OperationProgress, OperationQueue, PackageDetails, PackageKey, PackageSummary,
    PowerStorePreferences, QueuedOperation, ScopeFilter, SortKey, StatusFilter, UpdateFilters,
    available_sources, export_document, filter_updatable, query_packages,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

mod listing;
mod logo_loader;
#[cfg(test)]
mod tests;

use listing::ListOutcome;
use logo_loader::{IconMessage, carry_icons};

pub const MAX_VISIBLE_PACKAGES: usize = 500;
/// Exit runs on the UI thread (also for WM_ENDSESSION), so it only briefly waits for a
/// cancelled operation; the child also cancels on its own once our stdin pipe closes.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);
/// The first background check waits so it never competes with app startup.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);
/// History rows in the drawer snapshot; logs are shortened for display.
const VISIBLE_HISTORY_LOG_CHARS: usize = 300;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Ready,
    Unavailable(String),
    Failed(String),
}

/// Where one package stands in the serial operation queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowOperation {
    Running {
        kind: OperationKind,
        progress: Option<OperationProgress>,
        cancelling: bool,
    },
    /// Waiting at `index` in the queue (0 is next).
    Pending { kind: OperationKind, index: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageRow {
    pub key: PackageKey,
    /// Queued or running operation for this package; shown on its own row.
    pub operation: Option<RowOperation>,
    pub name: String,
    pub version: String,
    pub available: String,
    pub actionable_update: bool,
    pub installed: bool,
    pub ignored: bool,
    pub skipped: bool,
    pub selected: bool,
    pub configured: bool,
    /// Delivered by Microsoft Store: no WinGet update count; the action opens Store updates.
    pub store_managed: bool,
    pub scope: String,
    pub install_date: String,
    pub updated_at: String,
    /// PNG logo bytes already prepared by the catalog worker.
    pub icon: Option<Arc<[u8]>>,
}

impl PackageRow {
    fn new(
        package: &PackageSummary,
        prefs: &PowerStorePreferences,
        selected: bool,
        store_managed: bool,
    ) -> Self {
        let installed = package.version.is_some();
        let ignored = prefs.filters.is_ignored(&package.key);
        let skipped = package
            .available_version
            .as_deref()
            .is_some_and(|avail| prefs.filters.skipped_version(&package.key) == Some(avail));
        let actionable_update = package.has_actionable_update(&prefs.filters);
        // An installed package ahead of the catalog has no newer version to show.
        let available = if installed && !package.update_available {
            String::new()
        } else {
            package.available_version.clone().unwrap_or_default()
        };
        let scope = package
            .scope
            .clone()
            .or_else(|| {
                prefs
                    .options
                    .get(&package.key)
                    .and_then(|opt| opt.scope.clone())
            })
            .unwrap_or_default();
        Self {
            key: package.key.clone(),
            operation: None,
            name: package.name.clone(),
            version: package.version.clone().unwrap_or_default(),
            available,
            actionable_update,
            installed,
            ignored,
            skipped,
            selected,
            configured: prefs.options.contains_key(&package.key),
            store_managed,
            scope,
            install_date: package.install_date.clone().unwrap_or_default(),
            updated_at: package.updated_at.clone().unwrap_or_default(),
            icon: package.icon.as_deref().and_then(icons::cached),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationLabel {
    pub kind: OperationKind,
    pub name: String,
}

impl From<&QueuedOperation> for OperationLabel {
    fn from(operation: &QueuedOperation) -> Self {
        Self {
            kind: operation.kind,
            name: operation.name.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpResult {
    Succeeded,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpStatus {
    pub running: Option<OperationLabel>,
    pub progress: Option<OperationProgress>,
    pub cancelling: bool,
    pub pending: usize,
    pub pending_items: Vec<OperationLabel>,
    pub last: Option<(OperationLabel, OpResult)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DetailsView {
    #[default]
    Hidden,
    Loading(String),
    Ready(PackageDetails),
    Failed(String, String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectorView {
    pub key: PackageKey,
    pub name: String,
    pub version: String,
    pub available: String,
    pub actionable_update: bool,
    pub installed: bool,
    pub ignored: bool,
    pub skipped_version: Option<String>,
    pub scope: String,
    pub install_date: String,
    pub updated_at: String,
    pub options: InstallOptions,
    pub details: DetailsView,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DrawerView {
    #[default]
    Hidden,
    Inspector(Box<InspectorView>),
    History,
    Settings,
    /// Detail settings of one package manager, opened from the settings drawer.
    Manager(ManagerKind),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum ActiveDrawer {
    #[default]
    Hidden,
    Inspector(PackageKey, String),
    History,
    Settings,
    Manager(ManagerKind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportRow {
    pub package: ExportPackage,
    pub selected: bool,
}

/// One-shot feedback banner shown on the Power Store page or drawer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    Exported(usize, String),
    ExportFailed(String),
    Imported(usize),
    ImportFailed(String),
    OptionsSaved(String),
    OptionsRejected,
    Copied(String),
    /// A background refresh failed, so the rows on screen are the previous list.
    RefreshFailed(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateCheck {
    pub count: Option<usize>,
    pub checking: bool,
    pub last_checked: Option<String>,
}

/// Page requests from the settings shell, applied on the UI thread by the app window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    SetFilterText(String),
    /// Adds (`true`) or removes one source from the multi-select source filter.
    SetSourceChecked(String, bool),
    ClearSourceFilter,
    SetStatusFilter(StatusFilter),
    SetScopeFilter(ScopeFilter),
    /// Picks a key or flips its direction when it is already active (column header).
    SetSortKey(SortKey),
    SetSort(SortKey, bool),
    StartDiscover {
        query: String,
        source: Option<String>,
    },
    ExitDiscover,
    SetSelected(PackageKey, bool),
    SetSelectAllVisible(bool),
    ClearSelection,
    Queue(OperationKind, PackageKey, String),
    QueueSelected(OperationKind),
    /// Queues actionable updates that match the current list filters (page "Update all").
    QueueVisibleUpdates,
    /// Opens Microsoft Store's own updates page for Store-delivered packages.
    OpenStoreUpdates,
    IgnoreSelected,
    OpenInspector(PackageKey),
    ToggleHistoryDrawer,
    ToggleSettingsDrawer,
    CloseDrawer,
    SaveOptions(PackageKey, InstallOptions),
    SkipVersion(PackageKey, String),
    Ignore(PackageKey),
    Unignore(PackageKey),
    Unskip(PackageKey),
    CopyId(String),
    DismissNotice,
    /// Next longer (`true`) or shorter check interval.
    StepCheckMinutes(bool),
    SetNotify(bool),
    SetRemoteIcons(bool),
    SetManagerEnabled(ManagerKind, bool),
    /// Opens one manager's detail drawer from the settings drawer.
    OpenManagerDrawer(ManagerKind),
    /// Goes back from a manager's detail drawer to the settings drawer.
    ShowSettingsDrawer,
    /// Opens a file dialog to choose the program a CLI manager runs.
    PickManagerExecutable(ManagerKind),
    /// Goes back to finding the manager's program on PATH.
    ResetManagerExecutable(ManagerKind),
    /// Default install scope (`user` / `machine`) for the manager; `None` is automatic.
    SetManagerScope(ManagerKind, Option<String>),
    /// Default installer architecture (see `ARCHITECTURES`) for the manager; `None` is automatic.
    SetManagerArchitecture(ManagerKind, Option<String>),
    /// Default vcpkg triplet (see `VCPKG_TRIPLETS`); `None` leaves the choice to vcpkg.
    SetManagerTriplet(ManagerKind, Option<String>),
    Export,
    Import,
    SetImport(PackageKey, bool),
    InstallImport,
    ClearImport,
    ClearHistory,
    /// Drop one waiting queue entry by index (not the running operation).
    RemovePending(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub mode: ListMode,
    pub status: Status,
    pub discover_query: String,
    pub discover_source: Option<String>,
    pub query: ListViewQuery,
    pub sources: Vec<String>,
    /// Shared so the page's virtualized table can keep rows without copying them again.
    pub packages: Rc<[PackageRow]>,
    pub total_installed: usize,
    pub pinned_count: usize,
    pub visible_count: usize,
    /// Actionable updates inside the current list filters (not the global check count).
    pub visible_updatable_count: usize,
    pub truncated: bool,
    pub selected_count: usize,
    pub selected_updatable_count: usize,
    pub selected_removable_count: usize,
    pub all_visible_selected: bool,
    /// Background list refresh while the previous Installed rows stay on screen.
    pub refreshing: bool,
    /// The Installed rows are the list saved by an earlier run, not yet confirmed.
    pub from_cache: bool,
    pub op: OpStatus,
    pub check: UpdateCheck,
    pub drawer: DrawerView,
    pub history: Vec<HistoryEntry>,
    pub import: Vec<ImportRow>,
    pub notice: Option<Notice>,
    pub ignored: Vec<PackageKey>,
    pub skipped: Vec<(PackageKey, String)>,
    pub check_minutes: u32,
    pub notify: bool,
    pub remote_icons: bool,
    pub managers: Vec<ManagerStatus>,
    /// Saved per-manager settings; managers not listed use the defaults.
    pub manager_settings: BTreeMap<ManagerKind, ManagerSettings>,
    /// Program path and version looked up when a manager's drawer was opened or edited.
    pub manager_probes: BTreeMap<ManagerKind, ProbeView>,
}

/// A manager's program lookup as the drawer shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeView {
    Checking,
    Done(ManagerProbe),
}

struct ProbeEntry {
    /// Which lookup this is; an older lookup finishing late must not replace a newer one.
    serial: u64,
    view: ProbeView,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            mode: ListMode::Installed,
            status: Status::Idle,
            discover_query: String::new(),
            discover_source: None,
            query: ListViewQuery::default(),
            sources: vec!["winget".into(), "msstore".into()],
            packages: Rc::from(Vec::new()),
            total_installed: 0,
            pinned_count: 0,
            visible_count: 0,
            visible_updatable_count: 0,
            truncated: false,
            selected_count: 0,
            selected_updatable_count: 0,
            selected_removable_count: 0,
            all_visible_selected: false,
            refreshing: false,
            from_cache: false,
            op: OpStatus::default(),
            check: UpdateCheck::default(),
            drawer: DrawerView::Hidden,
            history: Vec::new(),
            import: Vec::new(),
            notice: None,
            ignored: Vec::new(),
            skipped: Vec::new(),
            check_minutes: crate::power_store::DEFAULT_CHECK_MINUTES,
            notify: true,
            remote_icons: false,
            // Located on a worker by `refresh_managers`; the UI thread never probes here.
            managers: Vec::new(),
            manager_settings: BTreeMap::new(),
            manager_probes: BTreeMap::new(),
        }
    }
}

enum OpOutcome {
    Progress(OperationProgress),
    Finished(OpResult),
}

enum JobOutcome {
    Checked(Result<InstalledList, String>),
    Details(PackageKey, Result<Box<PackageDetails>, String>),
    Exported(Result<(usize, String), String>),
    /// A manager program lookup (see [`Session::probe_manager`]) with its serial.
    Probed(ManagerKind, u64, ManagerProbe),
    /// Startup presence of every manager (see [`Session::refresh_managers`]).
    Managers(Vec<ManagerStatus>),
}

pub struct Session {
    mode: ListMode,
    installed_status: Status,
    discover_status: Status,
    discover_query: String,
    discover_source: Option<String>,
    query: ListViewQuery,
    installed_packages: Vec<PackageSummary>,
    /// Installed rows whose MSIX package Microsoft Store delivers (from the last list load).
    store_managed: BTreeSet<PackageKey>,
    discover_packages: Vec<PackageSummary>,
    selected: BTreeSet<PackageKey>,
    // Each list mode has its own worker channel and generation: a Discover search must
    // not drop or stale-out an Installed load that is still running.
    installed_generation: u64,
    installed_receiver: Option<Receiver<ListOutcome>>,
    discover_generation: u64,
    discover_receiver: Option<Receiver<ListOutcome>>,
    /// True while a silent Installed refresh runs; previous rows stay visible.
    refreshing: bool,
    /// Where the last Installed list is saved; `None` keeps lists in memory only.
    installed_cache_path: Option<PathBuf>,
    /// True while the Installed rows come from `installed_cache_path`, before a fresh list.
    from_cache: bool,
    /// Mirrors preferences; catalog workers read it without touching settings.
    remote_icons: bool,
    queue: OperationQueue,
    running: Option<QueuedOperation>,
    progress: Option<OperationProgress>,
    cancel: Option<CancelHandle>,
    cancelling: bool,
    last: Option<(OperationLabel, OpResult)>,
    op_receiver: Option<Receiver<OpOutcome>>,
    op_worker: Option<JoinHandle<()>>,
    jobs: Vec<Receiver<JobOutcome>>,
    updates: Option<Vec<PackageSummary>>,
    checking: bool,
    next_check: Option<Instant>,
    last_checked: Option<String>,
    notified: usize,
    drawer: ActiveDrawer,
    details: DetailsView,
    details_key: Option<PackageKey>,
    import: Vec<ImportRow>,
    notice: Option<Notice>,
    history: History,
    history_path: Option<PathBuf>,
    managers: Vec<ManagerStatus>,
    probes: BTreeMap<ManagerKind, ProbeEntry>,
    probe_serial: u64,
    /// Logo work of the latest list: database lookups and downloads run off the UI thread.
    icon_receiver: Option<Receiver<IconMessage>>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            mode: ListMode::Installed,
            installed_status: Status::Idle,
            discover_status: Status::Idle,
            discover_query: String::new(),
            discover_source: None,
            query: ListViewQuery::default(),
            installed_packages: Vec::new(),
            store_managed: BTreeSet::new(),
            discover_packages: Vec::new(),
            selected: BTreeSet::new(),
            installed_generation: 0,
            installed_receiver: None,
            discover_generation: 0,
            discover_receiver: None,
            refreshing: false,
            installed_cache_path: None,
            from_cache: false,
            remote_icons: false,
            queue: OperationQueue::default(),
            running: None,
            progress: None,
            cancel: None,
            cancelling: false,
            last: None,
            op_receiver: None,
            op_worker: None,
            jobs: Vec::new(),
            updates: None,
            checking: false,
            next_check: None,
            last_checked: None,
            notified: 0,
            drawer: ActiveDrawer::Hidden,
            details: DetailsView::Hidden,
            details_key: None,
            import: Vec::new(),
            notice: None,
            history: History::default(),
            history_path: None,
            managers: Vec::new(),
            probes: BTreeMap::new(),
            probe_serial: 0,
            icon_receiver: None,
        }
    }
}

impl Session {
    /// The running or waiting operation for `key`, so the table row can show its state.
    fn row_operation(&self, key: &PackageKey) -> Option<RowOperation> {
        if let Some(running) = self.running.as_ref().filter(|running| &running.key == key) {
            return Some(RowOperation::Running {
                kind: running.kind,
                progress: self.progress,
                cancelling: self.cancelling,
            });
        }
        self.queue
            .iter()
            .enumerate()
            .find(|(_, queued)| &queued.key == key)
            .map(|(index, queued)| RowOperation::Pending {
                kind: queued.kind,
                index,
            })
    }

    /// PowerKit itself and Store-delivered packages are never removed from this page.
    fn is_removable(&self, key: &PackageKey) -> bool {
        !key.is_own_package() && !self.store_managed.contains(key)
    }

    fn active_packages(&self) -> &[PackageSummary] {
        match self.mode {
            ListMode::Installed => &self.installed_packages,
            ListMode::Discover => &self.discover_packages,
        }
    }

    fn active_status(&self) -> &Status {
        match self.mode {
            ListMode::Installed => &self.installed_status,
            ListMode::Discover => &self.discover_status,
        }
    }

    pub fn snapshot(&self, prefs: &PowerStorePreferences) -> Snapshot {
        let pool = self.active_packages();
        let filtered = query_packages(pool, self.mode, &self.query, prefs);
        let visible_count = filtered.len();
        let visible_updatable_count = filtered
            .iter()
            .filter(|package| package.has_actionable_update(&prefs.filters))
            .count();
        let packages: Vec<PackageRow> = filtered
            .into_iter()
            .take(MAX_VISIBLE_PACKAGES)
            .map(|package| {
                let mut row = PackageRow::new(
                    package,
                    prefs,
                    self.selected.contains(&package.key),
                    self.store_managed.contains(&package.key),
                );
                row.operation = self.row_operation(&package.key);
                row
            })
            .collect();

        let all_visible_selected = !packages.is_empty() && packages.iter().all(|row| row.selected);

        let selected_packages: Vec<&PackageSummary> = pool
            .iter()
            .filter(|package| self.selected.contains(&package.key))
            .collect();
        let selected_count = selected_packages.len();
        let selected_updatable_count = selected_packages
            .iter()
            .filter(|package| package.has_actionable_update(&prefs.filters))
            .count();
        let selected_removable_count = selected_packages
            .iter()
            .filter(|package| self.is_removable(&package.key))
            .count();

        let pinned_count = self
            .installed_packages
            .iter()
            .filter(|package| package.is_pinned(&prefs.filters))
            .count();

        let drawer = match &self.drawer {
            ActiveDrawer::Hidden => DrawerView::Hidden,
            ActiveDrawer::History => DrawerView::History,
            ActiveDrawer::Settings => DrawerView::Settings,
            ActiveDrawer::Manager(kind) => DrawerView::Manager(*kind),
            ActiveDrawer::Inspector(key, fallback_name) => {
                DrawerView::Inspector(Box::new(self.build_inspector(key, fallback_name, prefs)))
            }
        };

        let history = if matches!(self.drawer, ActiveDrawer::History) {
            self.history
                .newest_first()
                .map(|entry| {
                    let mut entry = entry.clone();
                    if let Some((index, _)) =
                        entry.log.char_indices().nth(VISIBLE_HISTORY_LOG_CHARS)
                    {
                        entry.log.truncate(index);
                        entry.log.push('…');
                    }
                    entry
                })
                .collect()
        } else {
            Vec::new()
        };

        Snapshot {
            mode: self.mode,
            status: self.active_status().clone(),
            discover_query: self.discover_query.clone(),
            discover_source: self.discover_source.clone(),
            query: self.query.clone(),
            sources: available_sources(pool),
            packages: packages.into(),
            total_installed: self.installed_packages.len(),
            pinned_count,
            visible_count,
            visible_updatable_count,
            truncated: visible_count > MAX_VISIBLE_PACKAGES,
            selected_count,
            selected_updatable_count,
            selected_removable_count,
            all_visible_selected,
            refreshing: self.refreshing
                && self.mode == ListMode::Installed
                && !matches!(self.installed_status, Status::Loading),
            from_cache: self.from_cache && self.mode == ListMode::Installed,
            op: OpStatus {
                running: self.running.as_ref().map(OperationLabel::from),
                progress: self.progress,
                cancelling: self.cancelling,
                pending: self.queue.len(),
                pending_items: self.queue.iter().map(OperationLabel::from).collect(),
                last: self.last.clone(),
            },
            check: UpdateCheck {
                count: self.update_count(&prefs.filters),
                checking: self.checking,
                last_checked: self.last_checked.clone(),
            },
            drawer,
            history,
            import: self.import.clone(),
            notice: self.notice.clone(),
            ignored: prefs.filters.ignored().cloned().collect(),
            skipped: prefs
                .filters
                .skipped()
                .map(|(key, version)| (key.clone(), version.clone()))
                .collect(),
            check_minutes: prefs.check_minutes,
            notify: prefs.notify,
            remote_icons: prefs.remote_icons,
            managers: self.managers.clone(),
            manager_settings: prefs.managers.clone(),
            manager_probes: self
                .probes
                .iter()
                .map(|(kind, entry)| (*kind, entry.view.clone()))
                .collect(),
        }
    }

    fn build_inspector(
        &self,
        key: &PackageKey,
        fallback_name: &str,
        prefs: &PowerStorePreferences,
    ) -> InspectorView {
        let found = self
            .installed_packages
            .iter()
            .find(|package| &package.key == key)
            .or_else(|| {
                self.discover_packages
                    .iter()
                    .find(|package| &package.key == key)
            });
        let details_name = match &self.details {
            DetailsView::Ready(details) if details.id == key.id && !details.name.is_empty() => {
                Some(details.name.as_str())
            }
            _ => None,
        };
        let name = found
            .map(|package| package.name.as_str())
            .or(details_name)
            .unwrap_or(fallback_name)
            .to_string();
        let installed = found.is_some_and(|package| package.version.is_some());
        let version = found
            .and_then(|package| package.version.clone())
            .unwrap_or_default();
        let available = match found {
            Some(package) if installed && !package.update_available => String::new(),
            Some(package) => package.available_version.clone().unwrap_or_default(),
            None => match &self.details {
                DetailsView::Ready(details) if details.id == key.id => details.version.clone(),
                _ => String::new(),
            },
        };
        let actionable_update =
            found.is_some_and(|package| package.has_actionable_update(&prefs.filters));
        let scope = found
            .and_then(|package| package.scope.clone())
            .or_else(|| {
                prefs
                    .options
                    .get(key)
                    .and_then(|options| options.scope.clone())
            })
            .unwrap_or_default();
        InspectorView {
            key: key.clone(),
            name,
            version,
            available,
            actionable_update,
            installed,
            ignored: prefs.filters.is_ignored(key),
            skipped_version: prefs.filters.skipped_version(key).map(str::to_string),
            scope,
            install_date: found
                .and_then(|package| package.install_date.clone())
                .unwrap_or_default(),
            updated_at: found
                .and_then(|package| package.updated_at.clone())
                .unwrap_or_default(),
            options: prefs.options_for(key),
            details: self.details.clone(),
        }
    }

    /// Updatable packages after ignore/skip rules; `None` before the first check.
    pub fn update_count(&self, filters: &UpdateFilters) -> Option<usize> {
        self.updates
            .as_ref()
            .map(|updates| filter_updatable(updates, filters).len())
    }

    pub fn checking(&self) -> bool {
        self.checking
    }

    pub fn set_filter_text(&mut self, text: String) {
        self.query.filter_text = text;
    }

    pub fn set_source_checked(&mut self, source: &str, checked: bool) {
        let source = source.trim().to_ascii_lowercase();
        if source.is_empty() {
            return;
        }
        if checked {
            self.query.sources.insert(source);
        } else {
            self.query.sources.remove(&source);
        }
    }

    pub fn clear_source_filter(&mut self) {
        self.query.sources.clear();
    }

    pub fn set_status_filter(&mut self, status: StatusFilter) {
        self.query.status = status;
    }

    pub fn set_scope_filter(&mut self, scope: ScopeFilter) {
        self.query.scope = scope;
    }

    pub fn set_sort_key(&mut self, key: SortKey) {
        if self.query.sort_by == key {
            self.query.sort_ascending = !self.query.sort_ascending;
        } else {
            self.query.sort_by = key;
            self.query.sort_ascending = key.default_ascending();
        }
    }

    pub fn set_sort(&mut self, key: SortKey, ascending: bool) {
        self.query.sort_by = key;
        self.query.sort_ascending = ascending;
    }

    pub fn start_discover(&mut self, query: String, source: Option<String>) {
        let trimmed = query.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        self.mode = ListMode::Discover;
        self.discover_query = trimmed;
        self.discover_source = source.filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all"));
        self.discover_packages.clear();
        self.selected.clear();
        self.query.filter_text.clear();
        self.refresh_active();
    }

    pub fn exit_discover(&mut self) {
        if self.mode == ListMode::Discover {
            self.mode = ListMode::Installed;
            self.selected.clear();
            self.query.filter_text.clear();
            self.refresh_installed(true);
        }
    }

    /// Opens the unified Installed view (tray / toast) and refreshes without blanking
    /// a previously loaded list.
    pub fn show_installed(&mut self) {
        self.mode = ListMode::Installed;
        self.refresh_installed(true);
    }

    /// Loads or silently refreshes when navigating to the Power Store page.
    pub fn ensure_loaded(&mut self) {
        if self.mode != ListMode::Installed {
            return;
        }
        match self.installed_status {
            Status::Idle => self.refresh_installed(false),
            Status::Ready | Status::Failed(_) | Status::Unavailable(_) => {
                self.refresh_installed(true);
            }
            Status::Loading => {}
        }
    }

    pub fn refresh_active(&mut self) {
        match self.mode {
            ListMode::Installed => self.refresh_installed(true),
            ListMode::Discover => self.refresh_discover(),
        }
    }

    pub fn set_remote_icons(&mut self, enabled: bool) {
        self.remote_icons = enabled;
    }

    pub fn enqueue(&mut self, operation: QueuedOperation) {
        self.queue.push(operation);
        self.kick_queue();
    }

    /// Queues every actionable update from the latest check or Installed list.
    /// Used by the tray; ignores page list filters.
    pub fn enqueue_updates(&mut self, prefs: &PowerStorePreferences) {
        let source_list = self.updates.as_deref().unwrap_or(&self.installed_packages);
        let operations: Vec<QueuedOperation> = filter_updatable(source_list, &prefs.filters)
            .into_iter()
            .map(|package| operation(OperationKind::Update, package, prefs))
            .collect();
        for operation in operations {
            self.queue.push(operation);
        }
        self.kick_queue();
    }

    /// Queues actionable updates that match the current Installed/Discover filters.
    pub fn enqueue_visible_updates(&mut self, prefs: &PowerStorePreferences) -> usize {
        if self.mode != ListMode::Installed {
            return 0;
        }
        let operations: Vec<QueuedOperation> =
            query_packages(self.active_packages(), self.mode, &self.query, prefs)
                .into_iter()
                .filter(|package| package.has_actionable_update(&prefs.filters))
                .map(|package| operation(OperationKind::Update, package, prefs))
                .collect();
        let count = operations.len();
        for operation in operations {
            self.queue.push(operation);
        }
        self.kick_queue();
        count
    }

    /// Queues the selected rows of the current list; returns how many were queued.
    pub fn enqueue_selected(
        &mut self,
        kind: OperationKind,
        prefs: &PowerStorePreferences,
    ) -> usize {
        let operations: Vec<QueuedOperation> = self
            .active_packages()
            .iter()
            .filter(|package| self.selected.contains(&package.key))
            .filter(|package| match kind {
                OperationKind::Install => true,
                OperationKind::Update => package.has_actionable_update(&prefs.filters),
                OperationKind::Uninstall => self.is_removable(&package.key),
            })
            .map(|package| operation(kind, package, prefs))
            .collect();
        let count = operations.len();
        for operation in operations {
            self.queue.push(operation);
        }
        self.selected.clear();
        self.kick_queue();
        count
    }

    /// Selected non-PowerKit keys, clearing selection afterwards.
    pub fn take_selected_non_own(&mut self) -> Vec<PackageKey> {
        let keys: Vec<PackageKey> = self
            .active_packages()
            .iter()
            .filter(|package| self.selected.contains(&package.key) && !package.key.is_own_package())
            .map(|package| package.key.clone())
            .collect();
        self.selected.clear();
        keys
    }

    pub fn set_selected(&mut self, key: PackageKey, selected: bool) {
        if selected {
            if self
                .active_packages()
                .iter()
                .any(|package| package.key == key)
            {
                self.selected.insert(key);
            }
        } else {
            self.selected.remove(&key);
        }
    }

    pub fn set_select_all_visible(&mut self, select: bool, prefs: &PowerStorePreferences) {
        let visible_keys: Vec<PackageKey> =
            query_packages(self.active_packages(), self.mode, &self.query, prefs)
                .into_iter()
                .take(MAX_VISIBLE_PACKAGES)
                .map(|package| package.key.clone())
                .collect();
        if select {
            self.selected.extend(visible_keys);
        } else {
            for key in visible_keys {
                self.selected.remove(&key);
            }
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    pub fn clear_pending(&mut self) {
        self.queue.clear();
    }

    pub fn remove_pending(&mut self, index: usize) -> bool {
        self.queue.remove_at(index).is_some()
    }

    pub fn cancel_running(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
            self.cancelling = true;
        }
    }

    pub fn open_inspector(&mut self, key: PackageKey) {
        let fallback_name = self
            .installed_packages
            .iter()
            .chain(self.discover_packages.iter())
            .find(|package| package.key == key)
            .map(|package| package.name.clone())
            .unwrap_or_else(|| key.id.clone());
        self.drawer = ActiveDrawer::Inspector(key.clone(), fallback_name);
        if self.details_key.as_ref() != Some(&key)
            || matches!(self.details, DetailsView::Hidden | DetailsView::Failed(..))
        {
            self.details = DetailsView::Loading(key.id.clone());
            self.details_key = Some(key.clone());
            self.spawn_job(move || {
                JobOutcome::Details(
                    key.clone(),
                    catalog::details(&key)
                        .map(Box::new)
                        .map_err(|e| e.to_string()),
                )
            });
        }
    }

    pub fn toggle_history_drawer(&mut self) {
        self.drawer = if matches!(self.drawer, ActiveDrawer::History) {
            ActiveDrawer::Hidden
        } else {
            ActiveDrawer::History
        };
    }

    pub fn toggle_settings_drawer(&mut self) {
        self.drawer = if matches!(self.drawer, ActiveDrawer::Settings) {
            ActiveDrawer::Hidden
        } else {
            ActiveDrawer::Settings
        };
    }

    /// Opens the settings drawer itself (also the way back from a manager's drawer).
    pub fn show_settings_drawer(&mut self) {
        self.drawer = ActiveDrawer::Settings;
    }

    pub fn open_manager_drawer(&mut self, kind: ManagerKind) {
        self.drawer = ActiveDrawer::Manager(kind);
        self.probe_manager(kind);
    }

    /// Looks up the manager's program path and version off the UI thread. Call it after
    /// the manager settings were published, so the lookup sees the switch and program.
    pub fn probe_manager(&mut self, kind: ManagerKind) {
        self.probe_serial += 1;
        let serial = self.probe_serial;
        self.probes.insert(
            kind,
            ProbeEntry {
                serial,
                view: ProbeView::Checking,
            },
        );
        self.spawn_job(move || JobOutcome::Probed(kind, serial, manager_probe::probe(kind)));
    }

    pub fn close_drawer(&mut self) {
        self.drawer = ActiveDrawer::Hidden;
    }

    pub fn set_notice(&mut self, notice: Notice) {
        self.notice = Some(notice);
    }

    pub fn dismiss_notice(&mut self) {
        self.notice = None;
        self.last = None;
    }

    pub fn load_import(&mut self, packages: Vec<ExportPackage>) {
        self.import = packages
            .into_iter()
            .filter(|package| package.key.manager.phase3())
            .map(|package| ImportRow {
                package,
                selected: true,
            })
            .collect();
        self.notice = Some(Notice::Imported(self.import.len()));
        self.drawer = ActiveDrawer::Settings;
    }

    pub fn set_import(&mut self, key: &PackageKey, selected: bool) {
        if let Some(row) = self.import.iter_mut().find(|row| &row.package.key == key) {
            row.selected = selected;
        }
    }

    pub fn clear_import(&mut self) {
        self.import.clear();
    }

    /// Selected import rows; the caller stores their options and queues installs.
    pub fn take_import_selection(&mut self) -> Vec<ExportPackage> {
        let selected = self
            .import
            .iter()
            .filter(|row| row.selected)
            .map(|row| row.package.clone())
            .collect();
        self.import.clear();
        selected
    }

    pub fn load_history(&mut self, path: PathBuf) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            self.history = History::parse(&text);
        }
        self.history_path = Some(path);
    }

    /// Sets where the Installed list is saved; the next cold load shows it first.
    pub fn set_installed_cache(&mut self, path: PathBuf) {
        self.installed_cache_path = Some(path);
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.save_history();
    }

    fn save_history(&self) {
        if let Some(path) = &self.history_path {
            let _ = write_replacing(path, &self.history.serialize());
        }
    }

    /// Overlays PowerStore's own operation timestamps onto installed package summaries.
    fn overlay_history_dates(&self, packages: &mut [PackageSummary]) {
        for entry in self.history.newest_first() {
            if entry.outcome != HistoryOutcome::Succeeded {
                continue;
            }
            let date_part = entry.time.get(..10).unwrap_or(&entry.time);
            if let Some(package) = packages.iter_mut().find(|pkg| pkg.key == entry.key) {
                match entry.kind {
                    OperationKind::Install => {
                        if package.install_date.is_none() && !date_part.is_empty() {
                            package.install_date = Some(date_part.to_string());
                        }
                        if package
                            .updated_at
                            .as_deref()
                            .is_none_or(|existing| date_part > existing)
                        {
                            package.updated_at = Some(date_part.to_string());
                        }
                    }
                    OperationKind::Update => {
                        if package
                            .updated_at
                            .as_deref()
                            .is_none_or(|existing| date_part > existing)
                        {
                            package.updated_at = Some(date_part.to_string());
                        }
                    }
                    OperationKind::Uninstall => {}
                }
            }
        }
    }

    /// Starts an export of installed catalog packages with their stored options.
    pub fn export(&mut self, path: PathBuf, prefs: &PowerStorePreferences) {
        let options = prefs.options.clone();
        let created = clock::iso_utc(clock::now_utc());
        self.spawn_job(move || {
            let result = catalog::list_installed()
                .map_err(|error| error.to_string())
                .and_then(|list| {
                    let packages: Vec<ExportPackage> = list
                        .packages
                        .into_iter()
                        // ARP-only / no-catalog rows cannot be reinstalled from export.
                        .filter(|package| {
                            package.available_version.is_some()
                                || package.key.manager != ManagerKind::WinGet
                        })
                        .map(|package| ExportPackage {
                            options: options.get(&package.key).cloned().unwrap_or_default(),
                            key: package.key,
                        })
                        .collect();
                    write_replacing(&path, &export_document(&packages, &created))
                        .map_err(|error| error.to_string())?;
                    Ok((packages.len(), path.display().to_string()))
                });
            JobOutcome::Exported(result)
        });
    }

    /// Starts a background update check now, unless one is already running.
    pub fn check_now(&mut self) {
        if self.checking {
            return;
        }
        self.checking = true;
        self.spawn_job(move || {
            JobOutcome::Checked(catalog::list_installed().map_err(|error| error.to_string()))
        });
    }

    /// Locates every manager on a worker, off the UI thread. Call once at startup, after
    /// the manager switches are published, so a switched-off Chocolatey is not probed.
    pub fn refresh_managers(&mut self) {
        self.spawn_job(|| JobOutcome::Managers(catalog::manager_statuses()));
    }

    /// Starts a periodic check when due. Call from the UI timer.
    pub fn tick(&mut self, now: Instant, check_minutes: u32) {
        let due = *self.next_check.get_or_insert(now + FIRST_CHECK_DELAY);
        if now >= due && !self.checking {
            self.next_check = Some(now + Duration::from_secs(u64::from(check_minutes) * 60));
            self.check_now();
        }
    }

    /// Reschedules the next periodic check after the interval changed.
    pub fn reschedule(&mut self, check_minutes: u32) {
        self.next_check = Some(Instant::now() + Duration::from_secs(u64::from(check_minutes) * 60));
    }

    /// Drops pending work, cancels the running operation and waits at most
    /// `SHUTDOWN_WAIT`; an operation still running after that is detached.
    pub fn shutdown(&mut self) {
        self.queue.clear();
        self.cancel_running();
        if let Some(receiver) = self.op_receiver.take() {
            let deadline = Instant::now() + SHUTDOWN_WAIT;
            let finished = loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match receiver.recv_timeout(remaining) {
                    Ok(OpOutcome::Progress(_)) => {}
                    Ok(OpOutcome::Finished(result)) => {
                        let _ = self.record(result);
                        break true;
                    }
                    Err(RecvTimeoutError::Disconnected) => break true,
                    Err(RecvTimeoutError::Timeout) => break false,
                }
            };
            let worker = self.op_worker.take();
            if finished && let Some(handle) = worker {
                let _ = handle.join();
            }
        }
        self.running = None;
        self.progress = None;
        self.cancel = None;
        self.cancelling = false;
        self.jobs.clear();
        self.refreshing = false;
        for receiver in [
            self.installed_receiver.take(),
            self.discover_receiver.take(),
        ]
        .into_iter()
        .flatten()
        {
            let _ = receiver.recv_timeout(Duration::from_millis(50));
        }
    }

    /// Applies finished background work. Returns whether the snapshot changed and,
    /// when a check found more updates than last notified, the new count.
    pub fn poll(&mut self, filters: &UpdateFilters) -> (bool, Option<usize>) {
        let list_changed = self.poll_list();
        let op_changed = self.poll_ops();
        let (job_changed, notify) = self.poll_jobs(filters);
        let icon_changed = self.poll_icons();
        (
            list_changed || op_changed || job_changed || icon_changed,
            notify,
        )
    }

    fn spawn_job(&mut self, job: impl FnOnce() -> JobOutcome + Send + 'static) {
        let (sender, receiver) = mpsc::channel();
        self.jobs.push(receiver);
        std::thread::spawn(move || {
            let _ = sender.send(job());
        });
    }

    fn poll_jobs(&mut self, filters: &UpdateFilters) -> (bool, Option<usize>) {
        let mut finished = Vec::new();
        self.jobs.retain(|receiver| match receiver.try_recv() {
            Ok(outcome) => {
                finished.push(outcome);
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        let mut notify = None;
        let changed = !finished.is_empty();
        for outcome in finished {
            match outcome {
                JobOutcome::Checked(result) => {
                    self.checking = false;
                    // A failed check keeps the previous result and time instead of
                    // reporting itself as a successful check.
                    if let Ok(list) = result {
                        self.apply_installed(list.packages, list.store_managed);
                        let count = self.update_count(filters).unwrap_or_default();
                        if count > self.notified {
                            notify = Some(count);
                        }
                        self.notified = count;
                    }
                }
                JobOutcome::Details(key, result) => {
                    if self.details_key.as_ref() == Some(&key) {
                        self.details = match result {
                            Ok(details) => DetailsView::Ready(*details),
                            Err(error) => DetailsView::Failed(key.id, error),
                        };
                    }
                }
                JobOutcome::Probed(kind, serial, probe) => {
                    if let Some(entry) = self.probes.get_mut(&kind)
                        && entry.serial == serial
                    {
                        entry.view = ProbeView::Done(probe);
                    }
                }
                JobOutcome::Managers(statuses) => {
                    // A list load that finished first already holds fresher presence
                    // (WinGet is refined by COM), so this applies only while none exists.
                    if self.managers.is_empty() {
                        self.managers = statuses;
                    }
                }
                JobOutcome::Exported(result) => {
                    self.notice = Some(match result {
                        Ok((count, path)) => Notice::Exported(count, path),
                        Err(error) => Notice::ExportFailed(error),
                    });
                }
            }
        }
        (changed, notify)
    }

    fn store_updates(&mut self, installed: &[PackageSummary]) {
        self.updates = Some(
            installed
                .iter()
                .filter(|package| package.update_available)
                .cloned()
                .collect(),
        );
    }

    fn prune_selection(&mut self) {
        let keys: BTreeSet<PackageKey> = self
            .active_packages()
            .iter()
            .map(|package| package.key.clone())
            .collect();
        self.selected.retain(|key| keys.contains(key));
    }

    fn poll_ops(&mut self) -> bool {
        let mut changed = false;
        loop {
            let Some(receiver) = &self.op_receiver else {
                return changed;
            };
            match receiver.try_recv() {
                Ok(OpOutcome::Progress(progress)) => {
                    changed |= self.progress.replace(progress) != Some(progress);
                }
                Ok(OpOutcome::Finished(result)) => {
                    self.finish_operation(result);
                    return true;
                }
                Err(TryRecvError::Empty) => return changed,
                Err(TryRecvError::Disconnected) => {
                    self.finish_operation(OpResult::Failed(
                        "PowerStore operation worker stopped".into(),
                    ));
                    return true;
                }
            }
        }
    }

    fn record(&mut self, result: OpResult) -> bool {
        let Some(operation) = self.running.take() else {
            return false;
        };
        let (outcome, log) = match &result {
            OpResult::Succeeded => (HistoryOutcome::Succeeded, String::new()),
            OpResult::Cancelled => (HistoryOutcome::Cancelled, String::new()),
            OpResult::Failed(error) => (HistoryOutcome::Failed, error.clone()),
        };
        let succeeded = result == OpResult::Succeeded;
        self.history.push(HistoryEntry {
            time: clock::local_stamp(clock::now_utc()),
            kind: operation.kind,
            key: operation.key.clone(),
            name: operation.name.clone(),
            outcome,
            log,
        });
        self.save_history();
        if succeeded {
            match operation.kind {
                OperationKind::Update => {
                    if let Some(updates) = &mut self.updates {
                        updates.retain(|package| package.key != operation.key);
                    }
                    if let Some(pkg) = self
                        .installed_packages
                        .iter_mut()
                        .find(|pkg| pkg.key == operation.key)
                    {
                        pkg.update_available = false;
                        if let Some(avail) = pkg.available_version.clone() {
                            pkg.version = Some(avail);
                        }
                    }
                }
                OperationKind::Uninstall => {
                    if let Some(updates) = &mut self.updates {
                        updates.retain(|package| package.key != operation.key);
                    }
                    self.installed_packages
                        .retain(|package| package.key != operation.key);
                }
                OperationKind::Install => {
                    self.discover_packages
                        .retain(|package| package.key != operation.key);
                }
            }
        }
        self.last = Some((OperationLabel::from(&operation), result));
        succeeded
    }

    fn finish_operation(&mut self, result: OpResult) {
        self.op_receiver = None;
        if let Some(handle) = self.op_worker.take() {
            let _ = handle.join();
        }
        let succeeded = self.record(result);
        self.progress = None;
        self.cancel = None;
        self.cancelling = false;
        self.kick_queue();
        // Refresh installed packages in the background once a batch completes.
        if succeeded && self.running.is_none() && self.queue.is_empty() {
            self.check_now();
        }
    }

    fn kick_queue(&mut self) {
        if self.op_receiver.is_some() || self.running.is_some() {
            return;
        }
        let Some(operation) = self.queue.pop_front() else {
            return;
        };
        self.running = Some(operation.clone());
        let cancel = CancelHandle::default();
        self.cancel = Some(cancel.clone());
        let (sender, receiver) = mpsc::channel();
        self.op_receiver = Some(receiver);
        self.op_worker = Some(std::thread::spawn(move || {
            let progress_sender = sender.clone();
            let result = catalog::run_operation(&operation, &cancel, |progress| {
                let _ = progress_sender.send(OpOutcome::Progress(progress));
            });
            let _ = sender.send(OpOutcome::Finished(match result {
                Ok(()) => OpResult::Succeeded,
                Err(ManagerError::Cancelled) => OpResult::Cancelled,
                Err(error) => OpResult::Failed(error.to_string()),
            }));
        }));
    }
}

fn operation(
    kind: OperationKind,
    package: &PackageSummary,
    prefs: &PowerStorePreferences,
) -> QueuedOperation {
    QueuedOperation {
        kind,
        key: package.key.clone(),
        name: package.name.clone(),
        options: prefs.operation_options_for(&package.key),
    }
}
