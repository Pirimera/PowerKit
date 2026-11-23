//! Fake ports for controller tests: a host whose clock, power state and
//! processes a test sets directly and whose requests it can read back.
use crate::controller::always_top::{Cue, WindowId};
use crate::controller::hotkeys::Slot;
use crate::ports::{
    Autostart, Clock, Hotkeys, KeepAwake, PinnableWindows, PowerStatus, Processes, SettingsStore,
    WakeTimer, WatchedProcess,
};
use powerkit_core::always_on::{Mode, PowerSource};
use powerkit_core::hotkey::Hotkey;
use powerkit_core::primitives::UtcTicks;
use powerkit_core::settings::Preferences;
use std::{
    cell::Cell,
    collections::{BTreeMap, HashMap},
    io,
    rc::Rc,
    sync::Arc,
};

/// A host with every port, driven by public fields.
pub struct FakeHost {
    pub uptime: u64,
    pub utc: UtcTicks,
    pub source: PowerSource,
    /// The keep-awake request currently held.
    pub awake: (Mode, bool),
    /// Makes the next keep-awake changes fail.
    pub fail_keep_awake: bool,
    /// The scheduled wake-up, if any.
    pub wake: Option<u64>,
    /// Makes scheduling a wake-up fail; cancelling still works.
    pub fail_wake: bool,
    pub pid: u32,
    /// Every preferences value written, oldest first.
    pub saved: Vec<Preferences>,
    pub fail_save: bool,
    /// Every start-with-the-system registration made, oldest first.
    pub autostart: Vec<bool>,
    pub fail_autostart: bool,
    /// The chord each slot currently holds.
    pub hotkeys: HashMap<Slot, Hotkey>,
    /// Every successful registration, oldest first.
    pub hotkey_log: Vec<(Slot, Hotkey)>,
    /// Chords another application holds.
    pub taken_chords: Vec<Hotkey>,
    processes: BTreeMap<u32, (String, Rc<Cell<bool>>)>,
}
impl Default for FakeHost {
    fn default() -> Self {
        Self {
            uptime: 0,
            // 2026-01-01T00:00:00Z.
            utc: UtcTicks::from_unix_ms(1_767_225_600_000),
            source: PowerSource {
                on_ac: true,
                battery: None,
            },
            awake: (Mode::Off, false),
            fail_keep_awake: false,
            wake: None,
            fail_wake: false,
            pid: 1,
            saved: Vec::new(),
            fail_save: false,
            autostart: Vec::new(),
            fail_autostart: false,
            hotkeys: HashMap::new(),
            hotkey_log: Vec::new(),
            taken_chords: Vec::new(),
            processes: BTreeMap::new(),
        }
    }
}
impl FakeHost {
    /// Adds a running process and returns the flag that marks it exited.
    pub fn add_process(&mut self, pid: u32, name: &str) -> Rc<Cell<bool>> {
        let exited = Rc::new(Cell::new(false));
        self.processes.insert(pid, (name.into(), exited.clone()));
        exited
    }
}
impl Clock for FakeHost {
    fn uptime_ms(&self) -> u64 {
        self.uptime
    }
    fn now_utc(&self) -> UtcTicks {
        self.utc
    }
}
impl KeepAwake for FakeHost {
    fn keep_awake(&mut self, mode: Mode, display: bool) -> io::Result<()> {
        if self.fail_keep_awake {
            return Err(io::Error::other("keep-awake refused"));
        }
        self.awake = (mode, mode.active() && display);
        Ok(())
    }
}
impl PowerStatus for FakeHost {
    fn power_source(&self) -> PowerSource {
        self.source
    }
}
impl Processes for FakeHost {
    fn current_pid(&self) -> u32 {
        self.pid
    }
    fn watch(&mut self, pid: u32) -> Option<Box<dyn WatchedProcess>> {
        let (name, exited) = self.processes.get(&pid)?;
        (!exited.get()).then(|| {
            Box::new(FakeProcess {
                name: name.clone(),
                exited: exited.clone(),
            }) as Box<dyn WatchedProcess>
        })
    }
}
impl WakeTimer for FakeHost {
    fn wake_after(&mut self, delay_ms: Option<u64>) -> io::Result<()> {
        if self.fail_wake && delay_ms.is_some() {
            return Err(io::Error::other("wake-up refused"));
        }
        self.wake = delay_ms;
        Ok(())
    }
}

