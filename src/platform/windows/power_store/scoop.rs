//! Scoop CLI adapter: export JSON, bucket manifests for updates/search, CLI for ops.
use super::broker::CancelHandle;
use super::cli_process::{find_on_path, run_collect, run_output};
use super::manager_config;
use super::winget::WinGetError;
use crate::power_store::{
    ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey, PackageSummary,
    ProgressStage, QueuedOperation, SCOOP_SOURCE, parse_scoop_export, scoop_package_id,
    split_scoop_id, version_looks_newer,
};
use std::path::{Path, PathBuf};

/// Installed, whether or not the manager is switched on.
pub fn is_available() -> bool {
    locate().is_some()
}

/// The Scoop program, whether or not the manager is switched on.
pub fn locate() -> Option<PathBuf> {
    manager_config::locate_program(ManagerKind::Scoop, discover_scoop)
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
        let bucket_path = manifest_dir(&entry.path());
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
            let manifest = parse_manifest(&text);
            let description = manifest_field(&manifest, "description").unwrap_or_default();
            let version = manifest_field(&manifest, "version");
            let name_match = stem.to_ascii_lowercase().contains(&needle);
            let desc_match = description.to_ascii_lowercase().contains(&needle);
            if !name_match && !desc_match {
                continue;
            }
            packages.push(PackageSummary {
                key: PackageKey::new(
                    ManagerKind::Scoop,
                    SCOOP_SOURCE,
                    scoop_package_id(&bucket, stem),
                ),
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
    let (bucket, name) = split_scoop_id(&key.id);
    let path = manifest_path(&root, bucket, name).unwrap_or_else(|| {
        manifest_dir(&root.join("buckets").join(bucket)).join(format!("{name}.json"))
    });
    let text = std::fs::read_to_string(&path).map_err(|error| {
        WinGetError::Operation(format!("Scoop manifest {}: {error}", path.display()))
    })?;
    let manifest = parse_manifest(&text);
    Ok(PackageDetails {
        id: key.id.clone(),
        name: key.id.clone(),
        version: manifest_field(&manifest, "version").unwrap_or_default(),
        publisher: String::new(),
        author: String::new(),
        description: manifest_field(&manifest, "description").unwrap_or_default(),
        homepage: manifest_field(&manifest, "homepage").unwrap_or_default(),
        publisher_url: String::new(),
        license: manifest_field(&manifest, "license").unwrap_or_default(),
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
    // The id already reads `bucket/name` for non-main buckets, which is the form
    // `scoop install` takes. Update and uninstall use the bare name, which Scoop resolves
    // from the installed app itself.
    let package = match operation.kind {
        OperationKind::Install => match operation
            .options
            .version
            .as_deref()
            .filter(|v| !v.is_empty())
        {
            Some(version) => format!("{}@{version}", operation.key.id),
            None => operation.key.id.clone(),
        },
        OperationKind::Update | OperationKind::Uninstall => {
            split_scoop_id(&operation.key.id).1.to_string()
        }
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
        let (bucket, name) = split_scoop_id(&package.key.id);
        let Some(available) = manifest_path(&root, bucket, name)
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| manifest_field(&parse_manifest(&text), "version"))
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

/// Where a bucket keeps its manifests. Scoop's buckets (`main`, `java`, …) put them in a
/// `bucket` folder; a bucket laid out flat keeps them at its root.
fn manifest_dir(bucket_root: &Path) -> PathBuf {
    let nested = bucket_root.join("bucket");
    if nested.is_dir() {
        nested
    } else {
        bucket_root.to_path_buf()
    }
}

/// The manifest for `name` in `bucket`, if the bucket has one.
fn manifest_path(root: &Path, bucket: &str, name: &str) -> Option<PathBuf> {
    let path = manifest_dir(&root.join("buckets").join(bucket)).join(format!("{name}.json"));
    path.is_file().then_some(path)
}

fn scoop_program() -> Option<PathBuf> {
    manager_config::resolve_program(ManagerKind::Scoop, discover_scoop)
}

fn discover_scoop() -> Option<PathBuf> {
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

/// A manifest that fails to parse reads as empty, so its fields are simply missing.
fn parse_manifest(text: &str) -> crate::json::Value {
    crate::json::parse(text).unwrap_or(crate::json::Value::Null)
}

/// A top-level string of a Scoop manifest; `license` may also be `{ "identifier": … }`.
fn manifest_field(manifest: &crate::json::Value, field: &str) -> Option<String> {
    manifest
        .str_field(field)
        .or_else(|| manifest.get(field)?.str_field("identifier"))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_are_found_in_the_bucket_subfolder_or_flat() {
        let root = std::env::temp_dir().join(format!("powerkit-scoop-{}", std::process::id()));
        let nested = root.join("buckets").join("main").join("bucket");
        let flat = root.join("buckets").join("extras");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(&flat).unwrap();
        std::fs::write(nested.join("git.json"), "{ \"version\": \"2.56.1\" }").unwrap();
        std::fs::write(flat.join("tool.json"), "{ \"version\": \"1.0\" }").unwrap();

        assert_eq!(
            manifest_path(&root, "main", "git"),
            Some(nested.join("git.json"))
        );
        assert_eq!(
            manifest_path(&root, "extras", "tool"),
            Some(flat.join("tool.json"))
        );
        assert_eq!(manifest_path(&root, "main", "missing"), None);
        assert_eq!(manifest_dir(&root.join("buckets").join("main")), nested);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn manifest_fields_are_top_level_strings_or_license_objects() {
        let manifest = parse_manifest(
            r#"{ "version": "1.2.3", "homepage": "https://example.com",
                 "license": { "identifier": "MIT", "url": "https://x" },
                 "autoupdate": { "version": "9.9.9" } }"#,
        );
        assert_eq!(
            manifest_field(&manifest, "version").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            manifest_field(&manifest, "homepage").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(manifest_field(&manifest, "license").as_deref(), Some("MIT"));
        assert_eq!(manifest_field(&parse_manifest("not json"), "version"), None);
    }
}
