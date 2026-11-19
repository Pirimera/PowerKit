//! Chocolatey CLI adapter using `--limit-output` / `-r` machine-readable rows.
use super::broker::CancelHandle;
use super::cli_process::{find_on_path, run_collect, run_output};
use super::manager_config;
use super::winget::ManagerError;
use crate::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, parse_choco_limit_rows, parse_choco_outdated_rows,
};
use std::path::{Path, PathBuf};

const SOURCE: &str = "chocolatey";
const MIN_MAJOR: u64 = 2;

/// Installed, whether or not the manager is switched on. A switched-off manager is
/// only located; its version is checked once it is on.
pub fn is_available() -> bool {
    if locate().is_none() {
        return false;
    }
    !manager_config::is_enabled(ManagerKind::Chocolatey)
        || choco_program().is_some_and(|program| version_ok(&program).unwrap_or(false))
}

/// The choco program, whether or not the manager is switched on.
pub fn locate() -> Option<PathBuf> {
    manager_config::locate_program(ManagerKind::Chocolatey, || find_on_path("choco"))
}

pub fn list_installed() -> Result<Option<Vec<PackageSummary>>, ManagerError> {
    let Some(program) = choco_program() else {
        return Ok(None);
    };
    if !version_ok(&program)? {
        return Ok(None);
    }
    let listed = run_output(
        &program,
        &["list", "--limit-output", "--no-progress", "--no-color"],
    )?;
    let outdated = run_output(
        &program,
        &["outdated", "--limit-output", "--no-progress", "--no-color"],
    )
    .unwrap_or_default();
    let updates = parse_choco_outdated_rows(&outdated);
    let mut packages = Vec::new();
    for (id, version) in parse_choco_limit_rows(&listed) {
        let update = updates.iter().find(|(update_id, _, _)| update_id == &id);
        let (available, update_available) = match update {
            Some((_, _, latest)) => (Some(latest.clone()), true),
            None => (None, false),
        };
        packages.push(PackageSummary {
            key: PackageKey::new(ManagerKind::Chocolatey, SOURCE, id.clone()),
            name: id,
            version: Some(version),
            available_version: available,
            update_available,
            scope: Some("machine".into()),
            install_date: None,
            updated_at: None,
            icon: None,
        });
    }
    Ok(Some(packages))
}

pub fn search(query: &str) -> Result<Option<Vec<PackageSummary>>, ManagerError> {
    let Some(program) = choco_program() else {
        return Ok(None);
    };
    if !version_ok(&program)? {
        return Ok(None);
    }
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let text = run_output(
        &program,
        &[
            "search",
            trimmed,
            "--limit-output",
            "--no-progress",
            "--no-color",
        ],
    )?;
    let packages = parse_choco_limit_rows(&text)
        .into_iter()
        .take(200)
        .map(|(id, version)| PackageSummary {
            key: PackageKey::new(ManagerKind::Chocolatey, SOURCE, id.clone()),
            name: id,
            version: None,
            available_version: Some(version),
            update_available: false,
            scope: Some("machine".into()),
            install_date: None,
            updated_at: None,
            icon: None,
        })
        .collect();
    Ok(Some(packages))
}

pub fn details(key: &PackageKey) -> Result<PackageDetails, ManagerError> {
    let program = choco_program()
        .ok_or_else(|| ManagerError::Unavailable("Chocolatey is not installed".into()))?;
    let text = run_output(
        &program,
        &[
            "search",
            &key.id,
            "--exact",
            "--limit-output",
            "--no-progress",
            "--no-color",
        ],
    )
    .unwrap_or_default();
    let version = parse_choco_limit_rows(&text)
        .into_iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(&key.id))
        .map(|(_, version)| version)
        .unwrap_or_default();
    Ok(PackageDetails {
        id: key.id.clone(),
        name: key.id.clone(),
        version,
        publisher: String::new(),
        author: String::new(),
        description: String::new(),
        homepage: String::new(),
        publisher_url: String::new(),
        license: String::new(),
        license_url: String::new(),
    })
}

pub fn run_operation(
    operation: &QueuedOperation,
    cancel: &CancelHandle,
    mut on_progress: impl FnMut(OperationProgress),
) -> Result<(), ManagerError> {
    let program = choco_program()
        .ok_or_else(|| ManagerError::Unavailable("Chocolatey is not installed".into()))?;
    let verb = match operation.kind {
        OperationKind::Install => "install",
        OperationKind::Update => "upgrade",
        OperationKind::Uninstall => "uninstall",
    };
    let mut args = vec![
        verb.to_string(),
        operation.key.id.clone(),
        "--yes".into(),
        "--no-progress".into(),
        "--no-color".into(),
        "--limit-output".into(),
    ];
    if let Some(version) = operation
        .options
        .version
        .as_deref()
        .filter(|value| !value.is_empty())
        && matches!(
            operation.kind,
            OperationKind::Install | OperationKind::Update
        )
    {
        args.push(format!("--version={version}"));
    }
    if let Some(extra) = operation
        .options
        .extra_args
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        for part in extra.split_whitespace() {
            args.push(part.to_string());
        }
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    on_progress(OperationProgress {
        stage: ProgressStage::Queued,
        percent: None,
    });
    run_collect(&program, &arg_refs, Some(cancel), Some(&mut on_progress))?;
    on_progress(OperationProgress {
        stage: ProgressStage::Finishing,
        percent: Some(100),
    });
    Ok(())
}

fn choco_program() -> Option<PathBuf> {
    manager_config::resolve_program(ManagerKind::Chocolatey, || find_on_path("choco"))
}

fn version_ok(program: &Path) -> Result<bool, ManagerError> {
    let text = run_output(program, &["--version"]).or_else(|_| run_output(program, &["-v"]))?;
    Ok(parse_choco_major(&text).is_some_and(|major| major >= MIN_MAJOR))
}

fn parse_choco_major(text: &str) -> Option<u64> {
    let line = text.lines().next()?.trim();
    let digits = line
        .trim_start_matches("Chocolatey")
        .trim_start()
        .trim_start_matches('v')
        .trim();
    let major = digits.split(['.', ' ', '-']).next()?;
    major.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chocolatey_major_versions() {
        assert_eq!(parse_choco_major("2.2.2\n"), Some(2));
        assert_eq!(parse_choco_major("Chocolatey v1.4.0"), Some(1));
        assert_eq!(parse_choco_major("0.10.15"), Some(0));
    }
}
