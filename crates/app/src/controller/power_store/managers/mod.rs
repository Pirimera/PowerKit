//! The package managers PowerStore drives through their command-line
//! programs, and the fan-out that asks all of them at once.
//!
//! Each adapter builds the manager's commands and reads its output with the
//! parsers in `powerkit-core`; none of that depends on an operating system.
//! What does is starting a program and ending its process tree, which is the
//! [`Programs`] port. The platform's own package service (WinGet on Windows)
//! is not a command-line program; it joins the fan-out through
//! [`NativePackages`].
//!
//! Everything here runs on session worker threads. A manager the user
//! switched off is located but never started.
use super::{CatalogError, InstalledList, ManagerPresence, ManagerProbe, ManagerStatus};
use powerkit_core::power_store::{
    ManagerKind, ManagerSettings, NO_MANAGERS_AVAILABLE, NO_SEARCH_ANSWER, OperationProgress,
    PackageDetails, PackageKey, PackageSummary, PowerStorePreferences, ProgressStage,
    QueuedOperation,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

mod cargo_mgr;
mod chocolatey;
mod dotnet_tool;
mod npm;
mod pip;
mod powershell_mgr;
mod probe;
mod scoop;
mod vcpkg;

/// Starting package-manager programs.
pub trait Programs: Send + Sync {
    /// The first program called `name` on the search path that the platform
    /// can start directly.
    fn find(&self, name: &str) -> Option<PathBuf>;
    /// An environment variable as the programs would see it. Adapters read
    /// the environment only through this, so a manager's home is found the
    /// same way the manager finds it.
    fn var(&self, name: &str) -> Option<std::ffi::OsString>;
    /// Runs the program to its end and returns what it wrote to standard
    /// output; a failed exit is an error carrying its error output. With a
    /// `cancel` the run has no time limit and ends, with everything the
    /// program started, when cancel is requested. Without one it is a query,
    /// which the platform bounds in time.
    fn run(
        &self,
        program: &Path,
        args: &[&str],
        cancel: Option<&Cancel>,
    ) -> Result<String, CatalogError>;
}

/// The platform's own package service, asked beside the command-line
/// managers under one manager kind.
pub trait NativePackages: Send + Sync {
    fn kind(&self) -> ManagerKind;
    /// Source names that belong to this service.
    fn sources(&self) -> &'static [&'static str];
    /// Whether the service looks installed, without calling it.
    fn seems_installed(&self) -> bool;
    fn list_installed(&self) -> Result<Vec<PackageSummary>, CatalogError>;
    /// `source` is one of [`Self::sources`], or `None` for all of them.
    fn search(
        &self,
        query: &str,
        source: Option<&str>,
    ) -> Result<Vec<PackageSummary>, CatalogError>;
    fn details(&self, key: &PackageKey) -> Result<PackageDetails, CatalogError>;
    /// How a listing that failed as unavailable is shown: missing, or
    /// present but too old to answer.
    fn presence_when_unavailable(&self, message: &str) -> ManagerPresence;
    /// Runs after a successful listing: the platform may prepare local logos
    /// and returns the rows its own store delivers.
    fn after_listing(&self, packages: &[PackageSummary]) -> BTreeSet<PackageKey>;
}

