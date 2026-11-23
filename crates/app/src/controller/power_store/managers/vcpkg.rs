//! vcpkg classic-mode packages (`vcpkg list/search/install/remove`).
use super::{Cancel, Cx};
use crate::controller::power_store::CatalogError;
use powerkit_core::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, parse_vcpkg_list,
};
use std::path::PathBuf;

const SOURCE: &str = "vcpkg";

/// Installed, whether or not the manager is switched on.
pub fn is_available(cx: &Cx) -> bool {
    locate(cx).is_some()
}

/// The vcpkg program, whether or not the manager is switched on.
pub fn locate(cx: &Cx) -> Option<PathBuf> {
    cx.locate_program(ManagerKind::Vcpkg, || cx.find("vcpkg"))
}

pub fn list_installed(cx: &Cx) -> Result<Option<Vec<PackageSummary>>, CatalogError> {
    let Some(program) = vcpkg_program(cx) else {
        return Ok(None);
    };
    let text = cx.run_output(&program, &["list"])?;
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

pub fn search(cx: &Cx, query: &str) -> Result<Option<Vec<PackageSummary>>, CatalogError> {
    let Some(program) = vcpkg_program(cx) else {
        return Ok(None);
    };
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let text = cx.run_output(&program, &["search", trimmed])?;
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

pub fn details(cx: &Cx, key: &PackageKey) -> Result<PackageDetails, CatalogError> {
    let program = vcpkg_program(cx)
        .ok_or_else(|| CatalogError::Unavailable("vcpkg is not installed".into()))?;
    let name = key.id.split(':').next().unwrap_or(key.id.as_str());
    let text = cx
        .run_output(&program, &["search", name])
        .unwrap_or_default();
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
    cx: &Cx,
    operation: &QueuedOperation,
    cancel: &Cancel,
    mut on_progress: impl FnMut(OperationProgress),
) -> Result<(), CatalogError> {
    let program = vcpkg_program(cx)
        .ok_or_else(|| CatalogError::Unavailable("vcpkg is not installed".into()))?;
    let target = with_triplet(
        &operation.key.id,
        cx.default_triplet(ManagerKind::Vcpkg).as_deref(),
    );
    let args: Vec<&str> = match operation.kind {
        OperationKind::Install | OperationKind::Update => vec!["install", target.as_str()],
        OperationKind::Uninstall => vec!["remove", operation.key.id.as_str()],
    };
    on_progress(OperationProgress {
        stage: ProgressStage::Queued,
        percent: None,
    });
    cx.run_collect(&program, &args, Some(cancel), Some(&mut on_progress))?;
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

fn vcpkg_program(cx: &Cx) -> Option<PathBuf> {
    cx.resolve_program(ManagerKind::Vcpkg, || cx.find("vcpkg"))
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
