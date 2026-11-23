//! Persisted PowerStore settings and their line formats.

use super::*;

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

/// Reads the line [`encode_manager_line`] writes.
pub fn parse_manager_line(value: &str) -> Option<(ManagerKind, ManagerSettings)> {
    let parts: Vec<&str> = value.splitn(6, '|').collect();
    let [kind, enabled, scope, architecture, triplet, executable] = parts.as_slice() else {
        return None;
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
    Some(PackageKey::new(manager, source, id))
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
