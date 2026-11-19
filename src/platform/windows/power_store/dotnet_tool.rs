//! Global .NET tools (`dotnet tool list/search/install -g`).
use super::broker::CancelHandle;
use super::cli_process::{find_on_path, run_collect, run_output};
use super::manager_config;
use super::winget::ManagerError;
use crate::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, parse_dotnet_tool_list, parse_dotnet_tool_search,
};
use std::path::PathBuf;

const SOURCE: &str = "dotnet";

/// Installed, whether or not the manager is switched on.
pub fn is_available() -> bool {
    locate().is_some()
}

/// The dotnet program, whether or not the manager is switched on.
pub fn locate() -> Option<PathBuf> {
    manager_config::locate_program(ManagerKind::DotnetTool, || find_on_path("dotnet"))
}

pub fn list_installed() -> Result<Option<Vec<PackageSummary>>, ManagerError> {
    let Some(program) = dotnet_program() else {
        return Ok(None);
    };
    let text = run_output(&program, &["tool", "list", "-g"])?;
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

pub fn search(query: &str) -> Result<Option<Vec<PackageSummary>>, ManagerError> {
    let Some(program) = dotnet_program() else {
        return Ok(None);
    };
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let text = run_output(&program, &["tool", "search", trimmed])?;
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

pub fn details(key: &PackageKey) -> Result<PackageDetails, ManagerError> {
    Ok(PackageDetails {
        id: key.id.clone(),
        name: key.id.clone(),
        ..PackageDetails::default()
    })
}

pub fn run_operation(
    operation: &QueuedOperation,
    cancel: &CancelHandle,
    mut on_progress: impl FnMut(OperationProgress),
) -> Result<(), ManagerError> {
    let program = dotnet_program()
        .ok_or_else(|| ManagerError::Unavailable("dotnet is not installed".into()))?;
    let args: Vec<&str> = match operation.kind {
        OperationKind::Install => vec!["tool", "install", "-g", operation.key.id.as_str()],
        OperationKind::Update => vec!["tool", "update", "-g", operation.key.id.as_str()],
        OperationKind::Uninstall => vec!["tool", "uninstall", "-g", operation.key.id.as_str()],
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

fn dotnet_program() -> Option<PathBuf> {
    manager_config::resolve_program(ManagerKind::DotnetTool, || find_on_path("dotnet"))
}