impl SettingsStore for FakeHost {
    fn save(&mut self, prefs: &Preferences) -> io::Result<()> {
        if self.fail_save {
            return Err(io::Error::other("save refused"));
        }
        self.saved.push(prefs.clone());
        Ok(())
    }
}
impl Autostart for FakeHost {
    /// A refused registration is not recorded; an undo after a failed save is.
    fn set_autostart(&mut self, enabled: bool) -> io::Result<()> {
        if self.fail_autostart {
            return Err(io::Error::other("registration refused"));
        }
        self.autostart.push(enabled);
        Ok(())
    }
}
impl Hotkeys for FakeHost {
    /// A chord is refused while another application or another slot holds it.
    fn register(&mut self, slot: Slot, chord: Hotkey) -> bool {
        if self.taken_chords.contains(&chord) || self.hotkeys.values().any(|held| *held == chord) {
            return false;
        }
        self.hotkeys.insert(slot, chord);
        self.hotkey_log.push((slot, chord));
        true
    }
    fn unregister(&mut self, slot: Slot) {
        self.hotkeys.remove(&slot);
    }
}
struct FakeProcess {
    name: String,
    exited: Rc<Cell<bool>>,
}
impl WatchedProcess for FakeProcess {
    fn exited(&self) -> bool {
        self.exited.get()
    }
    fn name(&self) -> &str {
        &self.name
    }
}

/// A desktop of windows a test adds, closes and brings to the front.
#[derive(Default)]
pub struct FakeWindows {
    /// The window in front.
    pub front: Option<WindowId>,
    /// Pinned windows in pin order; a closed window stays until unpinned.
    pub pins: Vec<WindowId>,
    /// Every cue played, oldest first.
    pub cues: Vec<Cue>,
    /// Makes pin, unpin and opacity changes fail.
    pub fail: bool,
    /// Makes unpinning this one window fail.
    pub fail_unpin: Option<WindowId>,
    windows: HashMap<WindowId, FakeWindow>,
}
struct FakeWindow {
    title: String,
    own: bool,
    eligible: bool,
    opacity: i32,
}
impl FakeWindows {
    fn insert(&mut self, window: WindowId, title: &str, own: bool, eligible: bool) {
        self.windows.insert(
            window,
            FakeWindow {
                title: title.into(),
                own,
                eligible,
                opacity: 100,
            },
        );
    }
    /// A window of another application that can be pinned.
    pub fn add(&mut self, window: WindowId, title: &str) {
        self.insert(window, title, false, true);
    }
    /// A window of PowerKit itself.
    pub fn add_own(&mut self, window: WindowId) {
        self.insert(window, "PowerKit", true, false);
    }
    /// Another application's window that cannot be pinned, such as the desktop.
    pub fn add_ineligible(&mut self, window: WindowId) {
        self.insert(window, "", false, false);
    }
    /// Closes a window; a pin on it stays until it is unpinned.
    pub fn close(&mut self, window: WindowId) {
        self.windows.remove(&window);
    }
    pub fn opacity_of(&self, window: WindowId) -> i32 {
        self.opacity(window)
    }
    fn refused(&self) -> io::Result<()> {
        if self.fail {
            Err(io::Error::other("window change refused"))
        } else {
            Ok(())
        }
    }
}
impl PinnableWindows for FakeWindows {
    fn foreground(&self) -> Option<WindowId> {
        self.front
    }
    fn is_own(&self, window: WindowId) -> bool {
        self.windows.get(&window).is_some_and(|w| w.own)
    }
    fn eligible(&self, window: WindowId) -> bool {
        self.windows.get(&window).is_some_and(|w| w.eligible)
    }
    fn is_live(&self, window: WindowId) -> bool {
        self.windows.contains_key(&window)
    }
    fn pinned(&self) -> Vec<WindowId> {
        self.pins.clone()
    }
    fn pin(&mut self, window: WindowId) -> io::Result<()> {
        self.refused()?;
        if !self.pins.contains(&window) {
            self.pins.push(window);
        }
        Ok(())
    }
    fn unpin(&mut self, window: WindowId) -> io::Result<()> {
        self.refused()?;
        if self.fail_unpin == Some(window) {
            return Err(io::Error::other("unpin refused"));
        }
        self.pins.retain(|pin| *pin != window);
        // Ending a pin restores the window's own opacity.
        if let Some(restored) = self.windows.get_mut(&window) {
            restored.opacity = 100;
        }
        Ok(())
    }
    fn opacity(&self, window: WindowId) -> i32 {
        self.windows.get(&window).map_or(100, |w| w.opacity)
    }
    fn set_opacity(&mut self, window: WindowId, percent: i32) -> io::Result<()> {
        self.refused()?;
        if let Some(changed) = self.windows.get_mut(&window) {
            changed.opacity = percent;
        }
        Ok(())
    }
    fn title(&self, window: WindowId) -> String {
        self.windows
            .get(&window)
            .map(|w| w.title.clone())
            .unwrap_or_default()
    }
    fn play(&mut self, cue: Cue) {
        self.cues.push(cue);
    }
}

