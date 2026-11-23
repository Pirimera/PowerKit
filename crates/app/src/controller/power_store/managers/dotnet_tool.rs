//! Global .NET tools (`dotnet tool list/search/install -g`).
use super::{Cancel, Cx};
use crate::controller::power_store::CatalogError;
use powerkit_core::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, parse_dotnet_tool_list, parse_dotnet_tool_search,
};
use std::path::PathBuf;

const SOURCE: &str = "dotnet";

/// Installed, whether or not the manager is switched on.
pub fn is_available(cx: &Cx) -> bool {
    locate(cx).is_some()
}

/// The dotnet program, whether or not the manager is switched on.
pub fn locate(cx: &Cx) -> Option<PathBuf> {
    cx.locate_program(ManagerKind::DotnetTool, || cx.find("dotnet"))
}

pub fn list_installed(cx: &Cx) -> Result<Option<Vec<PackageSummary>>, CatalogError> {
    let Some(program) = dotnet_program(cx) else {
        return Ok(None);
    };
    let text = cx.run_output(&program, &["tool", "list", "-g"])?;
    let packages = parse_dotnet_tool_list(&text)
        .into_iter()
        .map(|(id, version)| PackageSummary {
            key: PackageKey::new(ManagerKind::DotnetTool, SOURCE, id.clone()),
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

pub fn search(cx: &Cx, query: &str) -> Result<Option<Vec<PackageSummary>>, CatalogError> {
    let Some(program) = dotnet_program(cx) else {
        return Ok(None);
    };
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let text = cx.run_output(&program, &["tool", "search", trimmed])?;
    let packages = parse_dotnet_tool_search(&text)
        .into_iter()
        .take(200)
        .map(|(id, version)| PackageSummary {
            key: PackageKey::new(ManagerKind::DotnetTool, SOURCE, id.clone()),
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

pub fn details(key: &PackageKey) -> Result<PackageDetails, CatalogError> {
    Ok(PackageDetails {
        id: key.id.clone(),
        name: key.id.clone(),
        ..PackageDetails::default()
    })
}

pub fn run_operation(
    cx: &Cx,
    operation: &QueuedOperation,
    cancel: &Cancel,
    mut on_progress: impl FnMut(OperationProgress),
) -> Result<(), CatalogError> {
    let program = dotnet_program(cx)
        .ok_or_else(|| CatalogError::Unavailable("dotnet is not installed".into()))?;
    let args: Vec<&str> = match operation.kind {
        OperationKind::Install => vec!["tool", "install", "-g", operation.key.id.as_str()],
        OperationKind::Update => vec!["tool", "update", "-g", operation.key.id.as_str()],
        OperationKind::Uninstall => vec!["tool", "uninstall", "-g", operation.key.id.as_str()],
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

fn dotnet_program(cx: &Cx) -> Option<PathBuf> {
    cx.resolve_program(ManagerKind::DotnetTool, || cx.find("dotnet"))
}
