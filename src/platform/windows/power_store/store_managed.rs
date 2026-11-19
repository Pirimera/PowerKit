//! Finds installed MSIX packages that Microsoft Store delivers (`PackageSignatureKind::Store`).
//!
//! WinGet's catalog does not list most of these (OEM codecs, Windows App Runtime, system
//! helpers), so their updates never reach PowerStore's update count. The rows are still
//! shown, but PowerStore sends the user to Microsoft Store updates instead of queuing them.
use crate::power_store::{PackageKey, PackageSummary};
use std::collections::BTreeSet;
use windows::ApplicationModel::PackageSignatureKind;
use windows::Management::Deployment::PackageManager;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

/// WinGet reports MSIX packages with the package full name after this prefix.
const MSIX_PREFIX: &str = "msix\\";

/// Keys of installed rows whose MSIX full name is Store-signed. Runs on a worker thread.
pub fn store_managed_keys(packages: &[PackageSummary]) -> BTreeSet<PackageKey> {
    if !packages
        .iter()
        .any(|package| msix_full_name(&package.key.id).is_some())
    {
        return BTreeSet::new();
    }
    mark_store_keys(packages, &store_full_names())
}

/// Pure matching step, separated from the WinRT enumeration so it can be unit tested.
pub fn mark_store_keys(
    packages: &[PackageSummary],
    store_full_names: &BTreeSet<String>,
) -> BTreeSet<PackageKey> {
    packages
        .iter()
        .filter(|package| {
            msix_full_name(&package.key.id).is_some_and(|name| store_full_names.contains(&name))
        })
        .map(|package| package.key.clone())
        .collect()
}

fn msix_full_name(id: &str) -> Option<String> {
    id.to_ascii_lowercase()
        .strip_prefix(MSIX_PREFIX)
        .map(str::to_string)
}

/// Lowercase full names of every installed package signed by Microsoft Store.
fn store_full_names() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    // SAFETY: balanced below only when this call succeeded on the same thread.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    if let Ok(manager) = PackageManager::new()
        && let Ok(packages) = manager.FindPackages()
        && let Ok(iterator) = packages.First()
    {
        loop {
            if !iterator.HasCurrent().unwrap_or(false) {
                break;
            }
            if let Ok(package) = iterator.Current()
                && package
                    .SignatureKind()
                    .is_ok_and(|kind| kind == PackageSignatureKind::Store)
                && let Ok(id) = package.Id()
                && let Ok(full_name) = id.FullName()
            {
                names.insert(full_name.to_string().to_ascii_lowercase());
            }
            if !iterator.MoveNext().unwrap_or(false) {
                break;
            }
        }
    }
    if com {
        // SAFETY: paired with the successful CoInitializeEx above on this thread.
        unsafe { CoUninitialize() };
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power_store::ManagerKind;

    fn msix(full_name: &str) -> PackageSummary {
        PackageSummary {
            key: PackageKey::new(ManagerKind::WinGet, "local", format!("MSIX\\{full_name}")),
            name: full_name.into(),
            version: Some("1.0".into()),
            available_version: None,
            update_available: false,
            scope: None,
            install_date: None,
            updated_at: None,
            icon: None,
        }
    }

    #[test]
    fn only_store_signed_msix_full_names_are_marked() {
        let packages = vec![
            msix("Microsoft.XboxIdentityProvider_12.130.16001.0_arm64__8wekyb3d8bbwe"),
            msix("Contoso.SideLoaded_1.0.0.0_arm64__abcdefghijklm"),
        ];
        let store = BTreeSet::from([
            "microsoft.xboxidentityprovider_12.130.16001.0_arm64__8wekyb3d8bbwe".to_string(),
        ]);
        let marked = mark_store_keys(&packages, &store);
        assert_eq!(marked.len(), 1);
        assert!(marked.contains(&packages[0].key));
    }

    #[test]
    fn non_msix_rows_are_never_marked() {
        let arp = PackageSummary {
            key: PackageKey::new(ManagerKind::WinGet, "local", "ARP\\Machine\\Akis"),
            ..msix("Akis")
        };
        let store = BTreeSet::from(["akis".to_string()]);
        assert!(mark_store_keys(&[arp], &store).is_empty());
    }
}