/// A request to stop a running operation. The session keeps one end; the
/// platform attaches how to end the program once it has started one.
#[derive(Clone, Default)]
pub struct Cancel(Arc<CancelState>);
#[derive(Default)]
struct CancelState {
    requested: AtomicBool,
    stop: Mutex<Option<Box<dyn FnMut() + Send>>>,
}
impl Cancel {
    /// Asks the operation to stop. A request made before a program runs is
    /// remembered.
    pub fn cancel(&self) {
        self.0.requested.store(true, Ordering::SeqCst);
        let mut stop = self
            .0
            .stop
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(stop) = stop.as_mut() {
            stop();
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.requested.load(Ordering::SeqCst)
    }
    /// Attaches how to end the running program. If cancel was already
    /// requested, `stop` runs at once.
    pub fn attach(&self, mut stop: impl FnMut() + Send + 'static) {
        let mut slot = self
            .0
            .stop
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if self.is_cancelled() {
            stop();
        }
        *slot = Some(Box::new(stop));
    }
    /// The program ended; a later cancel has nothing to end.
    pub fn detach(&self) {
        *self
            .0
            .stop
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = None;
    }
}

/// What an adapter sees while it answers one request: the programs port and
/// the manager settings as they were when the request started.
pub(crate) struct Cx<'a> {
    programs: &'a dyn Programs,
    settings: BTreeMap<ManagerKind, ManagerSettings>,
}
impl Cx<'_> {
    fn settings(&self, kind: ManagerKind) -> ManagerSettings {
        self.settings.get(&kind).cloned().unwrap_or_default()
    }
    pub(crate) fn find(&self, name: &str) -> Option<PathBuf> {
        self.programs.find(name)
    }
    pub(crate) fn var(&self, name: &str) -> Option<std::ffi::OsString> {
        self.programs.var(name)
    }
    /// A query: bounded in time, not cancellable.
    pub(crate) fn run_output(&self, program: &Path, args: &[&str]) -> Result<String, CatalogError> {
        self.programs.run(program, args, None)
    }
    /// An operation: runs until it ends or `cancel` is requested.
    pub(crate) fn run_collect(
        &self,
        program: &Path,
        args: &[&str],
        cancel: Option<&Cancel>,
        on_progress: Option<&mut dyn FnMut(OperationProgress)>,
    ) -> Result<String, CatalogError> {
        if let Some(on_progress) = on_progress {
            on_progress(OperationProgress {
                stage: ProgressStage::Installing,
                percent: None,
            });
        }
        // A missing handle still means "no time limit" for an operation.
        let unlimited = Cancel::default();
        self.programs
            .run(program, args, Some(cancel.unwrap_or(&unlimited)))
    }
    pub(crate) fn is_enabled(&self, kind: ManagerKind) -> bool {
        self.settings(kind).enabled
    }
    /// The program a manager runs: nothing when the manager is off, the
    /// user's chosen file when it exists (a missing custom file is reported
    /// as "not found", never replaced silently by another install),
    /// otherwise whatever `discover` finds.
    pub(crate) fn resolve_program(
        &self,
        kind: ManagerKind,
        discover: impl FnOnce() -> Option<PathBuf>,
    ) -> Option<PathBuf> {
        resolve(&self.settings(kind), discover)
    }
    /// Where the manager's program is, whatever its switch says. Settings
    /// show this next to the switch, so it only looks for the file and never
    /// runs it.
    pub(crate) fn locate_program(
        &self,
        kind: ManagerKind,
        discover: impl FnOnce() -> Option<PathBuf>,
    ) -> Option<PathBuf> {
        let settings = ManagerSettings {
            enabled: true,
            ..self.settings(kind)
        };
        resolve(&settings, discover)
    }
    /// vcpkg triplet to build for when a package name carries none.
    pub(crate) fn default_triplet(&self, kind: ManagerKind) -> Option<String> {
        self.settings(kind).triplet
    }
    /// The user's custom program, if one is set (callers gate on the switch).
    pub(crate) fn custom_executable(&self, kind: ManagerKind) -> Option<PathBuf> {
        self.settings(kind).executable.map(PathBuf::from)
    }
}
fn resolve(
    settings: &ManagerSettings,
    discover: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    if !settings.enabled {
        return None;
    }
    match settings.executable.as_deref() {
        Some(path) => {
            let path = PathBuf::from(path);
            path.is_file().then_some(path)
        }
        None => discover(),
    }
}

/// Source names that are a manager, not a Scoop bucket.
const CLI_SOURCES: &[&str] = &[
    "chocolatey",
    "scoop",
    "npm",
    "pip",
    "cargo",
    "dotnet",
    "powershell",
    "powershell7",
    "vcpkg",
];

/// One manager's answer. `Ok(None)` means switched off or program absent; an
/// error means it is present but failed.
type Answer = Result<Option<Vec<PackageSummary>>, CatalogError>;
type Job<'a> = Box<dyn FnOnce() -> Answer + Send + 'a>;
fn job<'a>(run: impl FnOnce() -> Answer + Send + 'a) -> Job<'a> {
    Box::new(run)
}

