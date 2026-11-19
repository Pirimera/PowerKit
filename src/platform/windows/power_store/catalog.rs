//! Fan-out across WinGet (broker child) and CLI managers (Scoop through vcpkg).
use super::broker::{self, CancelHandle};
use super::cargo_mgr;
use super::chocolatey;
use super::cli_process::find_on_path;
use super::dotnet_tool;
use super::icons;
use super::manager_config;
use super::npm;
use super::pip;
use super::powershell_mgr;
use super::scoop;
use super::store_managed;
use super::vcpkg;
use super::winget::ManagerError;
use crate::power_store::{
    ManagerKind, NO_MANAGERS_AVAILABLE, NO_SEARCH_ANSWER, OperationProgress, PackageDetails,
    PackageKey, PackageSummary, QueuedOperation,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagerPresence {
    Available,
    Missing,
    TooOld,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagerStatus {
    pub kind: ManagerKind,
    pub presence: ManagerPresence,
}

const FIXED_SOURCES: &[&str] = &[
    "winget",
    "msstore",
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

/// Snapshot of which phase-3 managers the host can see right now.
pub fn manager_statuses() -> Vec<ManagerStatus> {
    vec![
        // App Installer's `winget` alias; a listing refines this with the real COM result.
        status(ManagerKind::WinGet, || find_on_path("winget").is_some()),
        status(ManagerKind::Scoop, scoop::is_available),
        status(ManagerKind::Chocolatey, chocolatey::is_available),
        status(ManagerKind::Npm, npm::is_available),
        status(ManagerKind::Pip, pip::is_available),
        status(ManagerKind::Cargo, cargo_mgr::is_available),
        status(ManagerKind::DotnetTool, dotnet_tool::is_available),
        status(ManagerKind::PowerShell, || {
            powershell_mgr::is_available(ManagerKind::PowerShell)
        }),
        status(ManagerKind::PowerShell7, || {
            powershell_mgr::is_available(ManagerKind::PowerShell7)
        }),
        status(ManagerKind::Vcpkg, vcpkg::is_available),
    ]
}

/// Installed rows plus manager presence; `store_managed` keys are rows Microsoft Store delivers.
pub struct InstalledList {
    pub packages: Vec<PackageSummary>,
    pub managers: Vec<ManagerStatus>,
    pub store_managed: BTreeSet<PackageKey>,
}

pub fn list_installed() -> Result<InstalledList, ManagerError> {
    let mut packages = Vec::new();
    let mut statuses = manager_statuses();
    let mut any_ok = false;
    let mut last_error: Option<ManagerError> = None;

    // A switched-off WinGet gets no COM child at all, so it is not asked.
    let mut jobs: Vec<(ManagerKind, Job<'static>)> = Vec::new();
    if manager_config::is_enabled(ManagerKind::WinGet) {
        jobs.push((
            ManagerKind::WinGet,
            job(|| broker::list_installed().map(Some)),
        ));
    }
    jobs.extend([
        (ManagerKind::Scoop, job(scoop::list_installed)),
        (ManagerKind::Chocolatey, job(chocolatey::list_installed)),
        (ManagerKind::Npm, job(npm::list_installed)),
        (ManagerKind::Pip, job(pip::list_installed)),
        (ManagerKind::Cargo, job(cargo_mgr::list_installed)),
        (ManagerKind::DotnetTool, job(dotnet_tool::list_installed)),
        (
            ManagerKind::PowerShell,
            job(|| powershell_mgr::list_installed(ManagerKind::PowerShell)),
        ),
        (
            ManagerKind::PowerShell7,
            job(|| powershell_mgr::list_installed(ManagerKind::PowerShell7)),
        ),
        (ManagerKind::Vcpkg, job(vcpkg::list_installed)),
    ]);

    for (kind, answer) in gather(jobs) {
        if kind == ManagerKind::WinGet {
            merge_winget(
                &mut packages,
                &mut statuses,
                &mut any_ok,
                &mut last_error,
                answer,
            );
        } else {
            merge_optional(
                &mut packages,
                &mut statuses,
                &mut any_ok,
                &mut last_error,
                kind,
                answer,
            );
        }
    }

    let all_disabled = statuses
        .iter()
        .all(|status| !manager_config::is_enabled(status.kind));
    if any_ok || all_disabled {
        // Local DisplayIcon files only; network logos load afterwards (see `Session`).
        icons::prefetch_local(&packages);
        let store_managed = store_managed::store_managed_keys(&packages);
        Ok(InstalledList {
            packages,
            managers: statuses,
            store_managed,
        })
    } else {
        Err(last_error.unwrap_or_else(|| ManagerError::Unavailable(NO_MANAGERS_AVAILABLE.into())))
    }
}

pub fn search_discover(
    query: &str,
    source: Option<&str>,
) -> Result<Vec<PackageSummary>, ManagerError> {
    let source = source.map(str::trim).filter(|value| !value.is_empty());
    let mut packages = Vec::new();
    let mut any_ok = false;
    let mut last_error: Option<ManagerError> = None;
    let all = source.is_none_or(|value| value.eq_ignore_ascii_case("all"));
    // Every manager whose source name is the chosen one (or all of them) is asked at once.
    let wanted = |kind: ManagerKind| {
        all || source.is_some_and(|value| value.eq_ignore_ascii_case(kind.as_str()))
    };

    let mut jobs: Vec<(ManagerKind, Job<'_>)> = Vec::new();
    let winget_source = source.filter(|value| {
        value.eq_ignore_ascii_case("winget") || value.eq_ignore_ascii_case("msstore")
    });
    if manager_config::is_enabled(ManagerKind::WinGet) && (all || winget_source.is_some()) {
        jobs.push((
            ManagerKind::WinGet,
            job(move || broker::search_discover(query, winget_source).map(Some)),
        ));
    }
    // Scoop also answers for bucket names, which are not fixed sources.
    let want_scoop = all
        || source.is_some_and(|value| value.eq_ignore_ascii_case("scoop"))
        || source.is_some_and(|value| !is_fixed_source(value));
    if want_scoop {
        jobs.push((
            ManagerKind::Scoop,
            job(move || {
                scoop::search(query).map(|rows| rows.map(|rows| scoop_bucket_rows(rows, source)))
            }),
        ));
    }
    let searches: [(ManagerKind, Search); 8] = [
        (ManagerKind::Chocolatey, chocolatey::search),
        (ManagerKind::Npm, npm::search),
        (ManagerKind::Pip, pip::search),
        (ManagerKind::Cargo, cargo_mgr::search),
        (ManagerKind::DotnetTool, dotnet_tool::search),
        (ManagerKind::PowerShell, search_powershell),
        (ManagerKind::PowerShell7, search_powershell7),
        (ManagerKind::Vcpkg, vcpkg::search),
    ];
    for (kind, search) in searches {
        if wanted(kind) {
            jobs.push((kind, job(move || search(query))));
        }
    }

    for (_, answer) in gather(jobs) {
        merge_search(&mut packages, &mut any_ok, &mut last_error, answer);
    }

    if any_ok {
        Ok(packages)
    } else {
        Err(last_error.unwrap_or_else(|| ManagerError::Operation(NO_SEARCH_ANSWER.into())))
    }
}

pub fn details(key: &PackageKey) -> Result<PackageDetails, ManagerError> {
    match key.manager {
        ManagerKind::WinGet if !manager_config::is_enabled(ManagerKind::WinGet) => {
            Err(winget_disabled())
        }
        ManagerKind::WinGet => broker::details(key),
        ManagerKind::Scoop => scoop::details(key),
        ManagerKind::Chocolatey => chocolatey::details(key),
        ManagerKind::Npm => npm::details(key),
        ManagerKind::Pip => pip::details(key),
        ManagerKind::Cargo => cargo_mgr::details(key),
        ManagerKind::DotnetTool => dotnet_tool::details(key),
        ManagerKind::PowerShell | ManagerKind::PowerShell7 => {
            powershell_mgr::details(key.manager, key)
        }
        ManagerKind::Vcpkg => vcpkg::details(key),
    }
}

pub fn run_operation(
    operation: &QueuedOperation,
    cancel: &CancelHandle,
    on_progress: impl FnMut(OperationProgress),
) -> Result<(), ManagerError> {
    match operation.key.manager {
        ManagerKind::WinGet if !manager_config::is_enabled(ManagerKind::WinGet) => {
            Err(winget_disabled())
        }
        ManagerKind::WinGet => broker::run_operation(operation, cancel, on_progress),
        ManagerKind::Scoop => scoop::run_operation(operation, cancel, on_progress),
        ManagerKind::Chocolatey => chocolatey::run_operation(operation, cancel, on_progress),
        ManagerKind::Npm => npm::run_operation(operation, cancel, on_progress),
        ManagerKind::Pip => pip::run_operation(operation, cancel, on_progress),
        ManagerKind::Cargo => cargo_mgr::run_operation(operation, cancel, on_progress),
        ManagerKind::DotnetTool => dotnet_tool::run_operation(operation, cancel, on_progress),
        ManagerKind::PowerShell | ManagerKind::PowerShell7 => {
            powershell_mgr::run_operation(operation.key.manager, operation, cancel, on_progress)
        }
        ManagerKind::Vcpkg => vcpkg::run_operation(operation, cancel, on_progress),
    }
}

fn winget_disabled() -> ManagerError {
    ManagerError::Unavailable("WinGet is turned off in PowerStore settings".into())
}

/// Whether the manager is installed, independent of its on/off switch. `available` only
/// locates the program (see `manager_config::is_installed`); it never runs a switched-off
/// manager.
fn status(kind: ManagerKind, available: impl FnOnce() -> bool) -> ManagerStatus {
    let presence = if available() {
        ManagerPresence::Available
    } else {
        ManagerPresence::Missing
    };
    ManagerStatus { kind, presence }
}

fn is_fixed_source(value: &str) -> bool {
    FIXED_SOURCES
        .iter()
        .any(|source| value.eq_ignore_ascii_case(source))
}

fn merge_optional(
    packages: &mut Vec<PackageSummary>,
    statuses: &mut [ManagerStatus],
    any_ok: &mut bool,
    last_error: &mut Option<ManagerError>,
    kind: ManagerKind,
    result: Result<Option<Vec<PackageSummary>>, ManagerError>,
) {
    match result {
        Ok(Some(rows)) => {
            packages.extend(rows);
            *any_ok = true;
            set_presence(statuses, kind, ManagerPresence::Available);
        }
        // A switched-off adapter also answers `None`; keep the located-program status.
        Ok(None) if manager_config::is_enabled(kind) => {
            set_presence(statuses, kind, ManagerPresence::Missing);
        }
        Ok(None) => {}
        Err(error) => {
            set_presence(statuses, kind, ManagerPresence::Available);
            *last_error = Some(error);
        }
    }
}

/// One manager's answer. `Ok(None)` means switched off or program absent; an error means
/// it is present but failed.
type Answer = Result<Option<Vec<PackageSummary>>, ManagerError>;
type Job<'a> = Box<dyn FnOnce() -> Answer + Send + 'a>;
/// A manager's search adapter for one query.
type Search = fn(&str) -> Answer;

fn job<'a>(run: impl FnOnce() -> Answer + Send + 'a) -> Job<'a> {
    Box::new(run)
}

/// Runs every job on its own scoped thread and returns the answers in input order, so the
/// slowest manager sets the latency while the merge order stays fixed. A job that panics
/// reports an error for its own manager only.
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
                    Err(ManagerError::Operation(format!(
                        "{} worker stopped",
                        kind.as_str()
                    )))
                });
                (kind, answer)
            })
            .collect()
    })
}

