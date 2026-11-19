//! Per-manager settings as seen by adapters that run on worker threads.
//!
//! Preferences live on the UI thread; list, search and operation workers only need
//! "is this manager on" and "which program does it run". The UI thread publishes a
//! copy through [`apply`] at startup and after every manager edit.
use crate::power_store::{ManagerKind, ManagerSettings, PowerStorePreferences};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;

static CONFIG: RwLock<BTreeMap<ManagerKind, ManagerSettings>> = RwLock::new(BTreeMap::new());

/// Replaces the published settings with those in `prefs`.
pub fn apply(prefs: &PowerStorePreferences) {
    let mut config = CONFIG
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *config = prefs.managers.clone();
}

fn settings(kind: ManagerKind) -> ManagerSettings {
    CONFIG
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&kind)
        .cloned()
        .unwrap_or_default()
}

pub fn is_enabled(kind: ManagerKind) -> bool {
    settings(kind).enabled
}

/// The program a CLI manager runs: nothing when the manager is off, the user's chosen
/// file when it exists (a missing custom file is reported as "not found", never replaced
/// silently by another install), otherwise whatever `discover` finds.
pub fn resolve_program(
    kind: ManagerKind,
    discover: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    resolve(&settings(kind), discover)
}

/// Where `kind`'s program is, whatever the on/off switch says. Settings show this next
/// to the switch, so it only looks for the file and never runs it.
pub fn locate_program(
    kind: ManagerKind,
    discover: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    let settings = ManagerSettings {
        enabled: true,
        ..settings(kind)
    };
    resolve(&settings, discover)
}

/// vcpkg triplet to build for when a package name carries none.
pub fn default_triplet(kind: ManagerKind) -> Option<String> {
    settings(kind).triplet
}

fn resolve(
    settings: &ManagerSettings,
    discover: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    if !settings.enabled {
        return None;
    }
    match settings.executable.as_deref() {
        Some(path) => {
            let path = PathBuf::from(path);
            path.is_file().then_some(path)
        }
        None => discover(),
    }
}

/// The user's custom program for `kind`, if one is set (callers gate on the switch).
pub fn custom_executable(kind: ManagerKind) -> Option<PathBuf> {
    settings(kind).executable.map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discovered() -> Option<PathBuf> {
        Some(PathBuf::from("on-path.exe"))
    }

    #[test]
    fn disabled_manager_never_discovers_a_program() {
        let settings = ManagerSettings {
            enabled: false,
            ..ManagerSettings::default()
        };
        assert_eq!(resolve(&settings, discovered), None);
    }

    #[test]
    fn enabled_manager_without_override_uses_discovery() {
        assert_eq!(
            resolve(&ManagerSettings::default(), discovered),
            discovered()
        );
    }

    #[test]
    fn missing_custom_program_is_not_replaced_by_discovery() {
        let settings = ManagerSettings {
            executable: Some("Z:\\definitely\\missing\\tool.exe".into()),
            ..ManagerSettings::default()
        };
        assert_eq!(resolve(&settings, discovered), None);
    }

    #[test]
    fn existing_custom_program_wins_over_discovery() {
        let exe = std::env::current_exe().unwrap();
        let settings = ManagerSettings {
            executable: Some(exe.to_string_lossy().into_owned()),
            ..ManagerSettings::default()
        };
        assert_eq!(resolve(&settings, discovered), Some(exe));
    }
}