/// Every package manager of the machine behind one set of questions.
pub struct Managers {
    programs: Box<dyn Programs>,
    native: Box<dyn NativePackages>,
    /// Preferences live on the main thread; workers read this copy, which
    /// the shell replaces at startup and after every manager edit.
    settings: RwLock<BTreeMap<ManagerKind, ManagerSettings>>,
}
impl Managers {
    pub fn new(programs: Box<dyn Programs>, native: Box<dyn NativePackages>) -> Self {
        Self {
            programs,
            native,
            settings: RwLock::new(BTreeMap::new()),
        }
    }
    /// Replaces the manager settings workers see with those in `prefs`.
    pub fn configure(&self, prefs: &PowerStorePreferences) {
        let mut settings = self
            .settings
            .write()
            .unwrap_or_else(|poison| poison.into_inner());
        *settings = prefs.managers.clone();
    }
    fn cx(&self) -> Cx<'_> {
        Cx {
            programs: &*self.programs,
            settings: self
                .settings
                .read()
                .unwrap_or_else(|poison| poison.into_inner())
                .clone(),
        }
    }
    pub fn is_enabled(&self, kind: ManagerKind) -> bool {
        self.cx().is_enabled(kind)
    }

    /// Which managers are installed, without starting any of them.
    pub fn statuses(&self) -> Vec<ManagerStatus> {
        let cx = &self.cx();
        let status = |kind, available: bool| ManagerStatus {
            kind,
            presence: if available {
                ManagerPresence::Available
            } else {
                ManagerPresence::Missing
            },
        };
        vec![
            // A listing refines this with the service's real answer.
            status(self.native.kind(), self.native.seems_installed()),
            status(ManagerKind::Scoop, scoop::is_available(cx)),
            status(ManagerKind::Chocolatey, chocolatey::is_available(cx)),
            status(ManagerKind::Npm, npm::is_available(cx)),
            status(ManagerKind::Pip, pip::is_available(cx)),
            status(ManagerKind::Cargo, cargo_mgr::is_available(cx)),
            status(ManagerKind::DotnetTool, dotnet_tool::is_available(cx)),
            status(
                ManagerKind::PowerShell,
                powershell_mgr::is_available(cx, ManagerKind::PowerShell),
            ),
            status(
                ManagerKind::PowerShell7,
                powershell_mgr::is_available(cx, ManagerKind::PowerShell7),
            ),
            status(ManagerKind::Vcpkg, vcpkg::is_available(cx)),
        ]
    }

    /// Installed packages of every enabled manager, asked at the same time.
    pub fn list_installed(&self) -> Result<InstalledList, CatalogError> {
        let cx = &self.cx();
        let native = self.native.kind();
        let mut packages = Vec::new();
        let mut statuses = self.statuses();
        let mut any_ok = false;
        let mut last_error: Option<CatalogError> = None;

        // A switched-off native service is not called at all.
        let mut jobs: Vec<(ManagerKind, Job<'_>)> = Vec::new();
        if cx.is_enabled(native) {
            jobs.push((native, job(|| self.native.list_installed().map(Some))));
        }
        jobs.extend([
            (ManagerKind::Scoop, job(|| scoop::list_installed(cx))),
            (
                ManagerKind::Chocolatey,
                job(|| chocolatey::list_installed(cx)),
            ),
            (ManagerKind::Npm, job(|| npm::list_installed(cx))),
            (ManagerKind::Pip, job(|| pip::list_installed(cx))),
            (ManagerKind::Cargo, job(|| cargo_mgr::list_installed(cx))),
            (
                ManagerKind::DotnetTool,
                job(|| dotnet_tool::list_installed(cx)),
            ),
            (
                ManagerKind::PowerShell,
                job(|| powershell_mgr::list_installed(cx, ManagerKind::PowerShell)),
            ),
            (
                ManagerKind::PowerShell7,
                job(|| powershell_mgr::list_installed(cx, ManagerKind::PowerShell7)),
            ),
            (ManagerKind::Vcpkg, job(|| vcpkg::list_installed(cx))),
        ]);

        for (kind, answer) in gather(jobs) {
            match answer {
                Ok(Some(rows)) => {
                    packages.extend(rows);
                    any_ok = true;
                    set_presence(&mut statuses, kind, ManagerPresence::Available);
                }
                // A switched-off adapter also answers `None`; keep the
                // located-program status.
                Ok(None) if kind != native && cx.is_enabled(kind) => {
                    set_presence(&mut statuses, kind, ManagerPresence::Missing);
                }
                Ok(None) => {}
                // The native service keeps its own presence rules: one that
                // is missing or too old is reported as such instead of as a
                // generic failure.
                Err(CatalogError::Unavailable(message)) if kind == native => {
                    let presence = self.native.presence_when_unavailable(&message);
                    set_presence(&mut statuses, kind, presence);
                    last_error = Some(CatalogError::Unavailable(message));
                }
                Err(error) => {
                    if kind != native {
                        set_presence(&mut statuses, kind, ManagerPresence::Available);
                    }
                    last_error = Some(error);
                }
            }
        }

        let all_disabled = statuses.iter().all(|status| !cx.is_enabled(status.kind));
        if any_ok || all_disabled {
            let store_managed = self.native.after_listing(&packages);
            Ok(InstalledList {
                packages,
                managers: statuses,
                store_managed,
            })
        } else {
            Err(last_error
                .unwrap_or_else(|| CatalogError::Unavailable(NO_MANAGERS_AVAILABLE.into())))
        }
    }

    /// Packages matching `query`; `source` narrows the search to one source.
    pub fn search(
        &self,
        query: &str,
        source: Option<&str>,
    ) -> Result<Vec<PackageSummary>, CatalogError> {
        let cx = &self.cx();
        let native = self.native.kind();
        let source = source.map(str::trim).filter(|value| !value.is_empty());
        let all = source.is_none_or(|value| value.eq_ignore_ascii_case("all"));
        // Every manager whose source name is the chosen one (or all of them)
        // is asked at once.
        let wanted = |kind: ManagerKind| {
            all || source.is_some_and(|value| value.eq_ignore_ascii_case(kind.as_str()))
        };
        let is_native_source = |value: &str| {
            self.native
                .sources()
                .iter()
                .any(|name| value.eq_ignore_ascii_case(name))
        };

        let mut jobs: Vec<(ManagerKind, Job<'_>)> = Vec::new();
        let native_source = source.filter(|value| is_native_source(value));
        if cx.is_enabled(native) && (all || native_source.is_some()) {
            jobs.push((
                native,
                job(move || self.native.search(query, native_source).map(Some)),
            ));
        }
        // Scoop also answers for bucket names, which are not fixed sources.
        let is_fixed = |value: &str| {
            is_native_source(value)
                || CLI_SOURCES
                    .iter()
                    .any(|name| value.eq_ignore_ascii_case(name))
        };
        let want_scoop = all
            || source.is_some_and(|value| value.eq_ignore_ascii_case("scoop"))
            || source.is_some_and(|value| !is_fixed(value));
        if want_scoop {
            jobs.push((
                ManagerKind::Scoop,
                job(move || {
                    scoop::search(cx, query)
                        .map(|rows| rows.map(|rows| scoop_bucket_rows(rows, source)))
                }),
            ));
        }
        if wanted(ManagerKind::Chocolatey) {
            jobs.push((
                ManagerKind::Chocolatey,
                job(move || chocolatey::search(cx, query)),
            ));
        }
        if wanted(ManagerKind::Npm) {
            jobs.push((ManagerKind::Npm, job(move || npm::search(cx, query))));
        }
        if wanted(ManagerKind::Pip) {
            jobs.push((ManagerKind::Pip, job(move || pip::search(cx, query))));
        }
        if wanted(ManagerKind::Cargo) {
            jobs.push((
                ManagerKind::Cargo,
                job(move || cargo_mgr::search(cx, query)),
            ));
        }
        if wanted(ManagerKind::DotnetTool) {
            jobs.push((
                ManagerKind::DotnetTool,
                job(move || dotnet_tool::search(cx, query)),
            ));
        }
        for kind in [ManagerKind::PowerShell, ManagerKind::PowerShell7] {
            if wanted(kind) {
                jobs.push((kind, job(move || powershell_mgr::search(cx, kind, query))));
            }
        }
        if wanted(ManagerKind::Vcpkg) {
            jobs.push((ManagerKind::Vcpkg, job(move || vcpkg::search(cx, query))));
        }

        let mut packages = Vec::new();
        let mut any_ok = false;
        let mut last_error: Option<CatalogError> = None;
        for (_, answer) in gather(jobs) {
            match answer {
                Ok(Some(rows)) => {
                    packages.extend(rows);
                    any_ok = true;
                }
                Ok(None) => {}
                Err(error) => last_error = Some(error),
            }
        }
        if any_ok {
            Ok(packages)
        } else {
            Err(last_error.unwrap_or_else(|| CatalogError::Failed(NO_SEARCH_ANSWER.into())))
        }
    }

    pub fn details(&self, key: &PackageKey) -> Result<PackageDetails, CatalogError> {
        let cx = &self.cx();
        if key.manager == self.native.kind() {
            if !cx.is_enabled(key.manager) {
                return Err(native_disabled(key.manager));
            }
            return self.native.details(key);
        }
        match key.manager {
            ManagerKind::Scoop => scoop::details(cx, key),
            ManagerKind::Chocolatey => chocolatey::details(cx, key),
            ManagerKind::Npm => npm::details(cx, key),
            ManagerKind::Pip => pip::details(cx, key),
            ManagerKind::Cargo => cargo_mgr::details(cx, key),
            ManagerKind::DotnetTool => dotnet_tool::details(key),
            ManagerKind::PowerShell | ManagerKind::PowerShell7 => {
                powershell_mgr::details(cx, key.manager, key)
            }
            ManagerKind::Vcpkg => vcpkg::details(cx, key),
            ManagerKind::WinGet => Err(native_disabled(key.manager)),
        }
    }

    /// Runs an install, update or uninstall of a command-line manager. The
    /// native service runs its own operations; `None` says the operation is
    /// its to run, unless it is switched off.
    pub fn run_operation(
        &self,
        operation: &QueuedOperation,
        cancel: &Cancel,
        on_progress: impl FnMut(OperationProgress),
    ) -> Option<Result<(), CatalogError>> {
        let cx = &self.cx();
        let kind = operation.key.manager;
        if kind == self.native.kind() {
            return (!cx.is_enabled(kind)).then(|| Err(native_disabled(kind)));
        }
        Some(match kind {
            ManagerKind::Scoop => scoop::run_operation(cx, operation, cancel, on_progress),
            ManagerKind::Chocolatey => {
                chocolatey::run_operation(cx, operation, cancel, on_progress)
            }
            ManagerKind::Npm => npm::run_operation(cx, operation, cancel, on_progress),
            ManagerKind::Pip => pip::run_operation(cx, operation, cancel, on_progress),
            ManagerKind::Cargo => cargo_mgr::run_operation(cx, operation, cancel, on_progress),
            ManagerKind::DotnetTool => {
                dotnet_tool::run_operation(cx, operation, cancel, on_progress)
            }
            ManagerKind::PowerShell | ManagerKind::PowerShell7 => {
                powershell_mgr::run_operation(cx, kind, operation, cancel, on_progress)
            }
            ManagerKind::Vcpkg => vcpkg::run_operation(cx, operation, cancel, on_progress),
            ManagerKind::WinGet => Err(native_disabled(kind)),
        })
    }

    /// Where a manager's program is and which version it reports.
    pub fn probe(&self, kind: ManagerKind) -> ManagerProbe {
        probe::probe(&self.cx(), kind)
    }
}

fn native_disabled(kind: ManagerKind) -> CatalogError {
    CatalogError::Unavailable(format!(
        "{} is turned off in PowerStore settings",
        kind.as_str()
    ))
}

/// Runs every job on its own scoped thread and returns the answers in input
/// order, so the slowest manager sets the latency while the merge order stays
/// fixed. A job that panics reports an error for its own manager only.
fn gather(jobs: Vec<(ManagerKind, Job<'_>)>) -> Vec<(ManagerKind, Answer)> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .into_iter()
            .map(|(kind, run)| (kind, scope.spawn(run)))
            .collect();
        handles
            .into_iter()
            .map(|(kind, handle)| {
                let answer = handle.join().unwrap_or_else(|_| {
                    Err(CatalogError::Failed(format!(
                        "{} worker stopped",
                        kind.as_str()
                    )))
                });
                (kind, answer)
            })
            .collect()
    })
}

/// Scoop answers every bucket at once; a bucket name as the source keeps only
/// that bucket.
fn scoop_bucket_rows(rows: Vec<PackageSummary>, source: Option<&str>) -> Vec<PackageSummary> {
    match source {
        Some(value)
            if !value.eq_ignore_ascii_case("all") && !value.eq_ignore_ascii_case("scoop") =>
        {
            rows.into_iter()
                .filter(|package| package.key.source.eq_ignore_ascii_case(value))
                .collect()
        }
        _ => rows,
    }
}

fn set_presence(statuses: &mut [ManagerStatus], kind: ManagerKind, presence: ManagerPresence) {
    if let Some(status) = statuses.iter_mut().find(|status| status.kind == kind) {
        status.presence = presence;
    }
}

#[cfg(test)]
mod tests;