fn search_powershell(query: &str) -> Answer {
    powershell_mgr::search(ManagerKind::PowerShell, query)
}

fn search_powershell7(query: &str) -> Answer {
    powershell_mgr::search(ManagerKind::PowerShell7, query)
}

/// Scoop answers every bucket at once; a bucket name as the source keeps only that bucket.
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

/// WinGet keeps its own presence rules: a missing or too-old App Installer is reported as
/// such instead of as a generic failure.
fn merge_winget(
    packages: &mut Vec<PackageSummary>,
    statuses: &mut [ManagerStatus],
    any_ok: &mut bool,
    last_error: &mut Option<ManagerError>,
    answer: Answer,
) {
    match answer {
        Ok(Some(rows)) => {
            packages.extend(rows);
            *any_ok = true;
            set_presence(statuses, ManagerKind::WinGet, ManagerPresence::Available);
        }
        Ok(None) => {}
        Err(ManagerError::Unavailable(message)) => {
            let presence = if message.contains("app-installer-too-old")
                || message.contains("too old")
                || message.contains("CLASS_NOT_REGISTERED")
            {
                ManagerPresence::TooOld
            } else {
                ManagerPresence::Missing
            };
            set_presence(statuses, ManagerKind::WinGet, presence);
            *last_error = Some(ManagerError::Unavailable(message));
        }
        Err(error) => *last_error = Some(error),
    }
}

