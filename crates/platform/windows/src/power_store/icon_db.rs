//! Remote package-icon lookup using UniGetUI's public icon database.
//!
//! The database (`screenshot-database-v2.json` from the MIT-licensed UniGetUI repository)
//! maps normalised package ids to logo URLs for WinGet, Scoop, Chocolatey, npm, pip,
//! Cargo and other managers. It is downloaded at most once a day into
//! `%LOCALAPPDATA%\PowerKit` and read only on a worker thread. Only the `icon` field of
//! each entry is used; PowerKit never uploads package names.
//!
//! Lookup keys follow UniGetUI's `GenerateIconId` and lookup order so the shared database
//! keeps working: `{Manager}.{id}`, the raw WinGet id, then the manager-specific icon id.
use super::file_write::write_replacing;
use powerkit_core::power_store::{ManagerKind, PackageKey, PackageSummary, split_scoop_id};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// UniGetUI's community icon database (MIT licensed, maintained upstream).
pub const DEFAULT_ICON_DB_URL: &str = "https://raw.githubusercontent.com/Devolutions/UniGetUI/main/WebBasedData/screenshot-database-v2.json";
const CACHE_FILE: &str = "icon-database.json";
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MEMORY_TTL: Duration = Duration::from_secs(60);
/// The live file is about 1.5 MiB; leave room for growth but bound a hostile response.
pub const MAX_DB_BYTES: usize = 8 * 1024 * 1024;

type IconMap = Arc<HashMap<String, String>>;

static MEMORY: Mutex<Option<(Instant, IconMap)>> = Mutex::new(None);

/// Icon URLs for the packages that have none yet. Loads (and, when stale, downloads) the
/// database, so call it from a worker thread only. Empty when the database is unavailable.
pub fn resolve(packages: &[PackageSummary]) -> Vec<(PackageKey, String)> {
    let wanted: Vec<&PackageSummary> = packages
        .iter()
        .filter(|package| package.icon.as_deref().is_none_or(str::is_empty))
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    let map = load();
    if map.is_empty() {
        return Vec::new();
    }
    wanted
        .into_iter()
        .filter_map(|package| lookup(&map, &package.key).map(|url| (package.key.clone(), url)))
        .collect()
}

fn lookup(map: &HashMap<String, String>, key: &PackageKey) -> Option<String> {
    lookup_ids(key).iter().find_map(|id| map.get(id)).cloned()
}

/// Database keys to try for a package, most specific first (UniGetUI's order).
pub fn lookup_ids(key: &PackageKey) -> Vec<String> {
    let id = manager_package_id(key);
    let mut ids = Vec::with_capacity(3);
    ids.push(format!("{}.{id}", manager_label(key.manager)).to_lowercase());
    if key.manager == ManagerKind::WinGet {
        ids.push(id.to_lowercase());
    }
    ids.push(icon_id(key));
    ids.dedup();
    ids
}

/// UniGetUI's manager name, the prefix of its manager-specific database keys.
fn manager_label(manager: ManagerKind) -> &'static str {
    match manager {
        ManagerKind::WinGet => "Winget",
        ManagerKind::Scoop => "Scoop",
        ManagerKind::Chocolatey => "Chocolatey",
        ManagerKind::Npm => "Npm",
        ManagerKind::Pip => "Pip",
        ManagerKind::Cargo => "Cargo",
        ManagerKind::DotnetTool => ".NET Tool",
        ManagerKind::PowerShell => "PowerShell",
        ManagerKind::PowerShell7 => "PowerShell7",
        ManagerKind::Vcpkg => "vcpkg",
    }
}

/// The id as UniGetUI sees it: Scoop ids drop the bucket that PowerKit keeps in the id.
fn manager_package_id(key: &PackageKey) -> &str {
    match key.manager {
        ManagerKind::Scoop => split_scoop_id(&key.id).1,
        _ => &key.id,
    }
}

