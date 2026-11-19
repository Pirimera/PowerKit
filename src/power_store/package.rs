//! Package identity, summaries, details and install options.

use super::*;

/// WinGet lists a locally installed MSIX as `MSIX\Name_Version_Arch__PublisherId`, so the
/// guard matches the package name before the first underscore in that form as well.
/// `Pirimera.PowerKit.Shell` is a different package and does not match.
fn is_own_winget_id(id: &str) -> bool {
    let identity = id.strip_prefix("MSIX\\").unwrap_or(id);
    let name = identity.split('_').next().unwrap_or(identity);
    name.eq_ignore_ascii_case(OWN_PACKAGE_ID)
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