/// An in-memory folder of media files for MediaRenamer session tests. It
/// renames entries of its own table and nothing on disk.
#[derive(Default)]
pub struct FakeMedia {
    disk: std::sync::Mutex<FakeDisk>,
    scans: (std::sync::Mutex<bool>, std::sync::Condvar),
}
#[derive(Default)]
struct FakeDisk {
    files: BTreeMap<std::path::PathBuf, FakeFile>,
    /// Text files written through the port, the history file among them.
    texts: BTreeMap<std::path::PathBuf, String>,
    history_foreign: bool,
    /// Files another program holds open: their renames fail.
    locked: Vec<std::path::PathBuf>,
    /// Text writes that still succeed; `None` is no limit.
    writes_left: Option<usize>,
    writes: usize,
    ticks: i64,
}
#[derive(Clone)]
struct FakeFile {
    size: u64,
    modified: UtcTicks,
    capture: Option<powerkit_core::media_renamer::time::WallClock>,
}
impl FakeMedia {
    fn disk(&self) -> std::sync::MutexGuard<'_, FakeDisk> {
        self.disk.lock().unwrap()
    }
    /// Adds a file. `taken` is its capture time as `(y, m, d, h, min, s)`;
    /// `None` is a file without one, such as a sidecar.
    pub fn add(&self, path: &std::path::Path, taken: Option<(u16, u8, u8, u8, u8, u8)>) {
        let mut disk = self.disk();
        disk.ticks += 1;
        let modified = UtcTicks(disk.ticks);
        let capture = taken.and_then(|(y, m, d, h, min, s)| {
            powerkit_core::media_renamer::time::WallClock::new(y, m, d, h, min, s)
        });
        disk.files.insert(
            path.to_path_buf(),
            FakeFile {
                size: 100,
                modified,
                capture,
            },
        );
    }
    /// Changes a file behind the session's back, as another program would.
    pub fn touch(&self, path: &std::path::Path) {
        let mut disk = self.disk();
        disk.ticks += 1;
        let modified = UtcTicks(disk.ticks);
        if let Some(file) = disk.files.get_mut(path) {
            file.modified = modified;
        }
    }
    pub fn remove(&self, path: &std::path::Path) {
        self.disk().files.remove(path);
    }
    /// File names in `dir`, sorted.
    pub fn names(&self, dir: &std::path::Path) -> Vec<String> {
        self.disk()
            .files
            .keys()
            .filter(|path| path.parent() == Some(dir))
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect()
    }
    /// Makes the history file look like a file PowerKit did not write.
    pub fn make_history_foreign(&self) {
        self.disk().history_foreign = true;
    }
    /// Makes renaming `path` fail, as a file held open by another program.
    pub fn lock(&self, path: &std::path::Path) {
        self.disk().locked.push(path.to_path_buf());
    }
    /// Makes every text write fail, or succeed again.
    pub fn fail_writes(&self, fail: bool) {
        self.disk().writes_left = fail.then_some(0);
    }
    /// Lets `count` more text writes succeed and fails the ones after.
    pub fn fail_writes_after(&self, count: usize) {
        self.disk().writes_left = Some(count);
    }
    /// How many text writes succeeded.
    pub fn writes(&self) -> usize {
        self.disk().writes
    }
    /// Makes scans wait in `list` until [`FakeMedia::release_scans`].
    pub fn hold_scans(&self) {
        *self.scans.0.lock().unwrap() = true;
    }
    pub fn release_scans(&self) {
        *self.scans.0.lock().unwrap() = false;
        self.scans.1.notify_all();
    }
    fn member(
        path: &std::path::Path,
        file: &FakeFile,
    ) -> Option<powerkit_core::media_renamer::plan::Member> {
        use powerkit_core::media_renamer::plan::{Member, Probe, split_name};
        let (stem, suffix, kind) = split_name(&path.file_name()?.to_string_lossy())?;
        Some(Member {
            path: path.to_path_buf(),
            kind,
            stem,
            suffix,
            size: file.size,
            modified: file.modified,
            modified_local: None,
            companion: false,
            probe: Probe::Pending,
        })
    }
}