fn merge_search(
    packages: &mut Vec<PackageSummary>,
    any_ok: &mut bool,
    last_error: &mut Option<ManagerError>,
    result: Answer,
) {
    match result {
        Ok(Some(rows)) => {
            packages.extend(rows);
            *any_ok = true;
        }
        Ok(None) => {}
        Err(error) => *last_error = Some(error),
    }
}

fn set_presence(statuses: &mut [ManagerStatus], kind: ManagerKind, presence: ManagerPresence) {
    if let Some(status) = statuses.iter_mut().find(|status| status.kind == kind) {
        status.presence = presence;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_keep_input_order_and_a_panic_stays_with_its_manager() {
        let jobs: Vec<(ManagerKind, Job<'static>)> = vec![
            (ManagerKind::Npm, job(|| Ok(Some(Vec::new())))),
            (ManagerKind::Pip, job(|| panic!("synthetic adapter panic"))),
            (ManagerKind::Cargo, job(|| Ok(None))),
        ];
        let answers = gather(jobs);
        let kinds: Vec<ManagerKind> = answers.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(
            kinds,
            [ManagerKind::Npm, ManagerKind::Pip, ManagerKind::Cargo]
        );
        assert!(matches!(&answers[0].1, Ok(Some(rows)) if rows.is_empty()));
        assert!(answers[1].1.is_err());
        assert!(matches!(answers[2].1, Ok(None)));
    }
}