/// UniGetUI's per-manager icon id: strips publisher prefixes and install suffixes, then
/// lowercases and turns separators into `-`.
pub fn icon_id(key: &PackageKey) -> String {
    let id = manager_package_id(key);
    let base = match key.manager {
        ManagerKind::WinGet => winget_icon_base(&key.source, id),
        ManagerKind::Scoop => id.replace(".app", ""),
        ManagerKind::Chocolatey => id.replace(".install", "").replace(".portable", ""),
        ManagerKind::Vcpkg => {
            let name = id.split(':').next().unwrap_or(id);
            name.split('[').next().unwrap_or(name).to_string()
        }
        _ => id.to_string(),
    };
    base.to_lowercase().replace(['_', '.', ' ', '/', ','], "-")
}

fn winget_icon_base(source: &str, id: &str) -> String {
    let last_segment = || id.rsplit('\\').next().unwrap_or(id);
    if source.eq_ignore_ascii_case("steam") {
        return last_segment()
            .to_lowercase()
            .replace("steam app ", "steam-")
            .trim()
            .to_string();
    }
    if source.eq_ignore_ascii_case("local") {
        // `MSIX\Publisher.Name_1.0.0.0_arm64__hash`: keep the package name only.
        if let Some(full_name) = id.strip_prefix("MSIX\\") {
            let identity = full_name.split('_').next().unwrap_or(full_name);
            return without_publisher(identity).to_string();
        }
        // `ARP\Machine\X64\Name_is1`: the installer suffix never appears in the database.
        let segment = last_segment();
        return segment.strip_suffix("_is1").unwrap_or(segment).to_string();
    }
    without_publisher(id).to_string()
}

/// `Publisher.Name` → `Name`; an id without a dot is returned unchanged.
fn without_publisher(id: &str) -> &str {
    id.split_once('.').map_or(id, |(_, name)| name)
}

fn load() -> IconMap {
    if let Ok(guard) = MEMORY.lock()
        && let Some((loaded_at, map)) = guard.as_ref()
        && loaded_at.elapsed() < MEMORY_TTL
    {
        return Arc::clone(map);
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
    // A failed refresh keeps serving the stale file instead of dropping every logo.
    let map: IconMap = Arc::new(
        path.as_ref()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| parse_icon_db(&text))
            .unwrap_or_default(),
    );
    if let Ok(mut guard) = MEMORY.lock() {
        *guard = Some((Instant::now(), Arc::clone(&map)));
    }
    map
}

fn refresh_from_network() -> bool {
    let Some(bytes) = super::icons::download_url(DEFAULT_ICON_DB_URL, MAX_DB_BYTES) else {
        return false;
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return false;
    };
    if parse_icon_db(&text).is_none_or(|map| map.is_empty()) {
        return false;
    }
    let Some(path) = cache_path() else {
        return false;
    };
    // Replacing in place could leave a truncated file with a fresh mtime, which would
    // suppress the daily refresh and drop every logo for a day.
    write_replacing(&path, &text).is_ok()
}

fn cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("PowerKit").join(CACHE_FILE))
}

