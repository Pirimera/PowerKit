//! Global npm packages (`npm list/outdated/search/install -g`).
use super::broker::CancelHandle;
use super::cli_process::{find_on_path, run_collect, run_output};
use super::winget::WinGetError;
use crate::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, parse_npm_list_json, parse_npm_outdated_json,
    parse_npm_search_json, version_looks_newer,
};
use std::path::PathBuf;

const SOURCE: &str = "npm";

pub fn is_available() -> bool {
    npm_program().is_some()
}

pub fn list_installed() -> Result<Option<Vec<PackageSummary>>, WinGetError> {
    let Some(program) = npm_program() else {
        return Ok(None);
    };
    let listed = run_output(&program, &["list", "-g", "--depth=0", "--json"])?;
    let outdated = run_output(&program, &["outdated", "-g", "--json"]).unwrap_or_default();
    let updates = parse_npm_outdated_json(&outdated);
    let mut packages = Vec::new();
    for (id, version) in parse_npm_list_json(&listed) {
        let latest = updates
            .iter()
            .find(|(name, _)| name == &id)
            .map(|(_, latest)| latest.clone());
        let update_available = latest
            .as_deref()
            .is_some_and(|latest| version_looks_newer(latest, &version));
        packages.push(PackageSummary {
            key: PackageKey::new(ManagerKind::Npm, SOURCE, id.clone()),
            name: id,
            version: Some(version),
            available_version: latest,
            update_available,
            scope: Some("user".into()),
            install_date: None,
            updated_at: None,
        });
    }
    Ok(Some(packages))
}

pub fn search(query: &str) -> Result<Option<Vec<PackageSummary>>, WinGetError> {
    let Some(program) = npm_program() else {
        return Ok(None);
    };
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let text = run_output(
        &program,
        &["search", trimmed, "--json", "--searchlimit", "50"],
    )?;
    let packages = parse_npm_search_json(&text)
        .into_iter()
        .take(200)
        .map(|(id, version, _)| PackageSummary {
            key: PackageKey::new(ManagerKind::Npm, SOURCE, id.clone()),
            name: id,
            version: None,
            available_version: Some(version).filter(|value| !value.is_empty()),
            update_available: false,
            scope: Some("user".into()),
            install_date: None,
            updated_at: None,
        })
        .collect();
    Ok(Some(packages))
}

pub fn details(key: &PackageKey) -> Result<PackageDetails, WinGetError> {
    let program =
        npm_program().ok_or_else(|| WinGetError::Unavailable("npm is not installed".into()))?;
    let text = run_output(&program, &["view", &key.id, "--json"]).unwrap_or_default();
    Ok(PackageDetails {
        id: key.id.clone(),
        name: json_string(&text, "name").unwrap_or_else(|| key.id.clone()),
        version: json_string(&text, "version").unwrap_or_default(),
        publisher: String::new(),
        author: json_string(&text, "author").unwrap_or_default(),
        description: json_string(&text, "description").unwrap_or_default(),
        homepage: json_string(&text, "homepage").unwrap_or_default(),
        publisher_url: String::new(),
        license: json_string(&text, "license").unwrap_or_default(),
        license_url: String::new(),
    })
}

pub fn run_operation(
    operation: &QueuedOperation,
    cancel: &CancelHandle,
    mut on_progress: impl FnMut(OperationProgress),
) -> Result<(), WinGetError> {
    let program =
        npm_program().ok_or_else(|| WinGetError::Unavailable("npm is not installed".into()))?;
    let args: Vec<&str> = match operation.kind {
        OperationKind::Install => vec!["install", "-g", operation.key.id.as_str()],
        OperationKind::Update => vec!["update", "-g", operation.key.id.as_str()],
        OperationKind::Uninstall => vec!["uninstall", "-g", operation.key.id.as_str()],
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

fn npm_program() -> Option<PathBuf> {
    find_on_path("npm")
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
