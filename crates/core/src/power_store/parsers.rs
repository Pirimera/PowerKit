//! Parsers for the text and JSON that package-manager CLIs print.

use super::*;

/// Parse `scoop export` JSON into package summaries (updates filled by the adapter).
pub fn parse_scoop_export(text: &str) -> Option<Vec<PackageSummary>> {
    let root = json::parse(text)?;
    let apps = root.get("apps").or_else(|| root.get("Apps"))?;
    apps.as_array()?;
    let mut packages = Vec::new();
    for object in apps.objects() {
        let id = object.str_any(&["Name", "name"])?;
        let bucket = object
            .str_any(&["Source", "source", "Bucket", "bucket"])
            .unwrap_or_else(|| "main".into());
        let version = object.str_any(&["Version", "version"]);
        packages.push(PackageSummary {
            key: PackageKey::new(
                ManagerKind::Scoop,
                SCOOP_SOURCE,
                scoop_package_id(&bucket, &id),
            ),
            name: id,
            version,
            available_version: None,
            update_available: false,
            scope: Some("user".into()),
            install_date: None,
            updated_at: None,
            icon: None,
        });
    }
    Some(packages)
}

/// Parse Chocolatey `--limit-output` `id|version` rows (list or search).
pub fn parse_choco_limit_rows(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("Chocolatey") {
                return None;
            }
            let (id, version) = line.split_once('|')?;
            let id = id.trim();
            let version = version.trim();
            if id.is_empty() {
                return None;
            }
            Some((id.to_string(), version.to_string()))
        })
        .collect()
}

/// Parse `choco outdated -r` rows: `id|installed|latest|pinned`.
pub fn parse_choco_outdated_rows(text: &str) -> Vec<(String, String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("Chocolatey") {
                return None;
            }
            let mut parts = line.split('|');
            let id = parts.next()?.trim();
            let installed = parts.next()?.trim();
            let latest = parts.next()?.trim();
            if id.is_empty() {
                return None;
            }
            Some((id.to_string(), installed.to_string(), latest.to_string()))
        })
        .collect()
}

/// True when `available` looks newer than `installed` for simple dotted versions.
pub fn version_looks_newer(available: &str, installed: &str) -> bool {
    let parse = |value: &str| -> Option<Vec<u64>> {
        let parts: Vec<u64> = value
            .split(|c: char| !c.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .map(|part| part.parse().ok())
            .collect::<Option<_>>()?;
        (!parts.is_empty()).then_some(parts)
    };
    match (parse(available), parse(installed)) {
        (Some(left), Some(right)) => left > right,
        _ => available != installed && !available.is_empty(),
    }
}

/// `npm list -g --depth=0 --json` → dependency name/version pairs.
pub fn parse_npm_list_json(text: &str) -> Vec<(String, String)> {
    let Some(root) = json::parse(text) else {
        return Vec::new();
    };
    let Some(deps) = root.get("dependencies").and_then(json::Value::entries) else {
        return Vec::new();
    };
    deps.iter()
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, object)| {
            (
                name.clone(),
                object.str_field("version").unwrap_or_default(),
            )
        })
        .collect()
}

/// `npm outdated -g --json` → id → latest (prefers `latest`, else `wanted`).
pub fn parse_npm_outdated_json(text: &str) -> Vec<(String, String)> {
    let Some(root) = json::parse(text) else {
        return Vec::new();
    };
    let mut packages = Vec::new();
    for (name, object) in root.entries().unwrap_or_default() {
        let name = name.clone();
        let latest = object.str_any(&["latest", "wanted"]).unwrap_or_default();
        if !name.is_empty() && !latest.is_empty() {
            packages.push((name, latest));
        }
    }
    packages
}

/// `npm search --json` array of `{name, version, description}`.
pub fn parse_npm_search_json(text: &str) -> Vec<(String, String, String)> {
    let root = json::parse(text).unwrap_or(json::Value::Null);
    let mut packages = Vec::new();
    for object in root.objects() {
        let Some(name) = object.str_field("name") else {
            continue;
        };
        let version = object.str_field("version").unwrap_or_default();
        let description = object.str_field("description").unwrap_or_default();
        packages.push((name, version, description));
    }
    packages
}

