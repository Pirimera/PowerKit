//! Optional remote package-icon map (default off).
//!
//! When enabled, fills missing [`PackageSummary::icon`] URLs from a small JSON
//! database cached under `%LOCALAPPDATA%\PowerKit`. PowerKit does not ship a
//! third-party icon CDN; the default URL points at this repository's empty map
//! so maintainers can grow entries deliberately.
use crate::power_store::PackageSummary;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

/// Maintainer-owned map in this repository (empty by default).
pub const DEFAULT_ICON_DB_URL: &str =
    "https://raw.githubusercontent.com/Pirimera/PowerKit/main/data/package-icons.json";
const CACHE_FILE: &str = "package-icons.json";
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_DB_BYTES: u64 = 2 * 1024 * 1024;

static MEMORY: Mutex<Option<(Instant, BTreeMap<String, String>)>> = Mutex::new(None);

/// Fills empty icon slots from the cached remote map. No-op when the map is empty.
pub fn apply(packages: &mut [PackageSummary]) {
    let map = load();
    if map.is_empty() {
        return;
    }
    for package in packages {
        if package.icon.as_ref().is_some_and(|value| !value.is_empty()) {
            continue;
        }
        if let Some(url) = lookup(&map, package) {
            package.icon = Some(url);
        }
    }
}

fn lookup(map: &BTreeMap<String, String>, package: &PackageSummary) -> Option<String> {
    let manager = package.key.manager.as_str();
    let id = package.key.id.as_str();
    let candidates = [
        format!("{manager}:{id}").to_ascii_lowercase(),
        format!("{manager}.{id}").to_ascii_lowercase(),
        normalize_id(id),
        id.to_ascii_lowercase(),
    ];
    for key in candidates {
        if let Some(url) = map.get(&key) {
            return Some(url.clone());
        }
    }
    None
}

/// UniGetUI-style normalization so shared community keys can be adopted later.
fn normalize_id(id: &str) -> String {
    let mut value = id.to_ascii_lowercase();
    for suffix in [".install", ".portable", ".app"] {
        if let Some(stripped) = value.strip_suffix(suffix) {
            value = stripped.to_string();
        }
    }
    if let Some((name, _)) = value.split_once(':') {
        value = name.to_string();
    }
    if let Some((_, rest)) = value.split_once('.') {
        // Drop publisher prefix when present (`Git.Git` → `git`).
        value = rest.to_string();
    }
    value.replace(['_', '.', ' ', '/'], "-")
}

fn load() -> BTreeMap<String, String> {
    if let Ok(guard) = MEMORY.lock()
        && let Some((loaded_at, map)) = guard.as_ref()
        && loaded_at.elapsed() < Duration::from_secs(60)
    {
        return map.clone();
    }
    let path = cache_path();
    let fresh_enough = path
        .as_ref()
        .and_then(|path| fs::metadata(path).ok())
        .and_then(|meta| meta.modified().ok())
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < MAX_AGE);
    if !fresh_enough {
        let _ = refresh_from_network();
    }
    let map = path
        .as_ref()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| parse_icon_db(&text))
        .unwrap_or_default();
    if let Ok(mut guard) = MEMORY.lock() {
        *guard = Some((Instant::now(), map.clone()));
    }
    map
}

fn refresh_from_network() -> bool {
    let Some(bytes) = super::icons::download_url(DEFAULT_ICON_DB_URL) else {
        return false;
    };
    if bytes.len() as u64 > MAX_DB_BYTES {
        return false;
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return false;
    };
    if parse_icon_db(&text).is_none() {
        return false;
    }
    let Some(path) = cache_path() else {
        return false;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(path, text).is_ok()
}

fn cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("PowerKit").join(CACHE_FILE))
}

fn parse_icon_db(text: &str) -> Option<BTreeMap<String, String>> {
    let icons = json_object_field(text, "icons")?;
    let mut map = BTreeMap::new();
    for (key, raw) in json_object_entries(icons) {
        let url = json_string(raw)?;
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            continue;
        }
        map.insert(key.to_ascii_lowercase(), url);
    }
    Some(map)
}

fn json_string(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let inner = trimmed.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16).ok()?;
                    out.push(char::from_u32(code)?);
                }
                _ => return None,
            }
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

fn json_object_field<'a>(object: &'a str, field: &str) -> Option<&'a str> {
    for (key, value) in json_object_entries(object) {
        if key == field {
            return Some(value);
        }
    }
    None
}

fn json_object_entries(object: &str) -> Vec<(String, &str)> {
    let trimmed = object.trim();
    let body = trimmed
        .strip_prefix('{')
        .and_then(|text| text.strip_suffix('}'))
        .unwrap_or(trimmed);
    let mut entries = Vec::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;
    let mut key_start: Option<usize> = None;
    let mut current_key: Option<String> = None;
    let mut value_start: Option<usize> = None;
    let bytes = body.as_bytes();
    let mut index = 0usize;
    while index < body.len() {
        let ch = body[index..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
                if depth == 0
                    && current_key.is_none()
                    && let Some(start) = key_start
                {
                    current_key = Some(body[start + 1..index].to_string());
                    key_start = None;
                }
            }
            index += ch_len;
            continue;
        }
        match ch {
            '"' if depth == 0 && current_key.is_none() => {
                key_start = Some(index);
                in_string = true;
            }
            '"' => in_string = true,
            ':' if depth == 0 && current_key.is_some() && value_start.is_none() => {
                let mut start = index + ch_len;
                while start < body.len() && bytes[start].is_ascii_whitespace() {
                    start += 1;
                }
                value_start = Some(start);
            }
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if let (Some(key), Some(start)) = (current_key.take(), value_start.take()) {
                    entries.push((key, body[start..index].trim()));
                }
            }
            _ => {}
        }
        index += ch_len;
    }
    if let (Some(key), Some(start)) = (current_key, value_start) {
        entries.push((key, body[start..].trim()));
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power_store::{ManagerKind, PackageKey};

    #[test]
    fn icon_db_parses_https_entries_and_looks_up_normalized_ids() {
        let text = r#"{
          "schema": 1,
          "icons": {
            "winget:Git.Git": "https://example.test/git.png",
            "ripgrep": "https://example.test/rg.png",
            "bad": "ftp://nope"
          }
        }"#;
        let map = parse_icon_db(text).unwrap();
        assert_eq!(
            map.get("winget:git.git").map(String::as_str),
            Some("https://example.test/git.png")
        );
        assert_eq!(
            map.get("ripgrep").map(String::as_str),
            Some("https://example.test/rg.png")
        );
        assert!(!map.contains_key("bad"));

        let package = PackageSummary {
            key: PackageKey::new(ManagerKind::Scoop, "main", "ripgrep"),
            name: "ripgrep".into(),
            version: Some("1".into()),
            available_version: None,
            update_available: false,
            scope: None,
            install_date: None,
            updated_at: None,
            icon: None,
        };
        // apply() reads disk/network; exercise lookup helper instead.
        assert_eq!(
            lookup(&map, &package).as_deref(),
            Some("https://example.test/rg.png")
        );
        assert_eq!(normalize_id("Git.Git"), "git");
        assert_eq!(normalize_id("foo.install"), "foo");
    }
}
