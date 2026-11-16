//! Scoop CLI adapter: export JSON, bucket manifests for updates/search, CLI for ops.
use super::broker::CancelHandle;
use super::cli_process::{find_on_path, run_collect, run_output};
use super::winget::WinGetError;
use crate::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, parse_scoop_export, version_looks_newer,
};
use std::path::PathBuf;

pub fn is_available() -> bool {
    scoop_program().is_some()
}

pub fn list_installed() -> Result<Option<Vec<PackageSummary>>, WinGetError> {
    let Some(program) = scoop_program() else {
        return Ok(None);
    };
    let text = run_output(&program, &["export"])?;
    let mut packages = parse_scoop_export(&text)
        .ok_or_else(|| WinGetError::Operation("Scoop export JSON could not be parsed".into()))?;
    apply_bucket_updates(&mut packages);
    Ok(Some(packages))
}

pub fn search(query: &str) -> Result<Option<Vec<PackageSummary>>, WinGetError> {
    if scoop_program().is_none() {
        return Ok(None);
    }
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let root = scoop_root()
        .ok_or_else(|| WinGetError::Operation("Scoop root directory was not found".into()))?;
    let buckets = root.join("buckets");
    let mut packages = Vec::new();
    let Ok(bucket_dirs) = std::fs::read_dir(&buckets) else {
        return Ok(Some(packages));
    };
    for entry in bucket_dirs.flatten() {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let bucket = entry.file_name().to_string_lossy().into_owned();
        let bucket_path = entry.path();
        let Ok(files) = std::fs::read_dir(&bucket_path) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default();
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let description = json_loose_string(&text, "description").unwrap_or_default();
            let version = json_loose_string(&text, "version");
            let name_match = stem.to_ascii_lowercase().contains(&needle);
            let desc_match = description.to_ascii_lowercase().contains(&needle);
            if !name_match && !desc_match {
                continue;
            }
            packages.push(PackageSummary {
                key: PackageKey::new(ManagerKind::Scoop, bucket.clone(), stem),
                name: stem.to_string(),
                version: None,
                available_version: version,
                update_available: false,
                scope: Some("user".into()),
                install_date: None,
                updated_at: None,
                icon: None,
            });
            if packages.len() >= 200 {
                return Ok(Some(packages));
            }
        }
    }
    Ok(Some(packages))
}

pub fn details(key: &PackageKey) -> Result<PackageDetails, WinGetError> {
    let root = scoop_root()
        .ok_or_else(|| WinGetError::Operation("Scoop root directory was not found".into()))?;
    let path = root
        .join("buckets")
        .join(&key.source)
        .join(format!("{}.json", key.id));
    let text = std::fs::read_to_string(&path).map_err(|error| {
        WinGetError::Operation(format!("Scoop manifest {}: {error}", path.display()))
    })?;
    Ok(PackageDetails {
        id: key.id.clone(),
        name: key.id.clone(),
        version: json_loose_string(&text, "version").unwrap_or_default(),
        publisher: String::new(),
        author: String::new(),
        description: json_loose_string(&text, "description").unwrap_or_default(),
        homepage: json_loose_string(&text, "homepage").unwrap_or_default(),
        publisher_url: String::new(),
        license: json_loose_string(&text, "license").unwrap_or_default(),
        license_url: String::new(),
    })
}

pub fn run_operation(
    operation: &QueuedOperation,
    cancel: &CancelHandle,
    mut on_progress: impl FnMut(OperationProgress),
) -> Result<(), WinGetError> {
    let program =
        scoop_program().ok_or_else(|| WinGetError::Unavailable("Scoop is not installed".into()))?;
    let verb = match operation.kind {
        OperationKind::Install => "install",
        OperationKind::Update => "update",
        OperationKind::Uninstall => "uninstall",
    };
    let package = if operation.kind == OperationKind::Install {
        match operation
            .options
            .version
            .as_deref()
            .filter(|v| !v.is_empty())
        {
            Some(version) if operation.key.source.is_empty() || operation.key.source == "main" => {
                format!("{}@{version}", operation.key.id)
            }
            Some(version) => format!("{}/{}@{version}", operation.key.source, operation.key.id),
            None if operation.key.source.is_empty() || operation.key.source == "main" => {
                operation.key.id.clone()
            }
            None => format!("{}/{}", operation.key.source, operation.key.id),
        }
    } else if operation.key.source.is_empty() || operation.key.source == "main" {
        operation.key.id.clone()
    } else {
        format!("{}/{}", operation.key.source, operation.key.id)
    };
    on_progress(OperationProgress {
        stage: ProgressStage::Queued,
        percent: None,
    });
    run_collect(
        &program,
        &[verb, package.as_str()],
        Some(cancel),
        Some(&mut on_progress),
    )?;
    on_progress(OperationProgress {
        stage: ProgressStage::Finishing,
        percent: Some(100),
    });
    Ok(())
}

fn apply_bucket_updates(packages: &mut [PackageSummary]) {
    let Some(root) = scoop_root() else {
        return;
    };
    for package in packages {
        let bucket_manifest = root
            .join("buckets")
            .join(&package.key.source)
            .join(format!("{}.json", package.key.id));
        let Some(available) = std::fs::read_to_string(&bucket_manifest)
            .ok()
            .and_then(|text| json_loose_string(&text, "version"))
        else {
            continue;
        };
        let installed = package.version.as_deref().unwrap_or_default();
        if version_looks_newer(&available, installed) {
            package.available_version = Some(available);
            package.update_available = true;
        } else {
            package.available_version = Some(available);
            package.update_available = false;
        }
    }
}

fn scoop_program() -> Option<PathBuf> {
    if let Some(path) = find_on_path("scoop") {
        return Some(path);
    }
    let root = scoop_root()?;
    for name in ["scoop.cmd", "scoop.ps1", "scoop.exe"] {
        let candidate = root.join("shims").join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn scoop_root() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("SCOOP") {
        let root = PathBuf::from(path);
        if root.is_dir() {
            return Some(root);
        }
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let root = PathBuf::from(home).join("scoop");
    root.is_dir().then_some(root)
}

/// Minimal `"field": "value"` extractor that tolerates Scoop manifest JSON.
fn json_loose_string(text: &str, field: &str) -> Option<String> {
    let patterns = [format!("\"{field}\""), format!("'{field}'")];
    for pattern in patterns {
        let Some(index) = text.find(&pattern) else {
            continue;
        };
        let rest = text[index + pattern.len()..].trim_start();
        let rest = rest.strip_prefix(':')?.trim_start();
        if !rest.starts_with('"') {
            continue;
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
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    other => out.push(other),
                },
                other => out.push(other),
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loose_json_reads_version_and_homepage() {
        let text =
            r#"{ "version": "1.2.3", "homepage": "https://example.com", "description": "tool" }"#;
        assert_eq!(json_loose_string(text, "version").as_deref(), Some("1.2.3"));
        assert_eq!(
            json_loose_string(text, "homepage").as_deref(),
            Some("https://example.com")
        );
    }
}