/// `pip list --format=json` → name/version.
pub fn parse_pip_list_json(text: &str) -> Vec<(String, String)> {
    let root = json::parse(text).unwrap_or(json::Value::Null);
    let mut packages = Vec::new();
    for object in root.objects() {
        let Some(name) = object.str_field("name") else {
            continue;
        };
        let version = object.str_field("version").unwrap_or_default();
        packages.push((name, version));
    }
    packages
}

/// `pip list --outdated --format=json` → name/installed/latest.
pub fn parse_pip_outdated_json(text: &str) -> Vec<(String, String, String)> {
    let root = json::parse(text).unwrap_or(json::Value::Null);
    let mut packages = Vec::new();
    for object in root.objects() {
        let Some(name) = object.str_field("name") else {
            continue;
        };
        let version = object.str_field("version").unwrap_or_default();
        let latest = object.str_field("latest_version").unwrap_or_default();
        if !latest.is_empty() {
            packages.push((name, version, latest));
        }
    }
    packages
}

/// `cargo install --list` blocks: `name vX.Y.Z:` then indented binaries.
pub fn parse_cargo_install_list(text: &str) -> Vec<(String, String)> {
    let mut packages = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with([' ', '\t']) {
            continue;
        }
        let Some(name_ver) = line.strip_suffix(':') else {
            continue;
        };
        let Some((name, version)) = name_ver.rsplit_once(' ') else {
            continue;
        };
        let version = version.trim().trim_start_matches('v');
        let name = name.trim();
        if !name.is_empty() {
            packages.push((name.to_string(), version.to_string()));
        }
    }
    packages
}

/// `cargo search` lines: `name = "version"    # description`.
pub fn parse_cargo_search(text: &str) -> Vec<(String, String, String)> {
    let mut packages = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("...") {
            continue;
        }
        let Some((left, rest)) = line.split_once('=') else {
            continue;
        };
        let name = left.trim();
        let rest = rest.trim();
        if !rest.starts_with('"') {
            continue;
        }
        let mut version = String::new();
        let mut chars = rest[1..].chars();
        while let Some(ch) = chars.next() {
            if ch == '"' {
                break;
            }
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    version.push(next);
                }
            } else {
                version.push(ch);
            }
        }
        let description = rest
            .find('#')
            .map(|index| rest[index + 1..].trim().to_string())
            .unwrap_or_default();
        if !name.is_empty() {
            packages.push((name.to_string(), version, description));
        }
    }
    packages
}

/// `dotnet tool list -g` table rows after the separator.
pub fn parse_dotnet_tool_list(text: &str) -> Vec<(String, String)> {
    let mut packages = Vec::new();
    let mut past_header = false;
    for line in text.lines() {
        let line = line.trim_end();
        if line.chars().all(|c| c == '-' || c == ' ') && line.contains('-') {
            past_header = true;
            continue;
        }
        if !past_header || line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(id) = parts.next() else {
            continue;
        };
        let Some(version) = parts.next() else {
            continue;
        };
        packages.push((id.to_string(), version.to_string()));
    }
    packages
}

/// `dotnet tool search` table: PackageId Version Authors Downloads Verified.
pub fn parse_dotnet_tool_search(text: &str) -> Vec<(String, String)> {
    parse_dotnet_tool_list(text)
}

/// PowerShell `ConvertTo-Json` module objects → Name/Version pairs.
pub fn parse_powershell_modules_json(text: &str) -> Vec<(String, String)> {
    let Some(root) = json::parse(text) else {
        return Vec::new();
    };
    // ConvertTo-Json writes one object bare and several as an array.
    let objects: Vec<&json::Value> = match &root {
        json::Value::Object(_) => vec![&root],
        _ => root.objects().collect(),
    };
    let mut packages = Vec::new();
    for object in objects {
        let name = object.str_any(&["Name", "name"]).unwrap_or_default();
        let version = object.str_any(&["Version", "version"]).unwrap_or_default();
        if !name.is_empty() {
            packages.push((name, version));
        }
    }
    packages
}

/// `vcpkg list` lines: `name:triplet  version  description`.
pub fn parse_vcpkg_list(text: &str) -> Vec<(String, String, String)> {
    let mut packages = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(id) = parts.next() else {
            continue;
        };
        if !id.contains(':') {
            continue;
        }
        let version = parts.next().unwrap_or("").to_string();
        let description = parts.collect::<Vec<_>>().join(" ");
        packages.push((id.to_string(), version, description));
    }
    packages
}
