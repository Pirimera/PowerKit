//! Global Cargo binaries (`cargo install --list` / search / uninstall).
use super::broker::CancelHandle;
use super::cli_process::{find_on_path, run_collect, run_output};
use super::winget::WinGetError;
use crate::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, parse_cargo_install_list, parse_cargo_search,
};
use std::path::PathBuf;

const SOURCE: &str = "cargo";

pub fn is_available() -> bool {
    cargo_program().is_some()
}

pub fn list_installed() -> Result<Option<Vec<PackageSummary>>, WinGetError> {
    let Some(program) = cargo_program() else {
        return Ok(None);
    };
    let text = run_output(&program, &["install", "--list"])?;
    let packages = parse_cargo_install_list(&text)
        .into_iter()
        .map(|(id, version)| PackageSummary {
            key: PackageKey::new(ManagerKind::Cargo, SOURCE, id.clone()),
            name: id,
            version: Some(version),
            available_version: None,
            update_available: false,
            scope: Some("user".into()),
            install_date: None,
            updated_at: None,
            icon: None,
        })
        .collect();
    Ok(Some(packages))
}

pub fn search(query: &str) -> Result<Option<Vec<PackageSummary>>, WinGetError> {
    let Some(program) = cargo_program() else {
        return Ok(None);
    };
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let text = run_output(&program, &["search", trimmed, "--limit", "20"])?;
    let packages = parse_cargo_search(&text)
        .into_iter()
        .take(200)
        .map(|(id, version, _)| PackageSummary {
            key: PackageKey::new(ManagerKind::Cargo, SOURCE, id.clone()),
            name: id,
            version: None,
            available_version: Some(version).filter(|value| !value.is_empty()),
            update_available: false,
            scope: Some("user".into()),
            install_date: None,
            updated_at: None,
            icon: None,
        })
        .collect();
    Ok(Some(packages))
}

pub fn details(key: &PackageKey) -> Result<PackageDetails, WinGetError> {
    let program =
        cargo_program().ok_or_else(|| WinGetError::Unavailable("cargo is not installed".into()))?;
    let text = run_output(&program, &["search", &key.id, "--limit", "1"]).unwrap_or_default();
    let hit = parse_cargo_search(&text)
        .into_iter()
        .find(|(id, _, _)| id.eq_ignore_ascii_case(&key.id));
    Ok(PackageDetails {
        id: key.id.clone(),
        name: key.id.clone(),
        version: hit
            .as_ref()
            .map(|(_, version, _)| version.clone())
            .unwrap_or_default(),
        description: hit
            .map(|(_, _, description)| description)
            .unwrap_or_default(),
        ..PackageDetails::default()
    })
}

pub fn run_operation(
    operation: &QueuedOperation,
    cancel: &CancelHandle,
    mut on_progress: impl FnMut(OperationProgress),
) -> Result<(), WinGetError> {
    let program =
        cargo_program().ok_or_else(|| WinGetError::Unavailable("cargo is not installed".into()))?;
    let args: Vec<&str> = match operation.kind {
        OperationKind::Install => vec!["install", operation.key.id.as_str()],
        OperationKind::Update => vec!["install", "--force", operation.key.id.as_str()],
        OperationKind::Uninstall => vec!["uninstall", operation.key.id.as_str()],
    };
    on_progress(OperationProgress {
        stage: ProgressStage::Queued,
        percent: None,
    });
    run_collect(&program, &args, Some(cancel), Some(&mut on_progress))?;
    on_progress(OperationProgress {
        stage: ProgressStage::Finishing,
        percent: Some(100),
    });
    Ok(())
}

fn cargo_program() -> Option<PathBuf> {
    find_on_path("cargo")
}