impl crate::ports::MediaFiles for FakeMedia {
    fn list(
        &self,
        inputs: &[std::path::PathBuf],
        _recurse: bool,
        known: &std::collections::HashSet<String>,
        room: usize,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> crate::controller::media_renamer::Listing {
        use crate::controller::media_renamer::lower;
        let mut held = self.scans.0.lock().unwrap();
        while *held && !cancel.load(std::sync::atomic::Ordering::Relaxed) {
            held = self
                .scans
                .1
                .wait_timeout(held, std::time::Duration::from_millis(10))
                .unwrap()
                .0;
        }
        drop(held);
        let disk = self.disk();
        let mut listing = crate::controller::media_renamer::Listing::default();
        let mut media = 0;
        for input in inputs {
            // An input is a file, or a folder whose direct children are listed.
            let folder = !disk.files.contains_key(input);
            let dir = if folder {
                input.as_path()
            } else {
                input.parent().unwrap_or(input)
            };
            for (path, file) in &disk.files {
                if path.parent() != Some(dir) {
                    continue;
                }
                listing.disk.insert(lower(path));
                if (!folder && path != input) || known.contains(&lower(path)) {
                    continue;
                }
                match Self::member(path, file) {
                    Some(member) if member.kind.is_media() && media == room => {
                        listing.truncated = true;
                    }
                    Some(member) => {
                        media += usize::from(member.kind.is_media());
                        listing.members.push(member);
                    }
                    None => listing.skipped += 1,
                }
            }
        }
        listing
    }
    fn folder_names(&self, dirs: &[std::path::PathBuf]) -> std::collections::HashSet<String> {
        self.disk()
            .files
            .keys()
            .filter(|path| dirs.iter().any(|dir| path.parent() == Some(dir.as_path())))
            .map(|path| crate::controller::media_renamer::lower(path))
            .collect()
    }
    fn prober(&self) -> Box<dyn crate::ports::MediaProber> {
        Box::new(FakeProber(self.disk().files.clone()))
    }
    fn fingerprint(&self, path: &std::path::Path) -> Option<(u64, UtcTicks)> {
        self.disk()
            .files
            .get(path)
            .map(|file| (file.size, file.modified))
    }
    fn exists(&self, path: &std::path::Path) -> bool {
        let disk = self.disk();
        disk.files.contains_key(path) || disk.texts.contains_key(path)
    }
    fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
        let mut disk = self.disk();
        if disk.locked.iter().any(|locked| locked == from) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the file is in use",
            ));
        }
        if disk.files.contains_key(to) || disk.texts.contains_key(to) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let file = disk.files.remove(from).ok_or(io::ErrorKind::NotFound)?;
        disk.files.insert(to.to_path_buf(), file);
        Ok(())
    }
    fn read_text(&self, path: &std::path::Path, limit: u64) -> io::Result<Option<String>> {
        let disk = self.disk();
        if disk.history_foreign {
            return Ok(Some("not ours\n".into()));
        }
        match disk.texts.get(path) {
            Some(text) if text.len() as u64 > limit => Err(io::Error::other("too large")),
            text => Ok(text.cloned()),
        }
    }
    fn replace_text(&self, path: &std::path::Path, text: &str) -> io::Result<()> {
        let mut disk = self.disk();
        match &mut disk.writes_left {
            Some(0) => return Err(io::Error::other("the disk is full")),
            Some(left) => *left -= 1,
            None => {}
        }
        disk.writes += 1;
        disk.texts.insert(path.to_path_buf(), text.to_owned());
        Ok(())
    }
    fn now_utc(&self) -> UtcTicks {
        let mut disk = self.disk();
        disk.ticks += 1;
        UtcTicks(1_000_000 + disk.ticks)
    }
    fn local_stamp(&self, at: UtcTicks) -> String {
        format!("at {}", at.0)
    }
}

