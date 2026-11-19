//! vcpkg classic-mode packages (`vcpkg list/search/install/remove`).
use super::broker::CancelHandle;
use super::cli_process::{find_on_path, run_collect, run_output};
use super::manager_config;
use super::winget::ManagerError;
use crate::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, parse_vcpkg_list,
};
use std::path::PathBuf;

const SOURCE: &str = "vcpkg";

/// Installed, whether or not the manager is switched on.
pub fn is_available() -> bool {
    locate().is_some()
}

/// The vcpkg program, whether or not the manager is switched on.
pub fn locate() -> Option<PathBuf> {
    manager_config::locate_program(ManagerKind::Vcpkg, || find_on_path("vcpkg"))
}

pub fn list_installed() -> Result<Option<Vec<PackageSummary>>, ManagerError> {
    let Some(program) = vcpkg_program() else {
        return Ok(None);
    };
    let text = run_output(&program, &["list"])?;
    let packages = parse_vcpkg_list(&text)
        .into_iter()
        .map(|(id, version, _)| PackageSummary {
            key: PackageKey::new(ManagerKind::Vcpkg, SOURCE, id.clone()),
            name: id,
            version: Some(version).filter(|value| !value.is_empty()),
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

pub fn search(query: &str) -> Result<Option<Vec<PackageSummary>>, ManagerError> {
    let Some(program) = vcpkg_program() else {
        return Ok(None);
    };
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let text = run_output(&program, &["search", trimmed])?;
    let packages = parse_vcpkg_list(&text)
        .into_iter()
        .take(200)
        .map(|(id, version, _)| PackageSummary {
            key: PackageKey::new(ManagerKind::Vcpkg, SOURCE, id.clone()),
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

pub fn details(key: &PackageKey) -> Result<PackageDetails, ManagerError> {
    let program = vcpkg_program()
        .ok_or_else(|| ManagerError::Unavailable("vcpkg is not installed".into()))?;
    let name = key.id.split(':').next().unwrap_or(key.id.as_str());
    let text = run_output(&program, &["search", name]).unwrap_or_default();
    let hit = parse_vcpkg_list(&text)
        .into_iter()
        .find(|(id, _, _)| id.eq_ignore_ascii_case(&key.id) || id.starts_with(name));
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
) -> Result<(), ManagerError> {
    let program = vcpkg_program()
        .ok_or_else(|| ManagerError::Unavailable("vcpkg is not installed".into()))?;
    let target = with_triplet(
        &operation.key.id,
        manager_config::default_triplet(ManagerKind::Vcpkg).as_deref(),
    );
    let args: Vec<&str> = match operation.kind {
        OperationKind::Install | OperationKind::Update => vec!["install", target.as_str()],
        OperationKind::Uninstall => vec!["remove", operation.key.id.as_str()],
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

/// `name` as vcpkg expects it for an install: a package that already names its triplet
/// (`fmt:arm64-windows`, as in `vcpkg list`) keeps it, otherwise the default is appended;
/// without a default vcpkg picks its own triplet.
fn with_triplet(name: &str, triplet: Option<&str>) -> String {
    match triplet {
        Some(triplet) if !name.contains(':') => format!("{name}:{triplet}"),
        _ => name.to_string(),
    }
}

fn vcpkg_program() -> Option<PathBuf> {
    manager_config::resolve_program(ManagerKind::Vcpkg, || find_on_path("vcpkg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_triplet_is_appended_only_to_bare_names() {
        assert_eq!(
            with_triplet("fmt", Some("arm64-windows")),
            "fmt:arm64-windows"
        );
        assert_eq!(
            with_triplet("fmt:x64-windows", Some("arm64-windows")),
            "fmt:x64-windows"
        );
        assert_eq!(with_triplet("fmt", None), "fmt");
    }
}
