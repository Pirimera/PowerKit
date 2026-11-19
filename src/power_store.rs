//! PowerStore package model, filters, queue and export helpers (no Windows FFI).
#![allow(dead_code)]

use crate::json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// PowerKit's own package id is never offered for update or uninstall.
pub const OWN_PACKAGE_ID: &str = "PiriMedya.PowerKit";

/// WinGet lists a locally installed MSIX as `MSIX\Name_Version_Arch__PublisherId`, so the
/// guard matches the package name before the first underscore in that form as well.
/// `Pirimera.PowerKit.Shell` is a different package and does not match.
fn is_own_winget_id(id: &str) -> bool {
    let identity = id.strip_prefix("MSIX\\").unwrap_or(id);
    let name = identity.split('_').next().unwrap_or(identity);
    name.eq_ignore_ascii_case(OWN_PACKAGE_ID)
}

/// Sentinel `Status::Unavailable` detail when App Installer is not present.
pub const WINGET_UNAVAILABLE_MISSING: &str = "app-installer-missing";
/// Sentinel when App Installer is present but WinGet Packaged COM is not registered.
pub const WINGET_UNAVAILABLE_TOO_OLD: &str = "app-installer-too-old";

pub const DEFAULT_CHECK_MINUTES: u32 = 360;
pub const MIN_CHECK_MINUTES: u32 = 15;
pub const MAX_CHECK_MINUTES: u32 = 7 * 24 * 60;
pub const MAX_IGNORE_ENTRIES: usize = 512;
pub const MAX_SKIP_ENTRIES: usize = 512;
pub const MAX_HISTORY_OPERATIONS: usize = 200;
pub const MAX_HISTORY_LOG_BYTES: usize = 4096;