/// Reads the capture times of a snapshot of the fake folder.
struct FakeProber(BTreeMap<std::path::PathBuf, FakeFile>);
impl crate::ports::MediaProber for FakeProber {
    fn probe(
        &self,
        path: &std::path::Path,
        _: powerkit_core::media_renamer::Kind,
    ) -> powerkit_core::media_renamer::plan::Probe {
        use powerkit_core::media_renamer::plan::{Metadata, Probe};
        use powerkit_core::media_renamer::time::{Capture, TimeSource};
        match self.0.get(path) {
            Some(file) => Probe::Read(Metadata {
                capture: file.capture.map(|time| Capture {
                    time,
                    offset: None,
                    source: TimeSource::ExifOriginal,
                }),
                make: None,
                model: None,
            }),
            None => Probe::Unreadable,
        }
    }
    fn dump(
        &self,
        _: &std::path::Path,
        _: powerkit_core::media_renamer::Kind,
    ) -> Vec<powerkit_core::media_renamer::exif::ExifTag> {
        Vec::new()
    }
    fn recheck(
        &self,
        path: &std::path::Path,
        kind: powerkit_core::media_renamer::Kind,
        known: (u64, UtcTicks),
        _readable: bool,
    ) -> crate::controller::media_renamer::Recheck {
        use crate::controller::media_renamer::Recheck;
        match self.0.get(path) {
            None => Recheck::Gone,
            Some(file) if (file.size, file.modified) == known => Recheck::Unchanged,
            Some(file) => Recheck::Changed {
                size: file.size,
                modified: file.modified,
                modified_local: None,
                probe: self.probe(path, kind),
            },
        }
    }
}

