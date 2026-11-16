//! Fan-out across WinGet (broker child) and CLI managers (Scoop through vcpkg).
use super::broker::{self, CancelHandle};
use super::cargo_mgr;
use super::chocolatey;
use super::dotnet_tool;
use super::npm;
use super::pip;
use super::powershell_mgr;
use super::scoop;
use super::vcpkg;
use super::winget::WinGetError;
use crate::power_store::{
    ManagerKind, OperationProgress, PackageDetails, PackageKey, PackageSummary, QueuedOperation,
};

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
        ManagerStatus {
            kind: ManagerKind::WinGet,
            presence: ManagerPresence::Available,
        },
        status(ManagerKind::Scoop, scoop::is_available()),
        status(ManagerKind::Chocolatey, chocolatey::is_available()),
        status(ManagerKind::Npm, npm::is_available()),
        status(ManagerKind::Pip, pip::is_available()),
        status(ManagerKind::Cargo, cargo_mgr::is_available()),
        status(ManagerKind::DotnetTool, dotnet_tool::is_available()),
        status(
            ManagerKind::PowerShell,
            powershell_mgr::is_available(ManagerKind::PowerShell),
        ),
        status(
            ManagerKind::PowerShell7,
            powershell_mgr::is_available(ManagerKind::PowerShell7),
        ),
        status(ManagerKind::Vcpkg, vcpkg::is_available()),
    ]
}

pub fn list_installed() -> Result<(Vec<PackageSummary>, Vec<ManagerStatus>), WinGetError> {
    let mut packages = Vec::new();
    let mut statuses = manager_statuses();
    let mut any_ok = false;
    let mut last_error: Option<WinGetError> = None;

    match broker::list_installed() {
        Ok(rows) => {
            packages.extend(rows);
            any_ok = true;
            set_presence(
                &mut statuses,
                ManagerKind::WinGet,
                ManagerPresence::Available,
            );
        }
        Err(WinGetError::Unavailable(message)) => {
            let presence = if message.contains("app-installer-too-old")
                || message.contains("too old")
                || message.contains("CLASS_NOT_REGISTERED")
            {
                ManagerPresence::TooOld
            } else {
                ManagerPresence::Missing
            };
            set_presence(&mut statuses, ManagerKind::WinGet, presence);
            last_error = Some(WinGetError::Unavailable(message));
        }
        Err(error) => last_error = Some(error),
    }

    merge_optional(
        &mut packages,
        &mut statuses,
        &mut any_ok,
        &mut last_error,
        ManagerKind::Scoop,
        scoop::list_installed(),
    );
    merge_optional(
        &mut packages,
        &mut statuses,
        &mut any_ok,
        &mut last_error,
        ManagerKind::Chocolatey,
        chocolatey::list_installed(),
    );
    merge_optional(
        &mut packages,
        &mut statuses,
        &mut any_ok,
        &mut last_error,
        ManagerKind::Npm,
        npm::list_installed(),
    );
    merge_optional(
        &mut packages,
        &mut statuses,
        &mut any_ok,
        &mut last_error,
        ManagerKind::Pip,
        pip::list_installed(),
    );
    merge_optional(
        &mut packages,
        &mut statuses,
        &mut any_ok,
        &mut last_error,
        ManagerKind::Cargo,
        cargo_mgr::list_installed(),
    );
    merge_optional(
        &mut packages,
        &mut statuses,
        &mut any_ok,
        &mut last_error,
        ManagerKind::DotnetTool,
        dotnet_tool::list_installed(),
    );
    merge_optional(
        &mut packages,
        &mut statuses,
        &mut any_ok,
        &mut last_error,
        ManagerKind::PowerShell,
        powershell_mgr::list_installed(ManagerKind::PowerShell),
    );
    merge_optional(
        &mut packages,
        &mut statuses,
        &mut any_ok,
        &mut last_error,
        ManagerKind::PowerShell7,
        powershell_mgr::list_installed(ManagerKind::PowerShell7),
    );
    merge_optional(
        &mut packages,
        &mut statuses,
        &mut any_ok,
        &mut last_error,
        ManagerKind::Vcpkg,
        vcpkg::list_installed(),
    );

    if any_ok {
        Ok((packages, statuses))
    } else {
        Err(last_error.unwrap_or_else(|| {
            WinGetError::Unavailable("No package managers are available".into())
        }))
    }
}

