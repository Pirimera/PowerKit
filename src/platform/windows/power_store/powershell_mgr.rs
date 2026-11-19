//! PowerShellGet modules via Windows PowerShell 5.1 (`powershell`) or PowerShell 7 (`pwsh`).
use super::broker::CancelHandle;
use super::cli_process::{find_on_path, run_collect, run_output};
use super::manager_config;
use super::winget::WinGetError;
use crate::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, parse_powershell_modules_json,
};
use std::path::PathBuf;

/// Installed, whether or not the manager is switched on.
pub fn is_available(kind: ManagerKind) -> bool {
    locate(kind).is_some()
}

/// The shell program for `kind`, whether or not the manager is switched on.
pub fn locate(kind: ManagerKind) -> Option<PathBuf> {
    manager_config::locate_program(kind, || discover_shell(kind))
}

pub fn list_installed(kind: ManagerKind) -> Result<Option<Vec<PackageSummary>>, WinGetError> {
    let Some(program) = shell_program(kind) else {
        return Ok(None);
    };
    let source = source_name(kind);
    let installed = run_output(
        &program,
        &[
            "-NoProfile",
            "-Command",
            "Get-InstalledModule | Select-Object Name,Version | ConvertTo-Json -Compress",
        ],
    )?;
    // Outdated detection would need one gallery query per module; keep list local-only
    // and let Update-Module resolve newer versions when the user queues an update.
    let packages = parse_powershell_modules_json(&installed)
        .into_iter()
        .map(|(id, version)| PackageSummary {
            key: PackageKey::new(kind, source, id.clone()),
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

pub fn search(kind: ManagerKind, query: &str) -> Result<Option<Vec<PackageSummary>>, WinGetError> {
    let Some(program) = shell_program(kind) else {
        return Ok(None);
    };
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let source = source_name(kind);
    let pattern = format!("*{trimmed}*").replace('\'', "''");
    let command = format!(
        "Find-Module -Name '{pattern}' -ErrorAction SilentlyContinue | Select-Object -First 50 Name,Version | ConvertTo-Json -Compress"
    );
    let text = run_output(&program, &["-NoProfile", "-Command", &command])?;
    let packages = parse_powershell_modules_json(&text)
        .into_iter()
        .map(|(id, version)| PackageSummary {
            key: PackageKey::new(kind, source, id.clone()),
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

pub fn details(kind: ManagerKind, key: &PackageKey) -> Result<PackageDetails, WinGetError> {
    let program = shell_program(kind).ok_or_else(|| {
        WinGetError::Unavailable(format!("{} is not installed", source_name(kind)))
    })?;
    let command = format!(
        "Find-Module -Name '{}' -ErrorAction SilentlyContinue | Select-Object -First 1 Name,Version,Description,ProjectUri,Author | ConvertTo-Json -Compress",
        key.id.replace('\'', "''")
    );
    let text = run_output(&program, &["-NoProfile", "-Command", &command]).unwrap_or_default();
    let version = parse_powershell_modules_json(&text)
        .into_iter()
        .next()
        .map(|(_, version)| version)
        .unwrap_or_default();
    Ok(PackageDetails {
        id: key.id.clone(),
        name: key.id.clone(),
        version,
        description: json_string(&text, "Description").unwrap_or_default(),
        homepage: json_string(&text, "ProjectUri").unwrap_or_default(),
        author: json_string(&text, "Author").unwrap_or_default(),
        ..PackageDetails::default()
    })
}

pub fn run_operation(
    kind: ManagerKind,
    operation: &QueuedOperation,
    cancel: &CancelHandle,
    mut on_progress: impl FnMut(OperationProgress),
) -> Result<(), WinGetError> {
    let program = shell_program(kind).ok_or_else(|| {
        WinGetError::Unavailable(format!("{} is not installed", source_name(kind)))
    })?;
    let id = operation.key.id.replace('\'', "''");
    let command = match operation.kind {
        OperationKind::Install => {
            format!("Install-Module -Name '{id}' -Force -Scope CurrentUser -AllowClobber")
        }
        OperationKind::Update => format!("Update-Module -Name '{id}' -Force"),
        OperationKind::Uninstall => format!("Uninstall-Module -Name '{id}' -Force -AllVersions"),
    };
    on_progress(OperationProgress {
        stage: ProgressStage::Queued,
        percent: None,
    });
    run_collect(
        &program,
        &["-NoProfile", "-Command", &command],
        Some(cancel),
        Some(&mut on_progress),
    )?;
    on_progress(OperationProgress {
        stage: ProgressStage::Finishing,
        percent: Some(100),
    });
    Ok(())
}

fn shell_program(kind: ManagerKind) -> Option<PathBuf> {
    manager_config::resolve_program(kind, || discover_shell(kind))
}

fn discover_shell(kind: ManagerKind) -> Option<PathBuf> {
    match kind {
        ManagerKind::PowerShell => find_on_path("powershell"),
        ManagerKind::PowerShell7 => find_on_path("pwsh"),
        _ => None,
    }
}

fn source_name(kind: ManagerKind) -> &'static str {
    match kind {
        ManagerKind::PowerShell => "powershell",
        ManagerKind::PowerShell7 => "powershell7",
        other => other.as_str(),
    }
}

fn json_string(text: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{field}\"");
    let index = text.find(&pattern)?;
    let rest = text[index + pattern.len()..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut chars = rest[1..].chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                other => out.push(other),
            },
            other => out.push(other),
        }
    }
    None
}