/// A package catalog a test scripts: what is installed, what a search finds,
/// and how each operation ends.
#[derive(Default)]
pub struct FakeCatalog {
    state: std::sync::Mutex<FakeCatalogState>,
    gate: (std::sync::Mutex<bool>, std::sync::Condvar),
}
#[derive(Default)]
struct FakeCatalogState {
    installed: Vec<powerkit_core::power_store::PackageSummary>,
    list_error: Option<crate::controller::power_store::CatalogError>,
    found: Vec<powerkit_core::power_store::PackageSummary>,
    failing: Vec<String>,
    ran: Vec<(powerkit_core::power_store::OperationKind, String)>,
    cancelled: usize,
    lists: usize,
    written: BTreeMap<std::path::PathBuf, String>,
}
impl FakeCatalog {
    fn state(&self) -> std::sync::MutexGuard<'_, FakeCatalogState> {
        self.state.lock().unwrap()
    }
    pub fn set_installed(&self, packages: Vec<powerkit_core::power_store::PackageSummary>) {
        self.state().installed = packages;
    }
    /// Makes every later listing fail with this error.
    pub fn fail_listing(&self, error: crate::controller::power_store::CatalogError) {
        self.state().list_error = Some(error);
    }
    pub fn set_found(&self, packages: Vec<powerkit_core::power_store::PackageSummary>) {
        self.state().found = packages;
    }
    /// Makes operations on this package id fail.
    pub fn fail_operations_on(&self, id: &str) {
        self.state().failing.push(id.into());
    }
    /// Operations that ran to their end, oldest first.
    pub fn ran(&self) -> Vec<(powerkit_core::power_store::OperationKind, String)> {
        self.state().ran.clone()
    }
    pub fn cancel_requests(&self) -> usize {
        self.state().cancelled
    }
    /// How many times the installed list was asked for.
    pub fn list_calls(&self) -> usize {
        self.state().lists
    }
    pub fn written(&self, path: &std::path::Path) -> Option<String> {
        self.state().written.get(path).cloned()
    }
    /// Makes operations wait before they finish, until released or cancelled.
    pub fn hold_operations(&self) {
        *self.gate.0.lock().unwrap() = true;
    }
    pub fn release_operations(&self) {
        *self.gate.0.lock().unwrap() = false;
        self.gate.1.notify_all();
    }
}
impl crate::ports::PackageCatalog for Arc<FakeCatalog> {
    fn manager_statuses(&self) -> Vec<crate::controller::power_store::ManagerStatus> {
        vec![crate::controller::power_store::ManagerStatus {
            kind: powerkit_core::power_store::ManagerKind::WinGet,
            presence: crate::controller::power_store::ManagerPresence::Available,
        }]
    }
    fn list_installed(
        &self,
    ) -> Result<
        crate::controller::power_store::InstalledList,
        crate::controller::power_store::CatalogError,
    > {
        let mut state = self.state();
        state.lists += 1;
        if let Some(error) = &state.list_error {
            return Err(error.clone());
        }
        Ok(crate::controller::power_store::InstalledList {
            packages: state.installed.clone(),
            managers: self.manager_statuses(),
            store_managed: Default::default(),
        })
    }
    fn search(
        &self,
        query: &str,
        _source: Option<&str>,
    ) -> Result<
        Vec<powerkit_core::power_store::PackageSummary>,
        crate::controller::power_store::CatalogError,
    > {
        let needle = query.to_lowercase();
        Ok(self
            .state()
            .found
            .iter()
            .filter(|package| package.name.to_lowercase().contains(&needle))
            .cloned()
            .collect())
    }
    fn details(
        &self,
        key: &powerkit_core::power_store::PackageKey,
    ) -> Result<
        powerkit_core::power_store::PackageDetails,
        crate::controller::power_store::CatalogError,
    > {
        Err(crate::controller::power_store::CatalogError::Failed(
            format!("no details for {}", key.id),
        ))
    }
    fn probe_manager(
        &self,
        _: powerkit_core::power_store::ManagerKind,
    ) -> crate::controller::power_store::ManagerProbe {
        crate::controller::power_store::ManagerProbe {
            path: Some("fake-manager".into()),
            version: Some("1.0".into()),
        }
    }
    fn prepare(
        &self,
        operation: powerkit_core::power_store::QueuedOperation,
    ) -> crate::controller::power_store::Prepared {
        use crate::controller::power_store::CatalogError;
        use powerkit_core::power_store::OperationKind;
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = cancelled.clone();
        let (catalog, canceller) = (self.clone(), self.clone());
        crate::controller::power_store::Prepared {
            cancel: Arc::new(move || {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
                canceller.state().cancelled += 1;
                canceller.gate.1.notify_all();
            }),
            run: Box::new(move |_progress| {
                let mut held = catalog.gate.0.lock().unwrap();
                while *held && !cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    held = catalog
                        .gate
                        .1
                        .wait_timeout(held, std::time::Duration::from_millis(10))
                        .unwrap()
                        .0;
                }
                drop(held);
                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err(CatalogError::Cancelled);
                }
                let mut state = catalog.state();
                if state.failing.contains(&operation.key.id) {
                    return Err(CatalogError::Failed(format!(
                        "{} could not be changed",
                        operation.key.id
                    )));
                }
                // The fake machine changes like a real one would.
                match operation.kind {
                    OperationKind::Uninstall => {
                        state
                            .installed
                            .retain(|package| package.key != operation.key);
                    }
                    OperationKind::Update => {
                        if let Some(package) = state
                            .installed
                            .iter_mut()
                            .find(|package| package.key == operation.key)
                        {
                            package.update_available = false;
                            package.version = package.available_version.clone();
                        }
                    }
                    OperationKind::Install => {
                        if let Some(index) = state
                            .found
                            .iter()
                            .position(|package| package.key == operation.key)
                        {
                            let mut package = state.found.remove(index);
                            package.version = package.available_version.clone();
                            state.installed.push(package);
                        }
                    }
                }
                state.ran.push((operation.kind, operation.key.id.clone()));
                Ok(())
            }),
        }
    }
    fn cached_icon(&self, _: &str) -> Option<Arc<[u8]>> {
        None
    }
    fn resolve_icons(
        &self,
        _: &[powerkit_core::power_store::PackageSummary],
    ) -> Vec<(powerkit_core::power_store::PackageKey, String)> {
        Vec::new()
    }
    fn fetch_icons(&self, _: &[String], _: &(dyn Fn() + Sync)) {}
    fn write_text(&self, path: &std::path::Path, text: &str) -> io::Result<()> {
        self.state().written.insert(path.to_path_buf(), text.into());
        Ok(())
    }
    fn now_local_stamp(&self) -> String {
        "2026-11-23 10:00".into()
    }
    fn now_iso_utc(&self) -> String {
        "2026-11-23T07:00:00.000-00:00".into()
    }
}

