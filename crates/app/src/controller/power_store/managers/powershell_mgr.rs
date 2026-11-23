//! PowerShellGet modules via Windows PowerShell 5.1 (`powershell`) or PowerShell 7 (`pwsh`).
use super::{Cancel, Cx};
use crate::controller::power_store::CatalogError;
use powerkit_core::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, parse_powershell_modules_json,
};
use std::path::PathBuf;

/// PowerShell Gallery names use only letters, digits, `.`, `-` and `_`. Names are placed
/// in `-Command` text, so anything else is refused instead of being quoted: PowerShell also
/// treats typographic quotes as string delimiters, and imported files can carry any id.
fn is_gallery_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Installed, whether or not the manager is switched on.
pub fn is_available(cx: &Cx, kind: ManagerKind) -> bool {
    locate(cx, kind).is_some()
}

/// The shell program for `kind`, whether or not the manager is switched on.
pub fn locate(cx: &Cx, kind: ManagerKind) -> Option<PathBuf> {
    cx.locate_program(kind, || discover_shell(cx, kind))
}

pub fn list_installed(
    cx: &Cx,
    kind: ManagerKind,
) -> Result<Option<Vec<PackageSummary>>, CatalogError> {
    let Some(program) = shell_program(cx, kind) else {
        return Ok(None);
    };
    let source = source_name(kind);
    let installed = cx.run_output(
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

pub fn search(
    cx: &Cx,
    kind: ManagerKind,
    query: &str,
) -> Result<Option<Vec<PackageSummary>>, CatalogError> {
    let Some(program) = shell_program(cx, kind) else {
        return Ok(None);
    };
    let trimmed = query.trim();
    // A query outside the gallery name alphabet cannot match any module.
    if !is_gallery_name(trimmed) {
        return Ok(Some(Vec::new()));
    }
    let source = source_name(kind);
    let command = format!(
        "Find-Module -Name '*{trimmed}*' -ErrorAction SilentlyContinue | Select-Object -First 50 Name,Version | ConvertTo-Json -Compress"
    );
    let text = cx.run_output(&program, &["-NoProfile", "-Command", &command])?;
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

pub fn details(
    cx: &Cx,
    kind: ManagerKind,
    key: &PackageKey,
) -> Result<PackageDetails, CatalogError> {
    let program = shell_program(cx, kind).ok_or_else(|| {
        CatalogError::Unavailable(format!("{} is not installed", source_name(kind)))
    })?;
    let text = if is_gallery_name(&key.id) {
        let command = format!(
            "Find-Module -Name '{}' -ErrorAction SilentlyContinue | Select-Object -First 1 Name,Version,Description,ProjectUri,Author | ConvertTo-Json -Compress",
            key.id
        );
        cx.run_output(&program, &["-NoProfile", "-Command", &command])
            .unwrap_or_default()
    } else {
        String::new()
    };
    let version = parse_powershell_modules_json(&text)
        .into_iter()
        .next()
        .map(|(_, version)| version)
        .unwrap_or_default();
    let module = powerkit_core::json::parse(&text).unwrap_or(powerkit_core::json::Value::Null);
    Ok(PackageDetails {
        id: key.id.clone(),
        name: key.id.clone(),
        version,
        description: module.str_field("Description").unwrap_or_default(),
        // Windows PowerShell serialises a Uri as an object, PowerShell 7 as a string.
        homepage: module
            .str_field("ProjectUri")
            .or_else(|| module.get("ProjectUri")?.str_field("AbsoluteUri"))
            .unwrap_or_default(),
        author: module.str_field("Author").unwrap_or_default(),
        ..PackageDetails::default()
    })
}

pub fn run_operation(
    cx: &Cx,
    kind: ManagerKind,
    operation: &QueuedOperation,
    cancel: &Cancel,
    mut on_progress: impl FnMut(OperationProgress),
) -> Result<(), CatalogError> {
    let program = shell_program(cx, kind).ok_or_else(|| {
        CatalogError::Unavailable(format!("{} is not installed", source_name(kind)))
    })?;
    let id = operation.key.id.as_str();
    if !is_gallery_name(id) {
        return Err(CatalogError::Failed(format!(
            "invalid PowerShell module name: {id}"
        )));
    }
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
    cx.run_collect(
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

fn shell_program(cx: &Cx, kind: ManagerKind) -> Option<PathBuf> {
    cx.resolve_program(kind, || discover_shell(cx, kind))
}

fn discover_shell(cx: &Cx, kind: ManagerKind) -> Option<PathBuf> {
    match kind {
        ManagerKind::PowerShell => cx.find("powershell"),
        ManagerKind::PowerShell7 => cx.find("pwsh"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_gallery_name_characters_reach_the_command_text() {
        assert!(is_gallery_name("PSReadLine"));
        assert!(is_gallery_name("Az.Accounts_2"));
        assert!(!is_gallery_name(""));
        assert!(!is_gallery_name("x'; calc #"));
        // PowerShell also accepts typographic quotes as string delimiters.
        assert!(!is_gallery_name("x\u{2019}; calc #"));
        assert!(!is_gallery_name("x\u{2018}; calc #"));
        assert!(!is_gallery_name("a b"));
    }
}
