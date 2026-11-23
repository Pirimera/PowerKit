//! The machine's package managers: `powerkit_app`'s command-line adapters
//! over the Windows process runner, with WinGet (through the broker child) as
//! the native package service.
use super::cli_process::{Runner, find_on_path};
use super::{broker, icons, store_managed};
use powerkit_app::controller::power_store::managers::{Managers, NativePackages};
use powerkit_app::controller::power_store::{CatalogError, ManagerPresence};
use powerkit_core::power_store::{ManagerKind, PackageDetails, PackageKey, PackageSummary};
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// The managers of this process. Workers read the settings the shell last
/// published with `Managers::configure`.
pub fn managers() -> &'static Managers {
    static MANAGERS: OnceLock<Managers> = OnceLock::new();
    MANAGERS.get_or_init(|| Managers::new(Box::new(Runner), Box::new(WinGet)))
}

struct WinGet;
impl NativePackages for WinGet {
    fn kind(&self) -> ManagerKind {
        ManagerKind::WinGet
    }
    fn sources(&self) -> &'static [&'static str] {
        &["winget", "msstore"]
    }
    /// App Installer's `winget` alias; a listing refines this with the real
    /// COM result.
    fn seems_installed(&self) -> bool {
        find_on_path("winget").is_some()
    }
    fn list_installed(&self) -> Result<Vec<PackageSummary>, CatalogError> {
        Ok(broker::list_installed()?)
    }
    fn search(
        &self,
        query: &str,
        source: Option<&str>,
    ) -> Result<Vec<PackageSummary>, CatalogError> {
        Ok(broker::search_discover(query, source)?)
    }
    fn details(&self, key: &PackageKey) -> Result<PackageDetails, CatalogError> {
        Ok(broker::details(key)?)
    }
    fn presence_when_unavailable(&self, message: &str) -> ManagerPresence {
        too_old(message)
    }
    fn after_listing(&self, packages: &[PackageSummary]) -> BTreeSet<PackageKey> {
        // Local DisplayIcon files only; network logos load afterwards (see `Session`).
        icons::prefetch_local(packages);
        store_managed::store_managed_keys(packages)
    }
}

/// A missing App Installer and one too old to offer the COM API fail the same
/// way; the message tells them apart.
fn too_old(message: &str) -> ManagerPresence {
    if message.contains("app-installer-too-old")
        || message.contains("too old")
        || message.contains("CLASS_NOT_REGISTERED")
    {
        ManagerPresence::TooOld
    } else {
        ManagerPresence::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read-only probe of the real command-line managers on this machine:
    /// prints which are present and how many packages each lists. WinGet is
    /// left out because its worker is not beside the test executable.
    #[test]
    #[ignore = "starts the real package managers of this machine"]
    fn probe_lists_packages_from_the_installed_managers() {
        use powerkit_core::power_store::{ManagerSettings, PowerStorePreferences};
        let managers = Managers::new(Box::new(Runner), Box::new(WinGet));
        let mut prefs = PowerStorePreferences::default();
        let off = ManagerSettings {
            enabled: false,
            ..prefs.manager(ManagerKind::WinGet)
        };
        assert!(prefs.set_manager(ManagerKind::WinGet, off));
        managers.configure(&prefs);
        let list = managers.list_installed().expect("a manager answers");
        for status in &list.managers {
            let count = list
                .packages
                .iter()
                .filter(|package| package.key.manager == status.kind)
                .count();
            let probe = managers.probe(status.kind);
            println!(
                "{:<12} {:?} packages={count} version={:?}",
                status.kind.as_str(),
                status.presence,
                probe.version
            );
        }
        assert!(!list.packages.is_empty());
    }

    #[test]
    fn an_old_app_installer_is_told_apart_from_a_missing_one() {
        assert_eq!(
            too_old("app-installer-too-old: 1.19"),
            ManagerPresence::TooOld
        );
        assert_eq!(
            too_old("0x80040154 CLASS_NOT_REGISTERED"),
            ManagerPresence::TooOld
        );
        assert_eq!(too_old("WinGet is not installed"), ManagerPresence::Missing);
    }
}