/// A mouse that answers HID++ 2.0 requests from its own state, shaped like an
/// MX Master: a test changes the state, removes features or makes it fail.
pub struct FakeMouse {
    pub name: String,
    pub is_mouse: bool,
    /// Percent and charging status byte; `None` reports a battery fault.
    pub battery: Option<(u8, u8)>,
    /// Control id, divertable, raw XY.
    pub controls: Vec<(u16, bool, bool)>,
    /// Diverted and raw-XY state per control id, as last set.
    pub diverted: BTreeMap<u16, (bool, bool)>,
    pub dpi: u16,
    /// The DPI list words after the sensor index, as the device reports them.
    pub dpi_list: Vec<u16>,
    /// Wheel mode byte (1 free spin, 2 ratchet) and release speed, as last set.
    pub wheel: (u8, u8),
    pub auto_disengage: u8,
    /// High-resolution wheel mode flags; bit 1 is the resolution.
    pub hires_mode: u8,
    pub thumb_diverted: bool,
    pub thumb_positive_right: bool,
    /// The host the device was told to move to.
    pub host: Option<u8>,
    pub haptic_taps: usize,
    /// Feature ids in table order; a feature's index is its position plus one.
    pub features: Vec<u16>,
    /// `(feature id, function)` calls the device answers with an error code.
    pub refuse: Vec<(u16, u8)>,
    /// The device does not answer: every request times out.
    pub silent: bool,
    /// The device left: the channel fails.
    pub gone: bool,
    /// Every request and notification received, as `(feature id, function)`.
    pub log: Vec<(u16, u8)>,
}
impl Default for FakeMouse {
    fn default() -> Self {
        use powerkit_core::power_mouse::hidpp::{control, feature};
        Self {
            name: "MX Master 4".into(),
            is_mouse: true,
            battery: Some((80, 0)),
            controls: vec![
                (control::GESTURE_BUTTON, true, true),
                (control::MODE_SHIFT, true, false),
                (control::HAPTIC_PANEL, true, false),
                // An ordinary button the device keeps to itself.
                (0x0050, false, false),
            ],
            diverted: BTreeMap::new(),
            dpi: 1000,
            // 400 to 8000 in steps of 50.
            dpi_list: vec![400, 0xE000 | 50, 8000],
            wheel: (2, 10),
            auto_disengage: 10,
            hires_mode: 0b001,
            thumb_diverted: false,
            thumb_positive_right: false,
            host: None,
            haptic_taps: 0,
            features: vec![
                feature::DEVICE_NAME,
                feature::UNIFIED_BATTERY,
                feature::REPROG_CONTROLS,
                feature::ADJUSTABLE_DPI,
                feature::SMARTSHIFT_ENHANCED,
                feature::HIRES_WHEEL,
                feature::THUMB_WHEEL,
                feature::CHANGE_HOST,
                feature::HAPTIC_FEEDBACK,
            ],
            refuse: Vec::new(),
            silent: false,
            gone: false,
            log: Vec::new(),
        }
    }
}
impl FakeMouse {
    /// The same mouse without one feature.
    pub fn without(mut self, feature_id: u16) -> Self {
        self.features.retain(|id| *id != feature_id);
        self
    }
    /// The index the device gives a feature; 0 is the root, `None` is absent.
    pub fn index_of(&self, feature_id: u16) -> Option<u8> {
        self.features
            .iter()
            .position(|id| *id == feature_id)
            .map(|position| position as u8 + 1)
    }
    /// How many times `(feature id, function)` was called.
    pub fn calls(&self, feature_id: u16, function: u8) -> usize {
        self.log
            .iter()
            .filter(|entry| **entry == (feature_id, function))
            .count()
    }
    fn feature_at(&self, index: u8) -> Option<u16> {
        match index {
            0 => Some(0),
            index => self.features.get(usize::from(index) - 1).copied(),
        }
    }
    fn failure(&self) -> Result<(), crate::controller::power_mouse::RequestError> {
        if self.gone {
            return Err(io::Error::other("device left").into());
        }
        if self.silent {
            return Err(crate::controller::power_mouse::RequestError::Timeout);
        }
        Ok(())
    }
}
impl crate::ports::HidppChannel for FakeMouse {
    fn request(
        &mut self,
        request: &powerkit_core::power_mouse::hidpp::Request,
    ) -> Result<[u8; 16], crate::controller::power_mouse::RequestError> {
        use crate::controller::power_mouse::RequestError;
        use powerkit_core::power_mouse::hidpp::{DeviceError, feature};
        self.failure()?;
        let unknown = || RequestError::Device(DeviceError(2));
        let id = self.feature_at(request.feature_index).ok_or_else(unknown)?;
        self.log.push((id, request.function));
        if self.refuse.contains(&(id, request.function)) {
            return Err(RequestError::Device(DeviceError(5)));
        }
        let p = request.params;
        let mut reply = [0u8; 16];
        match (id, request.function) {
            // Root: where a feature lives, and the ping echo.
            (0, 0) => {
                reply[0] = self.index_of(u16::from_be_bytes([p[0], p[1]])).unwrap_or(0);
            }
            (0, 1) => reply = p,
            (feature::DEVICE_NAME, 0) => reply[0] = self.name.len() as u8,
            (feature::DEVICE_NAME, 1) => {
                let bytes = self.name.as_bytes();
                let from = usize::from(p[0]).min(bytes.len());
                let chunk = &bytes[from..bytes.len().min(from + 16)];
                reply[..chunk.len()].copy_from_slice(chunk);
            }
            (feature::DEVICE_NAME, 2) => reply[0] = if self.is_mouse { 3 } else { 0 },
            (feature::UNIFIED_BATTERY, 1) => match self.battery {
                Some((percent, status)) => {
                    reply[0] = percent;
                    reply[2] = status;
                }
                // Status 5 and above are battery faults.
                None => reply[2] = 5,
            },
            (feature::REPROG_CONTROLS, 0) => reply[0] = self.controls.len() as u8,
            (feature::REPROG_CONTROLS, 1) => {
                let (control, divertable, raw_xy) =
                    *self.controls.get(usize::from(p[0])).ok_or_else(unknown)?;
                reply[..2].copy_from_slice(&control.to_be_bytes());
                reply[4] = u8::from(divertable) << 5;
                reply[8] = u8::from(raw_xy);
            }
            (feature::REPROG_CONTROLS, 3) => {
                let control = u16::from_be_bytes([p[0], p[1]]);
                let state = self.diverted.entry(control).or_default();
                // Each setting is a value bit plus a "valid" bit.
                if p[2] & 1 << 1 != 0 {
                    state.0 = p[2] & 1 != 0;
                }
                if p[2] & 1 << 5 != 0 {
                    state.1 = p[2] & 1 << 4 != 0;
                }
                reply = p;
            }
            (feature::ADJUSTABLE_DPI, 1) => {
                let slots = reply[1..].as_chunks_mut::<2>().0;
                for (slot, word) in slots.iter_mut().zip(&self.dpi_list) {
                    *slot = word.to_be_bytes();
                }
            }
            (feature::ADJUSTABLE_DPI, 2) => reply[1..3].copy_from_slice(&self.dpi.to_be_bytes()),
            (feature::ADJUSTABLE_DPI, 3) => {
                self.dpi = u16::from_be_bytes([p[1], p[2]]);
                reply = p;
            }
            (feature::SMARTSHIFT_ENHANCED, 0) => reply[1] = self.auto_disengage,
            (feature::SMARTSHIFT_ENHANCED, 1) => {
                reply[0] = self.wheel.0;
                reply[1] = self.wheel.1;
            }
            (feature::SMARTSHIFT_ENHANCED, 2) => {
                // Zero keeps a value.
                if p[0] != 0 {
                    self.wheel.0 = p[0];
                }
                if p[1] != 0 {
                    self.wheel.1 = p[1];
                }
                reply = p;
            }
            (feature::HIRES_WHEEL, 1) => reply[0] = self.hires_mode,
            (feature::HIRES_WHEEL, 2) => {
                self.hires_mode = p[0];
                reply = p;
            }
            (feature::THUMB_WHEEL, 0) => reply[4] = u8::from(self.thumb_positive_right),
            (feature::THUMB_WHEEL, 1) => reply[0] = u8::from(self.thumb_diverted),
            (feature::THUMB_WHEEL, 2) => {
                self.thumb_diverted = p[0] != 0;
                reply = p;
            }
            (feature::HAPTIC_FEEDBACK, 4) => self.haptic_taps += 1,
            _ => return Err(unknown()),
        }
        Ok(reply)
    }
    fn notify(&mut self, request: &powerkit_core::power_mouse::hidpp::Request) -> io::Result<()> {
        use powerkit_core::power_mouse::hidpp::feature;
        if self.gone {
            return Err(io::Error::other("device left"));
        }
        let id = self.feature_at(request.feature_index).unwrap_or(0);
        self.log.push((id, request.function));
        if (id, request.function) == (feature::CHANGE_HOST, 1) {
            self.host = Some(request.params[0]);
        }
        Ok(())
    }
}