pub const MAX_OPTION_ENTRIES: usize = 512;
pub const MAX_EXECUTABLE_CHARS: usize = 1024;
pub const INSTALL_SCOPES: [&str; 2] = ["user", "machine"];
pub const ARCHITECTURES: [&str; 4] = ["x64", "arm64", "x86", "arm"];
/// Triplets vcpkg ships for Windows desktop (`<arch>-windows[-static[-md]]`).
pub const VCPKG_TRIPLETS: [&str; 9] = [
    "x64-windows",
    "x64-windows-static",
    "x64-windows-static-md",
    "arm64-windows",
    "arm64-windows-static",
    "arm64-windows-static-md",
    "x86-windows",
    "x86-windows-static",
    "x86-windows-static-md",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListMode {
    #[default]
    Installed,
    Discover,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusFilter {
    #[default]
    All,
    UpdatesOnly,
    UpToDate,
    Pinned,
}

impl StatusFilter {
    pub const ALL: [Self; 4] = [Self::All, Self::UpdatesOnly, Self::UpToDate, Self::Pinned];

    pub fn index(self) -> usize {
        match self {
            Self::All => 0,
            Self::UpdatesOnly => 1,
            Self::UpToDate => 2,
            Self::Pinned => 3,
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScopeFilter {
    #[default]
    All,
    User,
    Machine,
}

impl ScopeFilter {
    pub const ALL: [Self; 3] = [Self::All, Self::User, Self::Machine];

    pub fn index(self) -> usize {
        match self {
            Self::All => 0,
            Self::User => 1,
            Self::Machine => 2,
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    #[default]
    Name,
    Id,
    InstalledDate,
    UpdatedDate,
}

impl SortKey {
    pub const ALL: [Self; 4] = [Self::Name, Self::Id, Self::InstalledDate, Self::UpdatedDate];

    pub fn index(self) -> usize {
        match self {
            Self::Name => 0,
            Self::Id => 1,
            Self::InstalledDate => 2,
            Self::UpdatedDate => 3,
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub fn default_ascending(self) -> bool {
        matches!(self, Self::Name | Self::Id)
    }
}

/// One entry of the combined sort picker: every key in both directions, ascending first.
pub fn sort_choice_index(key: SortKey, ascending: bool) -> usize {
    key.index() * 2 + usize::from(!ascending)
}

pub fn sort_choice_from_index(index: usize) -> Option<(SortKey, bool)> {
    SortKey::from_index(index / 2).map(|key| (key, index.is_multiple_of(2)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListViewQuery {
    pub filter_text: String,
    /// Lowercase source names such as `"winget"`; empty means all sources.
    pub sources: BTreeSet<String>,
    pub status: StatusFilter,
    pub scope: ScopeFilter,
    pub sort_by: SortKey,
    pub sort_ascending: bool,
}

impl Default for ListViewQuery {
    fn default() -> Self {
        Self {
            filter_text: String::new(),
            sources: BTreeSet::new(),
            status: StatusFilter::All,
            scope: ScopeFilter::All,
            sort_by: SortKey::Name,
            sort_ascending: true,
        }
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

    /// CLI managers can run from a user-chosen program; WinGet uses its COM API instead.
    pub fn supports_executable(self) -> bool {
        !matches!(self, Self::WinGet)
    }

    /// Managers whose operations honor an install scope (see [`InstallOptions::scope`]).
    pub fn supports_default_scope(self) -> bool {
        matches!(self, Self::WinGet)
    }

    /// Managers whose operations honor an installer architecture (see
    /// [`InstallOptions::architecture`]).
    pub fn supports_default_architecture(self) -> bool {
        matches!(self, Self::WinGet)
    }

    /// vcpkg builds every package for a triplet (one of [`VCPKG_TRIPLETS`]).
    pub fn supports_default_triplet(self) -> bool {
        matches!(self, Self::Vcpkg)
    }

    /// Managers whose install and update operations honor [`InstallOptions::version`].
    pub fn supports_version(self) -> bool {
        matches!(
            self,
            Self::WinGet | Self::Scoop | Self::Chocolatey | Self::Pip
        )
    }

    /// Managers that pass [`InstallOptions::extra_args`] to their command line.
    pub fn supports_extra_args(self) -> bool {
        matches!(self, Self::WinGet | Self::Chocolatey)
    }

    /// Whether the package options editor has any field this manager honors.
    pub fn supports_install_options(self) -> bool {
        self.supports_default_scope()
            || self.supports_default_architecture()
            || self.supports_version()
            || self.supports_extra_args()
    }

    /// What to run or open to get the manager when it is missing. PowerStore never
    /// installs a manager itself; the settings drawer only shows this text for copying.
    pub fn install_hint(self) -> Option<&'static str> {
        match self {
            Self::WinGet => Some("https://aka.ms/getwinget"),
            Self::Scoop => Some("irm get.scoop.sh | iex"),
            Self::Chocolatey => Some("https://chocolatey.org/install"),
            Self::Npm => Some("winget install OpenJS.NodeJS.LTS"),
            Self::Pip => Some("winget install Python.Python.3.13"),
            Self::Cargo => Some("winget install Rustlang.Rustup"),
            Self::DotnetTool => Some("winget install Microsoft.DotNet.SDK.10"),
            // Windows PowerShell ships with Windows.
            Self::PowerShell => None,
            Self::PowerShell7 => Some("winget install Microsoft.PowerShell"),
            Self::Vcpkg => Some("https://learn.microsoft.com/vcpkg/get_started/get-started"),
        }
    }

    pub fn phase1(self) -> bool {
        matches!(self, Self::WinGet)
    }

    /// Managers shipped through PowerStore phase 2 (WinGet + Scoop + Chocolatey).
    pub fn phase2(self) -> bool {
        matches!(self, Self::WinGet | Self::Scoop | Self::Chocolatey)
    }

    /// All managers through PowerStore phase 3 (phase 2 plus developer CLIs).
    pub fn phase3(self) -> bool {
        matches!(
            self,
            Self::WinGet
                | Self::Scoop
                | Self::Chocolatey
                | Self::Npm
                | Self::Pip
                | Self::Cargo
                | Self::DotnetTool
                | Self::PowerShell
                | Self::PowerShell7
                | Self::Vcpkg
        )
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
        self.manager == ManagerKind::WinGet && is_own_winget_id(&self.id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageSummary {
    pub key: PackageKey,
    pub name: String,
    pub version: Option<String>,
    pub available_version: Option<String>,
    /// The manager's own version comparison; a differing catalog string can be older.
    pub update_available: bool,
    /// `"user"` or `"machine"` when reported by the manager or uninstall registry.
    pub scope: Option<String>,
    /// ISO local date (`YYYY-MM-DD`) when known.
    pub install_date: Option<String>,
    /// ISO local date or timestamp (`YYYY-MM-DD…`) when known.
    pub updated_at: Option<String>,
    /// Local icon location in `DisplayIcon` form (`path[,index]`) when known.
    pub icon: Option<String>,
}

impl PackageSummary {
    pub fn has_actionable_update(&self, filters: &UpdateFilters) -> bool {
        self.update_available
            && self
                .available_version
                .as_deref()
                .is_some_and(|available| !filters.blocks_update(&self.key, available))
    }

    pub fn is_pinned(&self, filters: &UpdateFilters) -> bool {
        filters.is_ignored(&self.key)
            || self
                .available_version
                .as_deref()
                .is_some_and(|available| filters.skipped_version(&self.key) == Some(available))
    }
}

/// Manifest fields shown for one package; empty strings mean "not in the manifest".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageDetails {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub author: String,
    pub description: String,
    pub homepage: String,
    pub publisher_url: String,
    pub license: String,
    pub license_url: String,
}

impl PackageDetails {
    pub const FIELD_COUNT: usize = 10;

    pub fn fields(&self) -> [&str; Self::FIELD_COUNT] {
        [
            &self.id,
            &self.name,
            &self.version,
            &self.publisher,
            &self.author,
            &self.description,
            &self.homepage,
            &self.publisher_url,
            &self.license,
            &self.license_url,
        ]
    }

    pub fn from_fields(fields: &[String]) -> Option<Self> {
        let [
            id,
            name,
            version,
            publisher,
            author,
            description,
            homepage,
            publisher_url,
            license,
            license_url,
        ] = fields
        else {
            return None;
        };
        Some(Self {
            id: id.clone(),
            name: name.clone(),
            version: version.clone(),
            publisher: publisher.clone(),
            author: author.clone(),
            description: description.clone(),
            homepage: homepage.clone(),
            publisher_url: publisher_url.clone(),
            license: license.clone(),
            license_url: license_url.clone(),
        })
    }
}

/// Per-package install choices, reused for later updates of the same package.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstallOptions {
    /// One of [`INSTALL_SCOPES`].
    pub scope: Option<String>,
    /// One of [`ARCHITECTURES`].
    pub architecture: Option<String>,
    pub version: Option<String>,
    pub extra_args: Option<String>,
}

impl InstallOptions {
    pub fn is_empty(&self) -> bool {
        self.scope.is_none()
            && self.architecture.is_none()
            && self.version.is_none()
            && self.extra_args.is_none()
    }

    /// Trims values, drops empty ones and rejects anything the settings line cannot hold.
    pub fn normalized(self) -> Option<Self> {
        let clean = |value: Option<String>| {
            value
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        let options = Self {
            scope: clean(self.scope),
            architecture: clean(self.architecture),
            version: clean(self.version),
            extra_args: clean(self.extra_args),
        };
        let listed = |value: &Option<String>, allowed: &[&str]| {
            value
                .as_deref()
                .is_none_or(|value| allowed.contains(&value))
        };
        let single_line = |value: &Option<String>| {
            value
                .as_deref()
                .is_none_or(|value| !value.contains(['\n', '\r']))
        };
        let valid = listed(&options.scope, &INSTALL_SCOPES)
            && listed(&options.architecture, &ARCHITECTURES)
            && single_line(&options.version)
            && options.version.as_deref().is_none_or(|v| !v.contains('|'))
            && single_line(&options.extra_args);
        valid.then_some(options)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Install,
    Update,
    Uninstall,
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Update => "update",
            Self::Uninstall => "uninstall",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "install" => Self::Install,
            "update" => Self::Update,
            "uninstall" => Self::Uninstall,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressStage {
    Queued,
    Downloading,
    Installing,
    Uninstalling,
    Finishing,
}

impl ProgressStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Installing => "installing",
            Self::Uninstalling => "uninstalling",
            Self::Finishing => "finishing",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "queued" => Self::Queued,
            "downloading" => Self::Downloading,
            "installing" => Self::Installing,
            "uninstalling" => Self::Uninstalling,
            "finishing" => Self::Finishing,
            _ => return None,
        })
    }
}

/// Progress of the running operation. `percent` is absent when the manager reports none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationProgress {
    pub stage: ProgressStage,
    pub percent: Option<u8>,
}

impl OperationProgress {
    /// `fraction` is the manager's 0.0–1.0 value; zero, negative and NaN mean "unknown".
    pub fn new(stage: ProgressStage, fraction: f64) -> Self {
        Self {
            stage,
            percent: (fraction > 0.0).then(|| (fraction.min(1.0) * 100.0).round() as u8),
        }
    }
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
        // Page and tray "Update all" can both name the same package; it runs once.
        if self
            .items
            .iter()
            .any(|queued| queued.kind == operation.kind && queued.key == operation.key)
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

    pub fn is_ignored(&self, key: &PackageKey) -> bool {
        self.ignored.contains(key)
    }

    pub fn skipped_version(&self, key: &PackageKey) -> Option<&str> {
        self.skipped.get(key).map(String::as_str)
    }

    pub fn ignored(&self) -> impl Iterator<Item = &PackageKey> {
        self.ignored.iter()
    }

    pub fn skipped(&self) -> impl Iterator<Item = (&PackageKey, &String)> {
        self.skipped.iter()
    }
}

/// The few per-manager settings PowerStore exposes. The default is "enabled,
/// auto-detected program, nothing preselected" and is never written to disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagerSettings {
    pub enabled: bool,
    /// Program that replaces PATH discovery; only for [`ManagerKind::supports_executable`].
    pub executable: Option<String>,
    /// One of [`INSTALL_SCOPES`], used when a package has no scope of its own; only for
    /// [`ManagerKind::supports_default_scope`].
    pub scope: Option<String>,
    /// One of [`ARCHITECTURES`], used when a package has no architecture of its own; only
    /// for [`ManagerKind::supports_default_architecture`]. It restricts the installer
    /// choice, so a package without a matching installer fails to install.
    pub architecture: Option<String>,
    /// One of [`VCPKG_TRIPLETS`], appended to a package name that has none; only for
    /// [`ManagerKind::supports_default_triplet`].
    pub triplet: Option<String>,
}

impl Default for ManagerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            executable: None,
            scope: None,
            architecture: None,
            triplet: None,
        }
    }
}

impl ManagerSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Trims values and returns `None` when `kind` cannot use them or the settings line
    /// could not hold them.
    pub fn normalized(self, kind: ManagerKind) -> Option<Self> {
        let clean = |value: Option<String>| {
            value
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        let executable = clean(self.executable);
        let scope = clean(self.scope);
        let architecture = clean(self.architecture);
        let triplet = clean(self.triplet);
        let executable_ok = executable.as_deref().is_none_or(|value| {
            kind.supports_executable()
                && value.chars().count() <= MAX_EXECUTABLE_CHARS
                && !value.contains(['\n', '\r'])
        });
        let scope_ok = scope
            .as_deref()
            .is_none_or(|value| kind.supports_default_scope() && INSTALL_SCOPES.contains(&value));
        let architecture_ok = architecture.as_deref().is_none_or(|value| {
            kind.supports_default_architecture() && ARCHITECTURES.contains(&value)
        });
        let triplet_ok = triplet
            .as_deref()
            .is_none_or(|value| kind.supports_default_triplet() && VCPKG_TRIPLETS.contains(&value));
        (executable_ok && scope_ok && architecture_ok && triplet_ok).then_some(Self {
            enabled: self.enabled,
            executable,
            scope,
            architecture,
            triplet,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerStorePreferences {
    pub check_minutes: u32,
    /// Notify-area balloon when the periodic check finds new updates.
    pub notify: bool,
    /// Fill missing logos from the optional remote icon map (off by default).
    pub remote_icons: bool,
    pub filters: UpdateFilters,
    pub options: BTreeMap<PackageKey, InstallOptions>,
    /// Only managers that differ from [`ManagerSettings::default`].
    pub managers: BTreeMap<ManagerKind, ManagerSettings>,
}

impl Default for PowerStorePreferences {
    fn default() -> Self {
        Self {
            check_minutes: DEFAULT_CHECK_MINUTES,
            notify: true,
            remote_icons: true,
            filters: UpdateFilters::default(),
            options: BTreeMap::new(),
            managers: BTreeMap::new(),
        }
    }
}

impl PowerStorePreferences {
    /// Stores normalized options; empty options remove the entry. Returns false when
    /// the options are invalid or the table is full.
    pub fn set_options(&mut self, key: PackageKey, options: InstallOptions) -> bool {
        let Some(options) = options.normalized() else {
            return false;
        };
        if options.is_empty() {
            self.options.remove(&key);
            return true;
        }
        if self.options.len() >= MAX_OPTION_ENTRIES && !self.options.contains_key(&key) {
            return false;
        }
        self.options.insert(key, options);
        true
    }

    pub fn options_for(&self, key: &PackageKey) -> InstallOptions {
        self.options.get(key).cloned().unwrap_or_default()
    }

    /// Options an operation runs with: the package's own, with the manager's default
    /// scope and architecture filling what is missing. Not for the options editor, which
    /// would otherwise freeze the manager defaults into the package.
    pub fn operation_options_for(&self, key: &PackageKey) -> InstallOptions {
        let mut options = self.options_for(key);
        let manager = self.manager(key.manager);
        if options.scope.is_none() {
            options.scope = manager.scope;
        }
        if options.architecture.is_none() {
            options.architecture = manager.architecture;
        }
        options
    }

    pub fn manager(&self, kind: ManagerKind) -> ManagerSettings {
        self.managers.get(&kind).cloned().unwrap_or_default()
    }

    /// Stores normalized settings; defaults remove the entry. Returns false when the
    /// settings are invalid for `kind`.
    pub fn set_manager(&mut self, kind: ManagerKind, settings: ManagerSettings) -> bool {
        let Some(settings) = settings.normalized(kind) else {
            return false;
        };
        if settings.is_default() {
            self.managers.remove(&kind);
        } else {
            self.managers.insert(kind, settings);
        }
        true
    }

    pub fn append_settings(&self, body: &mut String) {
        body.push_str(&format!(
            "power_store_check_minutes={}\npower_store_notify={}\npower_store_remote_icons={}\n",
            self.check_minutes, self.notify, self.remote_icons
        ));
        for (key, options) in &self.options {
            if let Some(line) = encode_options_line(key, options) {
                body.push_str("power_store_option=");
                body.push_str(&line);
                body.push('\n');
            }
        }
        for (kind, settings) in &self.managers {
            body.push_str("power_store_manager=");
            body.push_str(&encode_manager_line(*kind, settings));
            body.push('\n');
        }
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

/// `kind|1or0|scope|architecture|triplet|executable`; the executable comes last so it
/// may hold `|`. Settings reaching this function have passed [`ManagerSettings::normalized`].
pub fn encode_manager_line(kind: ManagerKind, settings: &ManagerSettings) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}",
        kind.as_str(),
        u8::from(settings.enabled),
        settings.scope.as_deref().unwrap_or(""),
        settings.architecture.as_deref().unwrap_or(""),
        settings.triplet.as_deref().unwrap_or(""),
        settings.executable.as_deref().unwrap_or("")
    )
}

/// Reads the current six-field line and the earlier `kind|1or0|scope|executable` form.
pub fn parse_manager_line(value: &str) -> Option<(ManagerKind, ManagerSettings)> {
    let parts: Vec<&str> = value.splitn(6, '|').collect();
    let (kind, enabled, scope, architecture, triplet, executable) = match parts.as_slice() {
        [kind, enabled, scope, architecture, triplet, executable] => {
            (kind, enabled, scope, *architecture, *triplet, executable)
        }
        [kind, enabled, scope, executable] => (kind, enabled, scope, "", "", executable),
        _ => return None,
    };
    let kind = ManagerKind::parse(kind)?;
    let enabled = match *enabled {
        "1" => true,
        "0" => false,
        _ => return None,
    };
    let present = |value: &str| (!value.is_empty()).then(|| value.to_string());
    ManagerSettings {
        enabled,
        executable: present(executable),
        scope: present(scope),
        architecture: present(architecture),
        triplet: present(triplet),
    }
    .normalized(kind)
    .map(|settings| (kind, settings))
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
    if manager == ManagerKind::Scoop && source != SCOOP_SOURCE {
        // Saved before Scoop packages carried the single `scoop` source: there the source
        // was the bucket, so the id moves into the bucket-qualified form.
        return Some(PackageKey::new(
            manager,
            SCOOP_SOURCE,
            scoop_package_id(&source, &id),
        ));
    }
    Some(PackageKey::new(manager, source, id))
}

/// Every Scoop package is listed under this one source; its bucket lives in the id.
pub const SCOOP_SOURCE: &str = "scoop";

/// Scoop id for `name` in `bucket`: the bare name for `main` (Scoop's default bucket),
/// otherwise `bucket/name`, which is also how Scoop itself names it on the command line.
pub fn scoop_package_id(bucket: &str, name: &str) -> String {
    if bucket.is_empty() || bucket == "main" {
        name.to_string()
    } else {
        format!("{bucket}/{name}")
    }
}

/// Splits a Scoop id into `(bucket, name)`; a bare name belongs to `main`.
pub fn split_scoop_id(id: &str) -> (&str, &str) {
    id.split_once('/').unwrap_or(("main", id))
}

/// `manager|source|id|scope|architecture|version|extra`; extra args are last so they may contain `|`.
fn encode_options_line(key: &PackageKey, options: &InstallOptions) -> Option<String> {
    let options = options.clone().normalized()?;
    let field = |value: &Option<String>| value.clone().unwrap_or_default();
    Some(format!(
        "{}|{}|{}|{}|{}",
        encode_package_key(key)?,
        field(&options.scope),
        field(&options.architecture),
        field(&options.version),
        field(&options.extra_args)
    ))
}

pub fn parse_options_line(value: &str) -> Option<(PackageKey, InstallOptions)> {
    let parts: Vec<&str> = value.splitn(7, '|').collect();
    let [manager, source, id, scope, architecture, version, extra] = parts.as_slice() else {
        return None;
    };
    let key = parse_package_key(&format!("{manager}|{source}|{id}"))?;
    let optional = |value: &str| (!value.is_empty()).then(|| value.to_string());
    let options = InstallOptions {
        scope: optional(scope),
        architecture: optional(architecture),
        version: optional(version),
        extra_args: optional(extra),
    }
    .normalized()?;
    (!options.is_empty()).then_some((key, options))
}

pub fn clamp_check_minutes(value: u32) -> u32 {
    value.clamp(MIN_CHECK_MINUTES, MAX_CHECK_MINUTES)
}

/// Settings-page stepper values, from `MIN_CHECK_MINUTES` to `MAX_CHECK_MINUTES`.
const CHECK_MINUTE_STEPS: [u32; 11] = [15, 30, 60, 120, 180, 360, 720, 1440, 2880, 4320, 10_080];

/// Next longer (`up`) or shorter step; a hand-edited value moves to its neighbour.
pub fn step_check_minutes(current: u32, up: bool) -> u32 {
    let next = if up {
        CHECK_MINUTE_STEPS.iter().find(|&&step| step > current)
    } else {
        CHECK_MINUTE_STEPS
            .iter()
            .rev()
            .find(|&&step| step < current)
    };
    clamp_check_minutes(next.copied().unwrap_or(current))
}

pub fn filter_updatable<'a>(
    packages: impl IntoIterator<Item = &'a PackageSummary>,
    filters: &UpdateFilters,
) -> Vec<&'a PackageSummary> {
    packages
        .into_iter()
        .filter(|package| package.has_actionable_update(filters))
        .collect()
}

/// Distinct source names in display order (known managers first, then extras).
pub fn available_sources(packages: &[PackageSummary]) -> Vec<String> {
    const KNOWN: &[&str] = &[
        "winget",
        "msstore",
        "chocolatey",
        "scoop",
        "npm",
        "pip",
        "cargo",
        "dotnet",
        "powershell",
        "powershell7",
        "vcpkg",
    ];
    let mut seen = BTreeSet::new();
    for package in packages {
        let source = package.key.source.trim().to_ascii_lowercase();
        if !source.is_empty() {
            seen.insert(source);
        }
    }
    let mut sources = Vec::new();
    for known in KNOWN {
        if *known == "winget" || *known == "msstore" || seen.contains(*known) {
            sources.push((*known).to_string());
            seen.remove(*known);
        }
    }
    sources.extend(seen);
    sources
}

/// Filters and sorts a package list. In `ListMode::Installed` with `StatusFilter::All`,
/// actionable updates always stay pinned above the rest of the installed packages.
pub fn query_packages<'a>(
    packages: &'a [PackageSummary],
    mode: ListMode,
    query: &ListViewQuery,
    prefs: &PowerStorePreferences,
) -> Vec<&'a PackageSummary> {
    let needle = query.filter_text.trim().to_lowercase();
    let mut visible: Vec<&'a PackageSummary> = packages
        .iter()
        .filter(|package| {
            if !needle.is_empty() {
                let matches_text = package.name.to_lowercase().contains(&needle)
                    || package.key.id.to_lowercase().contains(&needle)
                    || package.key.source.to_lowercase().contains(&needle);
                if !matches_text {
                    return false;
                }
            }
            if !query.sources.is_empty()
                && !query
                    .sources
                    .contains(&package.key.source.to_ascii_lowercase())
            {
                return false;
            }
            if mode == ListMode::Installed {
                let matches_status = match query.status {
                    StatusFilter::All => true,
                    StatusFilter::UpdatesOnly => package.has_actionable_update(&prefs.filters),
                    StatusFilter::UpToDate => !package.has_actionable_update(&prefs.filters),
                    StatusFilter::Pinned => package.is_pinned(&prefs.filters),
                };
                if !matches_status {
                    return false;
                }
                let effective_scope = package.scope.as_deref().or_else(|| {
                    prefs
                        .options
                        .get(&package.key)
                        .and_then(|options| options.scope.as_deref())
                });
                let matches_scope = match query.scope {
                    ScopeFilter::All => true,
                    ScopeFilter::User => {
                        effective_scope.is_some_and(|s| s.eq_ignore_ascii_case("user"))
                    }
                    ScopeFilter::Machine => {
                        effective_scope.is_some_and(|s| s.eq_ignore_ascii_case("machine"))
                    }
                };
                if !matches_scope {
                    return false;
                }
            }
            true
        })
        .collect();

    let pin_updates = mode == ListMode::Installed && query.status == StatusFilter::All;
    visible.sort_by(|left, right| {
        if pin_updates {
            let left_group = u8::from(!left.has_actionable_update(&prefs.filters));
            let right_group = u8::from(!right.has_actionable_update(&prefs.filters));
            let group_cmp = left_group.cmp(&right_group);
            if group_cmp != std::cmp::Ordering::Equal {
                return group_cmp;
            }
        }
        compare_packages(left, right, query.sort_by, query.sort_ascending)
    });
    visible
}

fn compare_packages(
    left: &PackageSummary,
    right: &PackageSummary,
    sort_by: SortKey,
    ascending: bool,
) -> std::cmp::Ordering {
    let tie_break = || {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.key.id.to_lowercase().cmp(&right.key.id.to_lowercase()))
    };
    match sort_by {
        SortKey::Name => {
            let ordering = tie_break();
            if ascending {
                ordering
            } else {
                ordering.reverse()
            }
        }
        SortKey::Id => {
            let ordering = left
                .key
                .id
                .to_lowercase()
                .cmp(&right.key.id.to_lowercase())
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
            if ascending {
                ordering
            } else {
                ordering.reverse()
            }
        }
        SortKey::InstalledDate | SortKey::UpdatedDate => {
            fn package_date(package: &PackageSummary, sort_by: SortKey) -> Option<&str> {
                let raw = match sort_by {
                    SortKey::InstalledDate => package.install_date.as_deref(),
                    SortKey::UpdatedDate => package.updated_at.as_deref(),
                    _ => None,
                };
                raw.filter(|value| !value.is_empty())
            }
            match (package_date(left, sort_by), package_date(right, sort_by)) {
                (Some(left_date), Some(right_date)) => {
                    let ordering = left_date.cmp(right_date);
                    let directed = if ascending {
                        ordering
                    } else {
                        ordering.reverse()
                    };
                    directed.then_with(tie_break)
                }
                // Packages with known dates always appear before unknown dates.
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => tie_break(),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryOutcome {
    Succeeded,
    Cancelled,
    Failed,
}

impl HistoryOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "ok",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "ok" => Self::Succeeded,
            "cancelled" => Self::Cancelled,
            "failed" => Self::Failed,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    /// Local wall-clock text supplied by the platform layer.
    pub time: String,
    pub kind: OperationKind,
    pub key: PackageKey,
    pub name: String,
    pub outcome: HistoryOutcome,
    pub log: String,
}

/// Recent operations, oldest first, bounded by count and per-entry log size.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    entries: VecDeque<HistoryEntry>,
}

impl History {
    pub fn push(&mut self, mut entry: HistoryEntry) {
        truncate_utf8(&mut entry.log, MAX_HISTORY_LOG_BYTES);
        self.entries.push_back(entry);
        while self.entries.len() > MAX_HISTORY_OPERATIONS {
            self.entries.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn newest_first(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter().rev()
    }

    pub fn serialize(&self) -> String {
        let mut text = String::from("powerkit-history=1\n");
        for entry in &self.entries {
            let Some(key) = encode_package_key(&entry.key) else {
                continue;
            };
            let fields = [
                entry.time.as_str(),
                entry.kind.as_str(),
                key.as_str(),
                entry.name.as_str(),
                entry.outcome.as_str(),
                entry.log.as_str(),
            ];
            let line: Vec<String> = fields.iter().map(|field| escape_field(field)).collect();
            text.push_str(&line.join("\t"));
            text.push('\n');
        }
        text
    }

    /// Malformed lines are skipped so one bad entry never hides the rest.
    pub fn parse(text: &str) -> Self {
        let mut history = Self::default();
        let mut lines = text.lines();
        if lines.next() != Some("powerkit-history=1") {
            return history;
        }
        for line in lines {
            let fields: Vec<String> = line.split('\t').map(unescape_field).collect();
            let [time, kind, key, name, outcome, log] = fields.as_slice() else {
                continue;
            };
            let (Some(kind), Some(key), Some(outcome)) = (
                OperationKind::parse(kind),
                parse_package_key(key),
                HistoryOutcome::parse(outcome),
            ) else {
                continue;
            };
            history.push(HistoryEntry {
                time: time.clone(),
                kind,
                key,
                name: name.clone(),
                outcome,
                log: log.clone(),
            });
        }
        history
    }
}

fn truncate_utf8(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}

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

/// Escapes `\`, tab, CR and LF so a value fits one tab-separated line.
pub fn escape_field(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => text.push_str("\\\\"),
            '\t' => text.push_str("\\t"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            other => text.push(other),
        }
    }
    text
}

pub fn unescape_field(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        match characters.next() {
            Some('t') => text.push('\t'),
            Some('n') => text.push('\n'),
            Some('r') => text.push('\r'),
            Some(other) => text.push(other),
            None => text.push('\\'),
        }
    }
    text
}

/// WinGet source details that `winget import` expects for the two phase-1 catalogs.
fn winget_source_details(source: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match source {
        "winget" => Some((
            "https://cdn.winget.microsoft.com/cache",
            "Microsoft.Winget.Source_8wekyb3d8bbwe",
            "Microsoft.PreIndexed.Package",
        )),
        "msstore" => Some((
            "https://storeedgefd.dsx.mp.microsoft.com/v9.0",
            "StoreEdgeFD",
            "Microsoft.Rest",
        )),
        _ => None,
    }
}

/// A `winget export`-shaped document; packages with options or from other
/// managers are repeated under a top-level `PowerKit` object.
pub fn export_document(packages: &[ExportPackage], creation_date: &str) -> String {
    let mut sources = String::new();
    for source in ["winget", "msstore"] {
        let Some((argument, identifier, kind)) = winget_source_details(source) else {
            continue;
        };
        let ids: Vec<String> = packages
            .iter()
            .filter(|package| {
                package.key.manager == ManagerKind::WinGet && package.key.source == source
            })
            .map(|package| {
                format!(
                    "        {{ \"PackageIdentifier\": {} }}",
                    json::quote(&package.key.id)
                )
            })
            .collect();
        if ids.is_empty() {
            continue;
        }
        if !sources.is_empty() {
            sources.push_str(",\n");
        }
        sources.push_str(&format!(
            "    {{\n      \"Packages\": [\n{}\n      ],\n      \"SourceDetails\": {{\n        \"Argument\": {},\n        \"Identifier\": {},\n        \"Name\": {},\n        \"Type\": {}\n      }}\n    }}",
            ids.join(",\n"),
            json::quote(argument),
            json::quote(identifier),
            json::quote(source),
            json::quote(kind)
        ));
    }
    let extras = ExportExtras {
        schema_version: ExportExtras::SCHEMA_VERSION,
        packages: packages
            .iter()
            .filter(|package| {
                !package.options.is_empty()
                    || package.key.manager != ManagerKind::WinGet
                    || winget_source_details(&package.key.source).is_none()
            })
            .cloned()
            .collect(),
    };
    format!(
        "{{\n  \"$schema\": \"https://aka.ms/winget-packages.schema.2.0.json\",\n  \"CreationDate\": {},\n  \"Sources\": [\n{sources}\n  ],\n  \"PowerKit\": {}\n}}\n",
        json::quote(creation_date),
        extras.to_json_object().replace('\n', "\n  ")
    )
}

/// Packages from a WinGet export, merged with options from a `PowerKit` object.
/// Returns `None` when the document has no `Sources` array.
pub fn parse_import(text: &str) -> Option<Vec<ExportPackage>> {
    let root = json::parse(text)?;
    let sources = root.get("Sources")?;
    sources.as_array()?;
    let mut packages: Vec<ExportPackage> = Vec::new();
    for source in sources.objects() {
        let Some(name) = source
            .get("SourceDetails")
            .and_then(|details| details.str_field("Name"))
        else {
            continue;
        };
        let Some(list) = source
            .get("Packages")
            .filter(|list| list.as_array().is_some())
        else {
            continue;
        };
        for package in list.objects() {
            let Some(id) = package.str_field("PackageIdentifier") else {
                continue;
            };
            let key = PackageKey::new(ManagerKind::WinGet, name.clone(), id);
            if packages.iter().any(|known| known.key == key) {
                continue;
            }
            packages.push(ExportPackage {
                key,
                options: InstallOptions {
                    version: package.str_field("Version"),
                    ..InstallOptions::default()
                },
            });
        }
    }
    let extras = root.get("PowerKit").and_then(ExportExtras::from_value);
    for extra in extras.map(|extras| extras.packages).unwrap_or_default() {
        match packages.iter_mut().find(|known| known.key == extra.key) {
            Some(known) => known.options = extra.options,
            None => packages.push(extra),
        }
    }
    Some(packages)
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
        Self::from_value(&json::parse(text)?)
    }

    fn from_value(value: &json::Value) -> Option<Self> {
        value.entries()?;
        let schema = value.get("SchemaVersion")?.as_u32()?;
        if schema != Self::SCHEMA_VERSION {
            return None;
        }
        let list = value.get("Packages")?;
        list.as_array()?;
        let mut packages = Vec::new();
        for object in list.objects() {
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
        json::quote(package.key.manager.as_str()),
        json::quote(&package.key.source),
        json::quote(&package.key.id)
    );
    if let Some(scope) = &package.options.scope {
        body.push_str(",\"Scope\":");
        body.push_str(&json::quote(scope));
    }
    if let Some(architecture) = &package.options.architecture {
        body.push_str(",\"Architecture\":");
        body.push_str(&json::quote(architecture));
    }
    if let Some(version) = &package.options.version {
        body.push_str(",\"Version\":");
        body.push_str(&json::quote(version));
    }
    if let Some(extra) = &package.options.extra_args {
        body.push_str(",\"ExtraArgs\":");
        body.push_str(&json::quote(extra));
    }
    body.push('}');
    body
}

fn parse_export_package(object: &json::Value) -> Option<ExportPackage> {
    let manager = ManagerKind::parse(&object.str_field("Manager")?)?;
    let source = object.str_field("Source")?;
    let id = object.str_field("Id")?;
    Some(ExportPackage {
        key: PackageKey::new(manager, source, id),
        options: InstallOptions {
            scope: object.str_field("Scope"),
            architecture: object.str_field("Architecture"),
            version: object.str_field("Version"),
            extra_args: object.str_field("ExtraArgs"),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: &str) -> PackageKey {
        PackageKey::new(ManagerKind::WinGet, "winget", id)
    }

    #[test]
    fn check_interval_steps_stay_in_range() {
        assert_eq!(step_check_minutes(DEFAULT_CHECK_MINUTES, true), 720);
        assert_eq!(step_check_minutes(DEFAULT_CHECK_MINUTES, false), 180);
        assert_eq!(
            step_check_minutes(MIN_CHECK_MINUTES, false),
            MIN_CHECK_MINUTES
        );
        assert_eq!(
            step_check_minutes(MAX_CHECK_MINUTES, true),
            MAX_CHECK_MINUTES
        );
        assert_eq!(step_check_minutes(100, true), 120);
        assert_eq!(step_check_minutes(100, false), 60);
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
    fn own_package_guard_matches_plain_and_msix_listings_only() {
        let listed = |id: &str| PackageKey::new(ManagerKind::WinGet, "local", id);
        assert!(listed("PiriMedya.PowerKit").is_own_package());
        assert!(listed("MSIX\\PiriMedya.PowerKit_0.18.0.0_arm64__6fnnvtwr0r606").is_own_package());
        assert!(
            !listed("MSIX\\Pirimera.PowerKit.Shell_0.1.0.0_arm64__6fnnvtwr0r606").is_own_package()
        );
        assert!(!PackageKey::new(ManagerKind::Npm, "npm", "PiriMedya.PowerKit").is_own_package());
    }

    #[test]
    fn install_option_fields_follow_what_each_adapter_honors() {
        assert!(ManagerKind::WinGet.supports_version());
        assert!(ManagerKind::WinGet.supports_extra_args());
        assert!(ManagerKind::Scoop.supports_version());
        assert!(!ManagerKind::Scoop.supports_extra_args());
        assert!(ManagerKind::Chocolatey.supports_extra_args());
        assert!(!ManagerKind::Npm.supports_install_options());
        assert!(!ManagerKind::Vcpkg.supports_install_options());
        assert!(ManagerKind::Vcpkg.supports_default_triplet());
    }

    #[test]
    fn the_same_package_and_kind_is_queued_once() {
        let mut queue = OperationQueue::default();
        let update = |id: &str| QueuedOperation {
            kind: OperationKind::Update,
            key: key(id),
            name: id.into(),
            options: InstallOptions::default(),
        };
        queue.push(update("Git.Git"));
        queue.push(update("Git.Git"));
        queue.push(QueuedOperation {
            kind: OperationKind::Uninstall,
            key: key("Git.Git"),
            name: "Git".into(),
            options: InstallOptions::default(),
        });
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn remove_at_drops_a_waiting_queue_item() {
        let mut queue = OperationQueue::default();
        queue.push(QueuedOperation {
            kind: OperationKind::Install,
            key: key("A.A"),
            name: "A".into(),
            options: InstallOptions::default(),
        });
        queue.push(QueuedOperation {
            kind: OperationKind::Update,
            key: key("B.B"),
            name: "B".into(),
            options: InstallOptions::default(),
        });
        queue.push(QueuedOperation {
            kind: OperationKind::Uninstall,
            key: key("C.C"),
            name: "C".into(),
            options: InstallOptions::default(),
        });
        assert_eq!(queue.remove_at(1).unwrap().key.id, "B.B");
        assert_eq!(queue.len(), 2);
        assert!(queue.remove_at(9).is_none());
        let ids: Vec<_> = queue.iter().map(|item| item.key.id.as_str()).collect();
        assert_eq!(ids, ["A.A", "C.C"]);
    }

    #[test]
    fn ignore_and_skip_filter_updates() {
        let mut filters = UpdateFilters::default();
        assert!(filters.ignore(key("Ignored.App")));
        assert!(filters.skip_version(key("Skipped.App"), "2.0.0"));
        let summary =
            |id: &str, installed: &str, available: &str, update_available| PackageSummary {
                key: key(id),
                name: id.into(),
                version: Some(installed.into()),
                available_version: Some(available.into()),
                update_available,
                scope: None,
                install_date: None,
                updated_at: None,
                icon: None,
            };
        let packages = [
            summary("Ignored.App", "1.0.0", "1.1.0", true),
            summary("Skipped.App", "1.0.0", "2.0.0", true),
            summary("Ready.App", "1.0.0", "1.2.0", true),
            summary(OWN_PACKAGE_ID, "0.1.0", "0.2.0", true),
            // Self-updating apps can run ahead of the catalog (Edge, OneDrive).
            summary("Ahead.App", "157.0.4341.60", "156.0.4314.58", false),
        ];
        let updatable = filter_updatable(&packages, &filters);
        assert_eq!(updatable.len(), 1);
        assert_eq!(updatable[0].key.id, "Ready.App");
    }

    #[test]
    fn unified_query_pins_updates_on_top_and_sorts_dates_with_missing_last() {
        let mut prefs = PowerStorePreferences::default();
        prefs.filters.ignore(key("Ignored.App"));
        let make = |id: &str,
                    name: &str,
                    source: &str,
                    update: bool,
                    scope: Option<&str>,
                    installed_on: Option<&str>,
                    updated_on: Option<&str>| PackageSummary {
            key: PackageKey::new(ManagerKind::WinGet, source, id),
            name: name.into(),
            version: Some("1.0".into()),
            available_version: Some(if update { "2.0" } else { "1.0" }.into()),
            update_available: update,
            scope: scope.map(Into::into),
            install_date: installed_on.map(Into::into),
            updated_at: updated_on.map(Into::into),
            icon: None,
        };
        let packages = vec![
            make(
                "Zeta.Tool",
                "Zeta",
                "winget",
                false,
                Some("user"),
                Some("2026-01-10"),
                Some("2026-03-01"),
            ),
            make(
                "Beta.Update",
                "Beta",
                "msstore",
                true,
                Some("machine"),
                Some("2026-02-01"),
                Some("2026-11-10"),
            ),
            make("Alpha.Tool", "Alpha", "winget", false, None, None, None),
            make(
                "Alpha.Update",
                "Argo",
                "winget",
                true,
                Some("user"),
                Some("2026-05-01"),
                None,
            ),
            make(
                "Ignored.App",
                "Ignored",
                "winget",
                true,
                Some("user"),
                Some("2026-09-01"),
                None,
            ),
        ];

        // Default unified view: actionable updates first (sorted by name), then the rest by name.
        let mut query = ListViewQuery::default();
        let ids: Vec<&str> = query_packages(&packages, ListMode::Installed, &query, &prefs)
            .into_iter()
            .map(|package| package.key.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                "Alpha.Update",
                "Beta.Update",
                "Alpha.Tool",
                "Ignored.App",
                "Zeta.Tool"
            ]
        );

        // Sort by install date (newest first): updates stay on top, dated rows precede undated rows.
        query.sort_by = SortKey::InstalledDate;
        query.sort_ascending = false;
        let by_date: Vec<&str> = query_packages(&packages, ListMode::Installed, &query, &prefs)
            .into_iter()
            .map(|package| package.key.id.as_str())
            .collect();
        assert_eq!(
            by_date,
            vec![
                "Alpha.Update",
                "Beta.Update",
                "Ignored.App",
                "Zeta.Tool",
                "Alpha.Tool"
            ]
        );

        // Pinned status filter shows ignored / skipped entries; source + scope + text filters narrow.
        query.status = StatusFilter::Pinned;
        let pinned = query_packages(&packages, ListMode::Installed, &query, &prefs);
        assert_eq!(pinned.len(), 1);
        assert_eq!(pinned[0].key.id, "Ignored.App");

        query.status = StatusFilter::All;
        query.sources = BTreeSet::from(["winget".to_string()]);
        query.scope = ScopeFilter::User;
        query.filter_text = "zeta".into();
        let narrowed = query_packages(&packages, ListMode::Installed, &query, &prefs);
        assert_eq!(narrowed.len(), 1);
        assert_eq!(narrowed[0].key.id, "Zeta.Tool");

        // Several sources combine as "any of".
        query.scope = ScopeFilter::All;
        query.filter_text.clear();
        query.sources = BTreeSet::from(["winget".to_string(), "msstore".to_string()]);
        let any_of = query_packages(&packages, ListMode::Installed, &query, &prefs);
        assert!(!any_of.is_empty());
        assert!(
            any_of
                .iter()
                .all(|package| matches!(package.key.source.as_str(), "winget" | "msstore"))
        );
    }

    #[test]
    fn sort_choices_cover_both_directions_for_every_key() {
        for key in SortKey::ALL {
            for ascending in [true, false] {
                let index = sort_choice_index(key, ascending);
                assert_eq!(sort_choice_from_index(index), Some((key, ascending)));
            }
        }
        assert_eq!(sort_choice_index(SortKey::Name, true), 0);
        assert_eq!(sort_choice_index(SortKey::UpdatedDate, false), 7);
        assert_eq!(sort_choice_from_index(8), None);
    }

    #[test]
    fn export_extras_round_trip() {
        let extras = ExportExtras {
            schema_version: ExportExtras::SCHEMA_VERSION,
            packages: vec![ExportPackage {
                key: PackageKey::new(ManagerKind::Scoop, SCOOP_SOURCE, "ripgrep"),
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
    fn scoop_export_json_parses_apps() {
        let text = r#"{
  "apps": [
    { "Name": "7zip", "Source": "main", "Version": "24.08", "Info": "" },
    { "Name": "ripgrep", "Bucket": "main", "Version": "14.1.0" },
    { "Name": "microsoft-jdk", "Source": "java", "Version": "25.0.5" }
  ]
}"#;
        let packages = parse_scoop_export(text).expect("parse");
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].key.manager, ManagerKind::Scoop);
        assert_eq!(packages[0].key.source, SCOOP_SOURCE);
        assert_eq!(packages[0].key.id, "7zip");
        assert_eq!(packages[0].name, "7zip");
        assert_eq!(packages[0].version.as_deref(), Some("24.08"));
        assert_eq!(packages[1].key.id, "ripgrep");
        assert_eq!(packages[2].key.source, SCOOP_SOURCE);
        assert_eq!(packages[2].key.id, "java/microsoft-jdk");
        assert_eq!(packages[2].name, "microsoft-jdk");
    }

    #[test]
    fn scoop_ids_keep_the_bucket_only_outside_main() {
        assert_eq!(scoop_package_id("main", "git"), "git");
        assert_eq!(scoop_package_id("", "git"), "git");
        assert_eq!(
            scoop_package_id("java", "microsoft-jdk"),
            "java/microsoft-jdk"
        );
        assert_eq!(split_scoop_id("git"), ("main", "git"));
        assert_eq!(
            split_scoop_id("java/microsoft-jdk"),
            ("java", "microsoft-jdk")
        );
    }

    #[test]
    fn scoop_keys_saved_with_the_bucket_as_source_still_load() {
        let main = parse_package_key("scoop|main|7zip").expect("main");
        assert_eq!(
            main,
            PackageKey::new(ManagerKind::Scoop, SCOOP_SOURCE, "7zip")
        );
        let java = parse_package_key("scoop|java|microsoft-jdk").expect("java");
        assert_eq!(
            java,
            PackageKey::new(ManagerKind::Scoop, SCOOP_SOURCE, "java/microsoft-jdk")
        );
        let winget = parse_package_key("winget|winget|Git.Git").expect("winget");
        assert_eq!(winget.source, "winget");
    }

    #[test]
    fn choco_limit_and_outdated_rows_parse() {
        let listed = "git|2.45.1\n7zip|24.8\nChocolatey v2.2.2\n";
        assert_eq!(
            parse_choco_limit_rows(listed),
            vec![
                ("git".into(), "2.45.1".into()),
                ("7zip".into(), "24.8".into())
            ]
        );
        let outdated = "git|2.45.1|2.46.0|false\n";
        assert_eq!(
            parse_choco_outdated_rows(outdated),
            vec![("git".into(), "2.45.1".into(), "2.46.0".into())]
        );
        assert!(version_looks_newer("2.46.0", "2.45.1"));
        assert!(!version_looks_newer("1.0.0", "1.0.0"));
    }

    #[test]
    fn phase3_cli_parsers_accept_fixtures() {
        let npm_list = r#"{"dependencies":{"typescript":{"version":"5.4.0"}}}"#;
        assert_eq!(
            parse_npm_list_json(npm_list),
            vec![("typescript".into(), "5.4.0".into())]
        );
        let npm_out = r#"{"typescript":{"current":"5.4.0","wanted":"5.4.5","latest":"5.5.0"}}"#;
        assert_eq!(
            parse_npm_outdated_json(npm_out),
            vec![("typescript".into(), "5.5.0".into())]
        );
        let pip = r#"[{"name":"pip","version":"24.0"},{"name":"wheel","version":"0.43.0"}]"#;
        assert_eq!(parse_pip_list_json(pip).len(), 2);
        let pip_out = r#"[{"name":"pip","version":"24.0","latest_version":"24.2","latest_filetype":"wheel"}]"#;
        assert_eq!(
            parse_pip_outdated_json(pip_out),
            vec![("pip".into(), "24.0".into(), "24.2".into())]
        );
        let cargo = "ripgrep v14.1.0:\n    rg\nfd-find v10.1.0:\n    fd\n";
        assert_eq!(
            parse_cargo_install_list(cargo),
            vec![
                ("ripgrep".into(), "14.1.0".into()),
                ("fd-find".into(), "10.1.0".into())
            ]
        );
        let cargo_search = "serde = \"1.0.210\"    # serialization\n... and 3 more crates\n";
        assert_eq!(
            parse_cargo_search(cargo_search)[0],
            ("serde".into(), "1.0.210".into(), "serialization".into())
        );
        let dotnet = "Package Id      Version      Commands\n--------------------------------------\ndotnetsay       2.1.0        dotnetsay\n";
        assert_eq!(
            parse_dotnet_tool_list(dotnet),
            vec![("dotnetsay".into(), "2.1.0".into())]
        );
        let ps = r#"[{"Name":"PSReadLine","Version":"2.3.5"}]"#;
        assert_eq!(
            parse_powershell_modules_json(ps),
            vec![("PSReadLine".into(), "2.3.5".into())]
        );
        let vcpkg = "fmt:x64-windows              9.0.0              Formatting library\n";
        assert_eq!(
            parse_vcpkg_list(vcpkg),
            vec![(
                "fmt:x64-windows".into(),
                "9.0.0".into(),
                "Formatting library".into()
            )]
        );
        assert!(ManagerKind::Npm.phase3());
        assert!(!ManagerKind::Npm.phase2());
    }

    #[test]
    fn manager_defaults_are_not_stored_and_invalid_settings_are_rejected() {
        let mut prefs = PowerStorePreferences::default();
        assert!(prefs.manager(ManagerKind::Npm).enabled);
        assert!(prefs.set_manager(
            ManagerKind::Npm,
            ManagerSettings {
                enabled: false,
                ..ManagerSettings::default()
            }
        ));
        assert!(!prefs.manager(ManagerKind::Npm).enabled);
        assert!(prefs.set_manager(ManagerKind::Npm, ManagerSettings::default()));
        assert!(prefs.managers.is_empty());

        let scope = |value: &str| ManagerSettings {
            scope: Some(value.into()),
            ..ManagerSettings::default()
        };
        let executable = |value: &str| ManagerSettings {
            executable: Some(value.into()),
            ..ManagerSettings::default()
        };
        assert!(!prefs.set_manager(ManagerKind::WinGet, scope("root")));
        assert!(!prefs.set_manager(ManagerKind::Npm, scope("user")));
        assert!(!prefs.set_manager(ManagerKind::WinGet, executable("winget.exe")));
        assert!(!prefs.set_manager(ManagerKind::Npm, executable("a\nb")));
        assert!(!prefs.set_manager(ManagerKind::Npm, executable(&"x".repeat(2000))));
        assert!(prefs.set_manager(ManagerKind::Npm, executable("  C:\\node\\npm.cmd ")));
        assert_eq!(
            prefs.manager(ManagerKind::Npm).executable.as_deref(),
            Some("C:\\node\\npm.cmd")
        );
    }

    #[test]
    fn manager_default_scope_fills_operations_but_not_the_options_editor() {
        let mut prefs = PowerStorePreferences::default();
        let git = PackageKey::new(ManagerKind::WinGet, "winget", "Git.Git");
        let code = PackageKey::new(ManagerKind::WinGet, "winget", "Microsoft.VisualStudioCode");
        assert!(prefs.set_manager(
            ManagerKind::WinGet,
            ManagerSettings {
                scope: Some("machine".into()),
                ..ManagerSettings::default()
            }
        ));
        assert!(prefs.set_options(
            code.clone(),
            InstallOptions {
                scope: Some("user".into()),
                ..InstallOptions::default()
            }
        ));
        assert_eq!(
            prefs.operation_options_for(&git).scope.as_deref(),
            Some("machine")
        );
        assert_eq!(
            prefs.operation_options_for(&code).scope.as_deref(),
            Some("user")
        );
        assert_eq!(prefs.options_for(&git).scope, None);
    }

    #[test]
    fn manager_default_architecture_and_triplet_follow_their_manager() {
        let mut prefs = PowerStorePreferences::default();
        let git = PackageKey::new(ManagerKind::WinGet, "winget", "Git.Git");
        let code = PackageKey::new(ManagerKind::WinGet, "winget", "Microsoft.VisualStudioCode");
        let architecture = |value: &str| ManagerSettings {
            architecture: Some(value.into()),
            ..ManagerSettings::default()
        };
        let triplet = |value: &str| ManagerSettings {
            triplet: Some(value.into()),
            ..ManagerSettings::default()
        };
        assert!(!prefs.set_manager(ManagerKind::WinGet, architecture("sparc")));
        assert!(!prefs.set_manager(ManagerKind::Npm, architecture("arm64")));
        assert!(!prefs.set_manager(ManagerKind::WinGet, triplet("arm64-windows")));
        assert!(!prefs.set_manager(ManagerKind::Vcpkg, triplet("linux")));
        assert!(!prefs.set_manager(ManagerKind::Vcpkg, architecture("arm64")));
        assert!(prefs.set_manager(ManagerKind::Vcpkg, triplet("arm64-windows")));
        assert!(prefs.set_manager(ManagerKind::WinGet, architecture("arm64")));
        assert!(prefs.set_options(
            code.clone(),
            InstallOptions {
                architecture: Some("x64".into()),
                ..InstallOptions::default()
            }
        ));
        assert_eq!(
            prefs.operation_options_for(&git).architecture.as_deref(),
            Some("arm64")
        );
        assert_eq!(
            prefs.operation_options_for(&code).architecture.as_deref(),
            Some("x64")
        );
        assert_eq!(prefs.options_for(&git).architecture, None);

        let mut body = String::new();
        prefs.append_settings(&mut body);
        assert!(body.contains("power_store_manager=vcpkg|1|||arm64-windows|\n"));
        assert!(body.contains("power_store_manager=winget|1||arm64||\n"));
        let (kind, parsed) = parse_manager_line("vcpkg|1|||arm64-windows|").unwrap();
        assert_eq!(kind, ManagerKind::Vcpkg);
        assert_eq!(parsed.triplet.as_deref(), Some("arm64-windows"));
        // Lines written before architecture and triplet existed still load.
        let (kind, legacy) = parse_manager_line("winget|1|machine|").unwrap();
        assert_eq!(kind, ManagerKind::WinGet);
        assert_eq!(legacy.scope.as_deref(), Some("machine"));
        assert_eq!(legacy.architecture, None);
        assert!(parse_manager_line("winget|1|machine|arm64").is_none());
    }

    #[test]
    fn manager_install_hints_exist_except_for_builtin_windows_powershell() {
        for kind in [
            ManagerKind::WinGet,
            ManagerKind::Scoop,
            ManagerKind::Chocolatey,
            ManagerKind::Npm,
            ManagerKind::Pip,
            ManagerKind::Cargo,
            ManagerKind::DotnetTool,
            ManagerKind::PowerShell7,
            ManagerKind::Vcpkg,
        ] {
            assert!(
                kind.install_hint().is_some_and(|hint| !hint.contains('\n')),
                "{kind:?}"
            );
        }
        assert_eq!(ManagerKind::PowerShell.install_hint(), None);
    }

    #[test]
    fn clamp_check_minutes_bounds() {
        assert_eq!(clamp_check_minutes(1), MIN_CHECK_MINUTES);
        assert_eq!(clamp_check_minutes(60), 60);
        assert_eq!(clamp_check_minutes(u32::MAX), MAX_CHECK_MINUTES);
    }

    #[test]
    fn options_line_round_trips_and_rejects_invalid_values() {
        let mut prefs = PowerStorePreferences::default();
        let options = InstallOptions {
            scope: Some(" user ".into()),
            architecture: Some("arm64".into()),
            version: Some("1.2.3".into()),
            extra_args: Some("--flag=a|b {x}".into()),
        };
        assert!(prefs.set_options(key("Contoso.App"), options));
        let mut body = String::new();
        prefs.append_settings(&mut body);
        let line = body
            .lines()
            .find_map(|line| line.strip_prefix("power_store_option="))
            .expect("options line");
        let (parsed_key, parsed) = parse_options_line(line).expect("parse");
        assert_eq!(parsed_key, key("Contoso.App"));
        assert_eq!(parsed.scope.as_deref(), Some("user"));
        assert_eq!(parsed.extra_args.as_deref(), Some("--flag=a|b {x}"));
        assert!(!prefs.set_options(
            key("Bad.Scope"),
            InstallOptions {
                scope: Some("everyone".into()),
                ..InstallOptions::default()
            }
        ));
        assert!(!prefs.set_options(
            key("Bad.Args"),
            InstallOptions {
                extra_args: Some("a\nb".into()),
                ..InstallOptions::default()
            }
        ));
        assert!(prefs.set_options(key("Contoso.App"), InstallOptions::default()));
        assert!(prefs.options.is_empty());
        assert_eq!(parse_options_line("winget|winget|Only.Key"), None);
    }

    #[test]
    fn history_is_bounded_truncates_logs_and_skips_bad_lines() {
        let entry = |index: usize, log: String| HistoryEntry {
            time: format!("2026-11-16 10:{index:02}"),
            kind: OperationKind::Install,
            key: key(&format!("App.{index}")),
            name: format!("App\t{index}"),
            outcome: HistoryOutcome::Failed,
            log,
        };
        let mut history = History::default();
        for index in 0..MAX_HISTORY_OPERATIONS + 5 {
            history.push(entry(index, "ğ".repeat(MAX_HISTORY_LOG_BYTES)));
        }
        assert_eq!(history.len(), MAX_HISTORY_OPERATIONS);
        let newest = history.newest_first().next().expect("newest");
        assert_eq!(newest.key.id, format!("App.{}", MAX_HISTORY_OPERATIONS + 4));
        assert!(newest.log.len() <= MAX_HISTORY_LOG_BYTES);
        let mut text = history.serialize();
        text.push_str("garbage line\nonly\ttwo\n");
        assert_eq!(History::parse(&text), history);
        assert_eq!(History::parse("unversioned\n").len(), 0);
    }

    /// Writes an export for `winget import --no-upgrade` to accept or reject; re-run
    /// after WinGet updates in case its schema validation tightens.
    #[test]
    #[ignore = "requires POWERKIT_EXPORT_FIXTURE and POWERKIT_EXPORT_IDS"]
    fn writes_winget_import_fixture() {
        let path = std::env::var("POWERKIT_EXPORT_FIXTURE").unwrap();
        let ids = std::env::var("POWERKIT_EXPORT_IDS").unwrap();
        let mut packages: Vec<ExportPackage> = ids
            .split(',')
            .map(|id| ExportPackage {
                key: key(id),
                options: InstallOptions::default(),
            })
            .collect();
        if let Some(first) = packages.first_mut() {
            first.options.scope = Some("user".into());
        }
        let document = export_document(&packages, "2026-11-16T12:00:00.000-00:00");
        assert_eq!(parse_import(&document).unwrap(), packages);
        std::fs::write(path, document).unwrap();
    }

    #[test]
    fn export_document_imports_back_with_options() {
        let packages = vec![
            ExportPackage {
                key: key("Git.Git"),
                options: InstallOptions::default(),
            },
            ExportPackage {
                key: key("PowerKit"),
                options: InstallOptions {
                    extra_args: Some("--x \"{y}\" ]".into()),
                    ..InstallOptions::default()
                },
            },
            ExportPackage {
                key: PackageKey::new(ManagerKind::WinGet, "msstore", "9NBLGGH4NNS1"),
                options: InstallOptions::default(),
            },
        ];
        let document = export_document(&packages, "2026-11-16T10:00:00.000-00:00");
        assert!(document.contains("\"Name\": \"msstore\""));
        assert_eq!(parse_import(&document), Some(packages));
        assert_eq!(parse_import("{\"NoSources\": []}"), None);
    }

    #[test]
    fn progress_percent_is_clamped_and_unknown_without_a_value() {
        let percent =
            |fraction| OperationProgress::new(ProgressStage::Downloading, fraction).percent;
        assert_eq!(percent(0.0), None);
        assert_eq!(percent(f64::NAN), None);
        assert_eq!(percent(-0.5), None);
        assert_eq!(percent(0.424), Some(42));
        assert_eq!(percent(7.0), Some(100));
        for stage in [
            ProgressStage::Queued,
            ProgressStage::Downloading,
            ProgressStage::Installing,
            ProgressStage::Uninstalling,
            ProgressStage::Finishing,
        ] {
            assert_eq!(ProgressStage::parse(stage.as_str()), Some(stage));
        }
    }
}