pub fn search_discover(
    query: &str,
    source: Option<&str>,
) -> Result<Vec<PackageSummary>, WinGetError> {
    let source = source.map(str::trim).filter(|value| !value.is_empty());
    let mut packages = Vec::new();
    let mut any_ok = false;
    let mut last_error: Option<WinGetError> = None;
    let all = source.is_none_or(|value| value.eq_ignore_ascii_case("all"));

    if all
        || source.is_some_and(|value| {
            value.eq_ignore_ascii_case("winget") || value.eq_ignore_ascii_case("msstore")
        })
    {
        let winget_source = source.filter(|value| {
            value.eq_ignore_ascii_case("winget") || value.eq_ignore_ascii_case("msstore")
        });
        match broker::search_discover(query, winget_source) {
            Ok(rows) => {
                packages.extend(rows);
                any_ok = true;
            }
            Err(error) => last_error = Some(error),
        }
    }

    let want_scoop = all
        || source.is_some_and(|value| value.eq_ignore_ascii_case("scoop"))
        || source.is_some_and(|value| !is_fixed_source(value));
    if want_scoop {
        match scoop::search(query) {
            Ok(Some(rows)) => {
                let filtered = match source {
                    Some(value)
                        if !value.eq_ignore_ascii_case("all")
                            && !value.eq_ignore_ascii_case("scoop") =>
                    {
                        rows.into_iter()
                            .filter(|package| package.key.source.eq_ignore_ascii_case(value))
                            .collect()
                    }
                    _ => rows,
                };
                packages.extend(filtered);
                any_ok = true;
            }
            Ok(None) => {}
            Err(error) => last_error = Some(error),
        }
    }

    merge_search(
        &mut packages,
        &mut any_ok,
        &mut last_error,
        all || source.is_some_and(|value| value.eq_ignore_ascii_case("chocolatey")),
        chocolatey::search(query),
    );
    merge_search(
        &mut packages,
        &mut any_ok,
        &mut last_error,
        all || source.is_some_and(|value| value.eq_ignore_ascii_case("npm")),
        npm::search(query),
    );
    merge_search(
        &mut packages,
        &mut any_ok,
        &mut last_error,
        all || source.is_some_and(|value| value.eq_ignore_ascii_case("pip")),
        pip::search(query),
    );
    merge_search(
        &mut packages,
        &mut any_ok,
        &mut last_error,
        all || source.is_some_and(|value| value.eq_ignore_ascii_case("cargo")),
        cargo_mgr::search(query),
    );
    merge_search(
        &mut packages,
        &mut any_ok,
        &mut last_error,
        all || source.is_some_and(|value| value.eq_ignore_ascii_case("dotnet")),
        dotnet_tool::search(query),
    );
    merge_search(
        &mut packages,
        &mut any_ok,
        &mut last_error,
        all || source.is_some_and(|value| value.eq_ignore_ascii_case("powershell")),
        powershell_mgr::search(ManagerKind::PowerShell, query),
    );
    merge_search(
        &mut packages,
        &mut any_ok,
        &mut last_error,
        all || source.is_some_and(|value| value.eq_ignore_ascii_case("powershell7")),
        powershell_mgr::search(ManagerKind::PowerShell7, query),
    );
    merge_search(
        &mut packages,
        &mut any_ok,
        &mut last_error,
        all || source.is_some_and(|value| value.eq_ignore_ascii_case("vcpkg")),
        vcpkg::search(query),
    );

    if any_ok {
        Ok(packages)
    } else {
        Err(last_error.unwrap_or_else(|| {
            WinGetError::Operation("No package managers answered the search".into())
        }))
    }
}

pub fn details(key: &PackageKey) -> Result<PackageDetails, WinGetError> {
    match key.manager {
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
) -> Result<(), WinGetError> {
    match operation.key.manager {
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

fn status(kind: ManagerKind, available: bool) -> ManagerStatus {
    ManagerStatus {
        kind,
        presence: if available {
            ManagerPresence::Available
        } else {
            ManagerPresence::Missing
        },
    }
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
    last_error: &mut Option<WinGetError>,
    kind: ManagerKind,
    result: Result<Option<Vec<PackageSummary>>, WinGetError>,
) {
    match result {
        Ok(Some(rows)) => {
            packages.extend(rows);
            *any_ok = true;
            set_presence(statuses, kind, ManagerPresence::Available);
        }
        Ok(None) => set_presence(statuses, kind, ManagerPresence::Missing),
        Err(error) => {
            set_presence(statuses, kind, ManagerPresence::Available);
            *last_error = Some(error);
        }
    }
}

fn merge_search(
    packages: &mut Vec<PackageSummary>,
    any_ok: &mut bool,
    last_error: &mut Option<WinGetError>,
    enabled: bool,
    result: Result<Option<Vec<PackageSummary>>, WinGetError>,
) {
    if !enabled {
        return;
    }
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
