//! PowerStore package model, filters, queue and export helpers (no Windows FFI).
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// PowerKit's own package id is never offered for update or uninstall.
pub const OWN_PACKAGE_ID: &str = "Pirimera.PowerKit";

pub const DEFAULT_CHECK_MINUTES: u32 = 360;
pub const MIN_CHECK_MINUTES: u32 = 15;
pub const MAX_CHECK_MINUTES: u32 = 7 * 24 * 60;
pub const MAX_IGNORE_ENTRIES: usize = 512;
pub const MAX_SKIP_ENTRIES: usize = 512;
pub const MAX_HISTORY_OPERATIONS: usize = 200;
pub const MAX_HISTORY_LOG_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StoreSection {
    #[default]
    Discover,
    Installed,
    Updates,
}

impl StoreSection {
    pub fn index(self) -> usize {
        match self {
            Self::Discover => 0,
            Self::Installed => 1,
            Self::Updates => 2,
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Some(match index {
            0 => Self::Discover,
            1 => Self::Installed,
            2 => Self::Updates,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ManagerKind {
    WinGet,
    Scoop,
    Chocolatey,
    Npm,
    Pip,
    Cargo,
    DotnetTool,
    PowerShell,
    PowerShell7,
    Vcpkg,
}

impl ManagerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WinGet => "winget",
            Self::Scoop => "scoop",
            Self::Chocolatey => "chocolatey",
            Self::Npm => "npm",
            Self::Pip => "pip",
            Self::Cargo => "cargo",
            Self::DotnetTool => "dotnet",
            Self::PowerShell => "powershell",
            Self::PowerShell7 => "powershell7",
            Self::Vcpkg => "vcpkg",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "winget" => Self::WinGet,
            "scoop" => Self::Scoop,
            "chocolatey" => Self::Chocolatey,
            "npm" => Self::Npm,
            "pip" => Self::Pip,
            "cargo" => Self::Cargo,
            "dotnet" => Self::DotnetTool,
            "powershell" => Self::PowerShell,
            "powershell7" => Self::PowerShell7,
            "vcpkg" => Self::Vcpkg,
            _ => return None,
        })
    }

    pub fn phase1(self) -> bool {
        matches!(self, Self::WinGet)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageKey {
    pub manager: ManagerKind,
    pub source: String,
    pub id: String,
}

impl PackageKey {
    pub fn new(manager: ManagerKind, source: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            manager,
            source: source.into(),
            id: id.into(),
        }
    }

    pub fn is_own_package(&self) -> bool {
        self.manager == ManagerKind::WinGet && self.id.eq_ignore_ascii_case(OWN_PACKAGE_ID)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageSummary {
    pub key: PackageKey,
    pub name: String,
    pub version: Option<String>,
    pub available_version: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstallOptions {
    pub scope: Option<String>,
    pub architecture: Option<String>,
    pub version: Option<String>,
    pub extra_args: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Install,
    Update,
    Uninstall,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedOperation {
    pub kind: OperationKind,
    pub key: PackageKey,
    pub name: String,
    pub options: InstallOptions,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OperationQueue {
    items: VecDeque<QueuedOperation>,
}

impl OperationQueue {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn push(&mut self, operation: QueuedOperation) {
        if operation.key.is_own_package()
            && matches!(
                operation.kind,
                OperationKind::Update | OperationKind::Uninstall
            )
        {
            return;
        }
        self.items.push_back(operation);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn pop_front(&mut self) -> Option<QueuedOperation> {
        self.items.pop_front()
    }

    pub fn remove_at(&mut self, index: usize) -> Option<QueuedOperation> {
        if index >= self.items.len() {
            return None;
        }
        self.items.remove(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &QueuedOperation> {
        self.items.iter()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateFilters {
    ignored: BTreeSet<PackageKey>,
    skipped: BTreeMap<PackageKey, String>,
}

impl UpdateFilters {
    pub fn ignore(&mut self, key: PackageKey) -> bool {
        if self.ignored.len() >= MAX_IGNORE_ENTRIES && !self.ignored.contains(&key) {
            return false;
        }
        self.skipped.remove(&key);
        self.ignored.insert(key);
        true
    }

    pub fn clear_ignore(&mut self, key: &PackageKey) {
        self.ignored.remove(key);
    }

    pub fn skip_version(&mut self, key: PackageKey, version: impl Into<String>) -> bool {
        if self.ignored.contains(&key) {
            return true;
        }
        if self.skipped.len() >= MAX_SKIP_ENTRIES && !self.skipped.contains_key(&key) {
            return false;
        }
        self.skipped.insert(key, version.into());
        true
    }

    pub fn clear_skip(&mut self, key: &PackageKey) {
        self.skipped.remove(key);
    }

    pub fn blocks_update(&self, key: &PackageKey, available_version: &str) -> bool {
        if key.is_own_package() || self.ignored.contains(key) {
            return true;
        }
        self.skipped
            .get(key)
            .is_some_and(|skipped| skipped == available_version)
    }

    pub fn ignored(&self) -> impl Iterator<Item = &PackageKey> {
        self.ignored.iter()
    }

    pub fn skipped(&self) -> impl Iterator<Item = (&PackageKey, &String)> {
        self.skipped.iter()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerStorePreferences {
    pub check_minutes: u32,
    pub filters: UpdateFilters,
    pub options: BTreeMap<PackageKey, InstallOptions>,
}

impl Default for PowerStorePreferences {
    fn default() -> Self {
        Self {
            check_minutes: DEFAULT_CHECK_MINUTES,
            filters: UpdateFilters::default(),
            options: BTreeMap::new(),
        }
    }
}

impl PowerStorePreferences {
    pub fn append_settings(&self, body: &mut String) {
        body.push_str(&format!(
            "power_store_check_minutes={}\n",
            self.check_minutes
        ));
        for key in self.filters.ignored() {
            if let Some(encoded) = encode_package_key(key) {
                body.push_str("power_store_ignore=");
                body.push_str(&encoded);
                body.push('\n');
            }
        }
        for (key, version) in self.filters.skipped() {
            if version.contains('|') || version.contains('\n') {
                continue;
            }
            if let Some(encoded) = encode_package_key(key) {
                body.push_str("power_store_skip=");
                body.push_str(&encoded);
                body.push('|');
                body.push_str(version);
                body.push('\n');
            }
        }
    }
}

pub fn encode_package_key(key: &PackageKey) -> Option<String> {
    for part in [key.manager.as_str(), key.source.as_str(), key.id.as_str()] {
        if part.is_empty() || part.contains('|') || part.contains('\n') {
            return None;
        }
    }
    Some(format!(
        "{}|{}|{}",
        key.manager.as_str(),
        key.source,
        key.id
    ))
}

pub fn parse_package_key(value: &str) -> Option<PackageKey> {
    let mut parts = value.splitn(3, '|');
    let manager = ManagerKind::parse(parts.next()?)?;
    let source = parts.next()?.to_string();
    let id = parts.next()?.to_string();
    if source.is_empty() || id.is_empty() || source.contains('\n') || id.contains('\n') {
        return None;
    }
    Some(PackageKey::new(manager, source, id))
}

pub fn clamp_check_minutes(value: u32) -> u32 {
    value.clamp(MIN_CHECK_MINUTES, MAX_CHECK_MINUTES)
}

pub fn filter_updatable<'a>(
    packages: impl IntoIterator<Item = &'a PackageSummary>,
    filters: &UpdateFilters,
) -> Vec<&'a PackageSummary> {
    packages
        .into_iter()
        .filter(|package| {
            let Some(available) = package.available_version.as_deref() else {
                return false;
            };
            if package.version.as_deref() == Some(available) {
                return false;
            }
            !filters.blocks_update(&package.key, available)
        })
        .collect()
}

/// Top-level `PowerKit` object stored beside a WinGet export document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportExtras {
    pub schema_version: u32,
    pub packages: Vec<ExportPackage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportPackage {
    pub key: PackageKey,
    pub options: InstallOptions,
}

impl ExportExtras {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn to_json_object(&self) -> String {
        let mut body = String::from("{\n  \"SchemaVersion\": ");
        body.push_str(&self.schema_version.to_string());
        body.push_str(",\n  \"Packages\": [");
        for (index, package) in self.packages.iter().enumerate() {
            if index > 0 {
                body.push(',');
            }
            body.push_str("\n    ");
            body.push_str(&package_to_json(package));
        }
        if !self.packages.is_empty() {
            body.push_str("\n  ");
        }
        body.push_str("]\n}");
        body
    }

    pub fn parse_json_object(text: &str) -> Option<Self> {
        let text = text.trim();
        if !text.starts_with('{') || !text.ends_with('}') {
            return None;
        }
        let schema = json_u32_field(text, "SchemaVersion")?;
        if schema != Self::SCHEMA_VERSION {
            return None;
        }
        let packages_text = json_array_field(text, "Packages")?;
        let mut packages = Vec::new();
        for object in json_objects(packages_text) {
            packages.push(parse_export_package(object)?);
        }
        Some(Self {
            schema_version: schema,
            packages,
        })
    }
}

fn package_to_json(package: &ExportPackage) -> String {
    let mut body = format!(
        "{{\"Manager\":{},\"Source\":{},\"Id\":{}",
        json_string(package.key.manager.as_str()),
        json_string(&package.key.source),
        json_string(&package.key.id)
    );
    if let Some(scope) = &package.options.scope {
        body.push_str(",\"Scope\":");
        body.push_str(&json_string(scope));
    }
    if let Some(architecture) = &package.options.architecture {
        body.push_str(",\"Architecture\":");
        body.push_str(&json_string(architecture));
    }
    if let Some(version) = &package.options.version {
        body.push_str(",\"Version\":");
        body.push_str(&json_string(version));
    }
    if let Some(extra) = &package.options.extra_args {
        body.push_str(",\"ExtraArgs\":");
        body.push_str(&json_string(extra));
    }
    body.push('}');
    body
}

fn parse_export_package(object: &str) -> Option<ExportPackage> {
    let manager = ManagerKind::parse(&json_string_field(object, "Manager")?)?;
    let source = json_string_field(object, "Source")?;
    let id = json_string_field(object, "Id")?;
    Some(ExportPackage {
        key: PackageKey::new(manager, source, id),
        options: InstallOptions {
            scope: json_string_field(object, "Scope"),
            architecture: json_string_field(object, "Architecture"),
            version: json_string_field(object, "Version"),
            extra_args: json_string_field(object, "ExtraArgs"),
        },
    })
}

fn json_string(value: &str) -> String {
    let mut out = String::from('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_string_field(object: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\"");
    let start = object.find(&key)? + key.len();
    let rest = object[start..].trim_start();
    if !rest.starts_with(':') {
        return None;
    }
    let rest = rest[1..].trim_start();
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
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        hex.push(chars.next()?);
                    }
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

fn json_u32_field(object: &str, field: &str) -> Option<u32> {
    let key = format!("\"{field}\"");
    let start = object.find(&key)? + key.len();
    let rest = object[start..].trim_start();
    if !rest.starts_with(':') {
        return None;
    }
    let rest = rest[1..].trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn json_array_field<'a>(object: &'a str, field: &str) -> Option<&'a str> {
    let key = format!("\"{field}\"");
    let start = object.find(&key)? + key.len();
    let rest = object[start..].trim_start();
    if !rest.starts_with(':') {
        return None;
    }
    let rest = rest[1..].trim_start();
    if !rest.starts_with('[') {
        return None;
    }
    let mut depth = 0usize;
    for (index, ch) in rest.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[..=index]);
                }
            }
            _ => {}
        }
    }
    None
}

fn json_objects(array: &str) -> Vec<&str> {
    let inner = array.trim();
    let inner = inner
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or("");
    let mut objects = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (index, ch) in inner.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0
                    && let Some(from) = start.take()
                {
                    objects.push(&inner[from..=index]);
                }
            }
            _ => {}
        }
    }
    objects
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: &str) -> PackageKey {
        PackageKey::new(ManagerKind::WinGet, "winget", id)
    }

    #[test]
    fn own_package_is_blocked_from_update_queue() {
        let mut queue = OperationQueue::default();
        queue.push(QueuedOperation {
            kind: OperationKind::Update,
            key: key(OWN_PACKAGE_ID),
            name: "PowerKit".into(),
            options: InstallOptions::default(),
        });
        queue.push(QueuedOperation {
            kind: OperationKind::Install,
            key: key("Git.Git"),
            name: "Git".into(),
            options: InstallOptions::default(),
        });
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.pop_front().unwrap().key.id, "Git.Git");
    }

    #[test]
    fn ignore_and_skip_filter_updates() {
        let mut filters = UpdateFilters::default();
        assert!(filters.ignore(key("Ignored.App")));
        assert!(filters.skip_version(key("Skipped.App"), "2.0.0"));
        let packages = [
            PackageSummary {
                key: key("Ignored.App"),
                name: "Ignored".into(),
                version: Some("1.0.0".into()),
                available_version: Some("1.1.0".into()),
            },
            PackageSummary {
                key: key("Skipped.App"),
                name: "Skipped".into(),
                version: Some("1.0.0".into()),
                available_version: Some("2.0.0".into()),
            },
            PackageSummary {
                key: key("Ready.App"),
                name: "Ready".into(),
                version: Some("1.0.0".into()),
                available_version: Some("1.2.0".into()),
            },
            PackageSummary {
                key: key(OWN_PACKAGE_ID),
                name: "PowerKit".into(),
                version: Some("0.1.0".into()),
                available_version: Some("0.2.0".into()),
            },
        ];
        let updatable = filter_updatable(&packages, &filters);
        assert_eq!(updatable.len(), 1);
        assert_eq!(updatable[0].key.id, "Ready.App");
    }

    #[test]
    fn export_extras_round_trip() {
        let extras = ExportExtras {
            schema_version: ExportExtras::SCHEMA_VERSION,
            packages: vec![ExportPackage {
                key: PackageKey::new(ManagerKind::Scoop, "main", "ripgrep"),
                options: InstallOptions {
                    architecture: Some("arm64".into()),
                    ..InstallOptions::default()
                },
            }],
        };
        let json = extras.to_json_object();
        assert!(json.contains("\"Manager\":\"scoop\""));
        assert_eq!(ExportExtras::parse_json_object(&json), Some(extras));
    }

    #[test]
    fn clamp_check_minutes_bounds() {
        assert_eq!(clamp_check_minutes(1), MIN_CHECK_MINUTES);
        assert_eq!(clamp_check_minutes(60), 60);
        assert_eq!(clamp_check_minutes(u32::MAX), MAX_CHECK_MINUTES);
    }
}