/// Reads `icons_and_screenshots.<id>.icon`. Ids are case-folded because the upstream
/// file contains the same id in several spellings; the first spelling wins.
fn parse_icon_db(text: &str) -> Option<HashMap<String, String>> {
    let root = powerkit_core::json::parse(text)?;
    let entries = root.get("icons_and_screenshots")?.entries()?;
    let mut map = HashMap::new();
    for (key, entry) in entries {
        let Some(url) = entry.str_field("icon") else {
            continue;
        };
        // https only: the bytes go straight into an image decoder.
        if !url.starts_with("https://") {
            continue;
        }
        map.entry(key.to_lowercase()).or_insert(url);
    }
    Some(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(manager: ManagerKind, source: &str, id: &str) -> PackageKey {
        PackageKey::new(manager, source, id)
    }

    #[test]
    fn upstream_format_is_parsed_and_non_https_icons_are_dropped() {
        let text = r#"{
          "package_count": {"total": 3},
          "icons_and_screenshots": {
            "ripgrep": {"icon": "https://example.test/rg.png", "images": ["https://example.test/s.png"]},
            "Git": {"icon": "https://example.test/git.png", "images": []},
            "git": {"icon": "https://example.test/other.png", "images": []},
            "plain": {"icon": "http://example.test/p.png", "images": []},
            "empty": {"icon": "", "images": []}
          }
        }"#;
        let map = parse_icon_db(text).unwrap();
        assert_eq!(
            map.get("ripgrep").map(String::as_str),
            Some("https://example.test/rg.png")
        );
        assert_eq!(
            map.get("git").map(String::as_str),
            Some("https://example.test/git.png")
        );
        assert!(!map.contains_key("plain"));
        assert!(!map.contains_key("empty"));
        assert!(parse_icon_db("{\"icons\": {}}").is_none());
    }

    #[test]
    fn icon_ids_follow_unigetui_rules() {
        let id = |manager, source, id| icon_id(&key(manager, source, id));
        assert_eq!(id(ManagerKind::WinGet, "winget", "Git.Git"), "git");
        assert_eq!(
            id(ManagerKind::WinGet, "winget", "Microsoft.VisualStudioCode"),
            "visualstudiocode"
        );
        assert_eq!(id(ManagerKind::WinGet, "winget", "Foo.Bar.Baz"), "bar-baz");
        assert_eq!(
            id(ManagerKind::WinGet, "msstore", "9NBLGGH4NNS1"),
            "9nblggh4nns1"
        );
        assert_eq!(
            id(
                ManagerKind::WinGet,
                "local",
                "MSIX\\Microsoft.WindowsTerminal_1.2.3.4_arm64__8wekyb3d8bbwe"
            ),
            "windowsterminal"
        );
        assert_eq!(
            id(ManagerKind::WinGet, "local", "ARP\\Machine\\X64\\Git_is1"),
            "git"
        );
        assert_eq!(
            id(ManagerKind::WinGet, "Steam", "Steam App 220"),
            "steam-220"
        );
        assert_eq!(
            id(ManagerKind::Scoop, "scoop", "extras/Notepad.app"),
            "notepad"
        );
        assert_eq!(id(ManagerKind::Scoop, "scoop", "ripgrep"), "ripgrep");
        assert_eq!(
            id(ManagerKind::Chocolatey, "chocolatey", "git.install"),
            "git"
        );
        assert_eq!(
            id(ManagerKind::Vcpkg, "vcpkg", "zlib[core]:arm64-windows"),
            "zlib"
        );
        assert_eq!(id(ManagerKind::Npm, "npm", "@types/node"), "@types-node");
        assert_eq!(
            id(ManagerKind::Pip, "pip", "Requests_OAuthlib"),
            "requests-oauthlib"
        );
    }

    #[test]
    fn lookup_tries_manager_key_then_raw_winget_id_then_icon_id() {
        let ids = lookup_ids(&key(ManagerKind::WinGet, "winget", "Git.Git"));
        assert_eq!(ids, ["winget.git.git", "git.git", "git"]);
        let ids = lookup_ids(&key(ManagerKind::Scoop, "scoop", "extras/ripgrep"));
        assert_eq!(ids, ["scoop.ripgrep", "ripgrep"]);
    }

    #[test]
    fn resolve_uses_the_icon_id_for_packages_without_logos() {
        let map: HashMap<String, String> = HashMap::from([(
            "ripgrep".to_string(),
            "https://example.test/rg.png".to_string(),
        )]);
        assert_eq!(
            lookup(&map, &key(ManagerKind::Scoop, "scoop", "extras/ripgrep")).as_deref(),
            Some("https://example.test/rg.png")
        );
        assert!(lookup(&map, &key(ManagerKind::Npm, "npm", "left-pad")).is_none());
    }
}
