//! User/global pip packages via `python -m pip` / `py -m pip` / `pip`.
use super::broker::CancelHandle;
use super::cli_process::{find_on_path, run_collect, run_output};
use super::manager_config;
use super::winget::ManagerError;
use crate::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, parse_pip_list_json, parse_pip_outdated_json,
};
use std::path::PathBuf;

const SOURCE: &str = "pip";

/// Installed, whether or not the manager is switched on.
pub fn is_available() -> bool {
    locate_pip().is_some()
}

pub fn list_installed() -> Result<Option<Vec<PackageSummary>>, ManagerError> {
    let Some((program, prefix)) = pip_invocation() else {
        return Ok(None);
    };
    let listed = pip_output(&program, &prefix, &["list", "--format=json"])?;
    let outdated =
        pip_output(&program, &prefix, &["list", "--outdated", "--format=json"]).unwrap_or_default();
    let updates = parse_pip_outdated_json(&outdated);
    let mut packages = Vec::new();
    for (id, version) in parse_pip_list_json(&listed) {
        let latest = updates
            .iter()
            .find(|(name, _, _)| name.eq_ignore_ascii_case(&id))
            .map(|(_, _, latest)| latest.clone());
        packages.push(PackageSummary {
            key: PackageKey::new(ManagerKind::Pip, SOURCE, id.clone()),
            name: id,
            version: Some(version),
            available_version: latest.clone(),
            update_available: latest.is_some(),
            scope: Some("user".into()),
            install_date: None,
            updated_at: None,
            icon: None,
        });
    }
    Ok(Some(packages))
}

pub fn search(query: &str) -> Result<Option<Vec<PackageSummary>>, ManagerError> {
    let Some((program, prefix)) = pip_invocation() else {
        return Ok(None);
    };
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Some(Vec::new()));
    }
    // Broad search is unavailable; treat the query as an exact package id.
    let text = match pip_output(&program, &prefix, &["index", "versions", trimmed]) {
        Ok(text) => text,
        Err(_) => return Ok(Some(Vec::new())),
    };
    let latest = text
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("Available versions:")
                .or_else(|| line.strip_prefix("LATEST:"))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(|value| value.split(',').next().map(str::trim))
                .map(str::to_string)
        })
        .or_else(|| {
            text.lines()
                .find_map(|line| line.split_whitespace().nth(1).map(str::to_string))
        });
    Ok(Some(vec![PackageSummary {
        key: PackageKey::new(ManagerKind::Pip, SOURCE, trimmed),
        name: trimmed.to_string(),
        version: None,
        available_version: latest,
        update_available: false,
        scope: Some("user".into()),
        install_date: None,
        updated_at: None,
        icon: None,
    }]))
}

pub fn details(key: &PackageKey) -> Result<PackageDetails, ManagerError> {
    let (program, prefix) =
        pip_invocation().ok_or_else(|| ManagerError::Unavailable("pip is not installed".into()))?;
    let text = pip_output(&program, &prefix, &["show", &key.id]).unwrap_or_default();
    let mut details = PackageDetails {
        id: key.id.clone(),
        name: key.id.clone(),
        ..PackageDetails::default()
    };
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Name: ") {
            details.name = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("Version: ") {
            details.version = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("Summary: ") {
            details.description = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("Home-page: ") {
            details.homepage = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("Author: ") {
            details.author = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("License: ") {
            details.license = value.trim().to_string();
        }
    }
    Ok(details)
}

pub fn run_operation(
    operation: &QueuedOperation,
    cancel: &CancelHandle,
    mut on_progress: impl FnMut(OperationProgress),
) -> Result<(), ManagerError> {
    let (program, prefix) =
        pip_invocation().ok_or_else(|| ManagerError::Unavailable("pip is not installed".into()))?;
    let mut args = prefix.clone();
    match operation.kind {
        OperationKind::Install => {
            args.extend(["install".into(), operation.key.id.clone()]);
        }
        OperationKind::Update => {
            args.extend(["install".into(), "-U".into(), operation.key.id.clone()]);
        }
        OperationKind::Uninstall => {
            args.extend(["uninstall".into(), "-y".into(), operation.key.id.clone()]);
        }
    }
    if let Some(version) = operation
        .options
        .version
        .as_deref()
        .filter(|value| !value.is_empty())
        && matches!(
            operation.kind,
            OperationKind::Install | OperationKind::Update
        )
        && let Some(last) = args.last_mut()
    {
        *last = format!("{}=={version}", operation.key.id);
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    on_progress(OperationProgress {
        stage: ProgressStage::Queued,
        percent: None,
    });
    run_collect(&program, &refs, Some(cancel), Some(&mut on_progress))?;
    on_progress(OperationProgress {
        stage: ProgressStage::Finishing,
        percent: Some(100),
    });
    Ok(())
}

fn pip_invocation() -> Option<(PathBuf, Vec<String>)> {
    if !manager_config::is_enabled(ManagerKind::Pip) {
        return None;
    }
    locate_pip()
}

/// Program plus leading arguments, whatever the on/off switch says. A custom program is
/// either `pip` itself or the Python interpreter that should run `-m pip`; it never falls
/// back to PATH when it is missing.
pub(super) fn locate_pip() -> Option<(PathBuf, Vec<String>)> {
    if let Some(custom) = manager_config::custom_executable(ManagerKind::Pip) {
        return custom.is_file().then(|| {
            let is_pip = custom
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.to_ascii_lowercase().starts_with("pip"));
            let prefix = if is_pip {
                Vec::new()
            } else {
                vec!["-m".into(), "pip".into()]
            };
            (custom, prefix)
        });
    }
    if let Some(python) = find_on_path("python") {
        return Some((python, vec!["-m".into(), "pip".into()]));
    }
    if let Some(py) = find_on_path("py") {
        return Some((py, vec!["-m".into(), "pip".into()]));
    }
    find_on_path("pip").map(|pip| (pip, Vec::new()))
}

fn pip_output(
    program: &std::path::Path,
    prefix: &[String],
    args: &[&str],
) -> Result<String, ManagerError> {
    let mut full = prefix.iter().map(String::as_str).collect::<Vec<_>>();
    full.extend(args);
    run_output(program, &full)
}
