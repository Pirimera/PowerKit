//! The Windows package catalog as the `powerkit_app` PowerStore port: WinGet
//! through the worker child, the CLI managers, the logo cache and the files
//! PowerStore writes. Every method runs on a session worker thread.
use super::super::clock;
use super::broker::CancelHandle;
use super::catalog::managers;
use super::winget::ManagerError;
use super::{broker, file_write, icon_db, icons};
use powerkit_app::controller::power_store::managers::Cancel;
use powerkit_app::controller::power_store::{
    CatalogError, InstalledList, ManagerProbe, ManagerStatus, Prepared, Session,
};
use powerkit_app::ports::PackageCatalog;
use powerkit_core::power_store::{
    ManagerKind, PackageDetails, PackageKey, PackageSummary, QueuedOperation,
};
use std::{io, path::Path, sync::Arc};

impl From<ManagerError> for CatalogError {
    fn from(error: ManagerError) -> Self {
        match error {
            ManagerError::Unavailable(message) => Self::Unavailable(message),
            ManagerError::Operation(message) => Self::Failed(message),
            ManagerError::Cancelled => Self::Cancelled,
        }
    }
}

struct Catalog;
impl PackageCatalog for Catalog {
    fn manager_statuses(&self) -> Vec<ManagerStatus> {
        managers().statuses()
    }
    fn list_installed(&self) -> Result<InstalledList, CatalogError> {
        managers().list_installed()
    }
    fn search(
        &self,
        query: &str,
        source: Option<&str>,
    ) -> Result<Vec<PackageSummary>, CatalogError> {
        managers().search(query, source)
    }
    fn details(&self, key: &PackageKey) -> Result<PackageDetails, CatalogError> {
        managers().details(key)
    }
    fn probe_manager(&self, kind: ManagerKind) -> ManagerProbe {
        managers().probe(kind)
    }
    fn prepare(&self, operation: QueuedOperation) -> Prepared {
        // One handle reaches the WinGet child's stdin, the other a CLI
        // program's process, once the operation has started one; cancelling
        // earlier is remembered by both.
        let winget = CancelHandle::default();
        let cli = Cancel::default();
        let (winget_hook, cli_hook) = (winget.clone(), cli.clone());
        Prepared {
            cancel: Arc::new(move || {
                winget_hook.cancel();
                cli_hook.cancel();
            }),
            run: Box::new(move |progress| {
                match managers().run_operation(&operation, &cli, &mut *progress) {
                    Some(result) => result,
                    // WinGet runs its own operations.
                    None => Ok(broker::run_operation(&operation, &winget, |step| {
                        progress(step)
                    })?),
                }
            }),
        }
    }
    fn cached_icon(&self, location: &str) -> Option<Arc<[u8]>> {
        icons::cached(location)
    }
    fn resolve_icons(&self, packages: &[PackageSummary]) -> Vec<(PackageKey, String)> {
        icon_db::resolve(packages)
    }
    fn fetch_icons(&self, locations: &[String], on_ready: &(dyn Fn() + Sync)) {
        icons::prefetch_remote(locations, on_ready);
    }
    fn write_text(&self, path: &Path, text: &str) -> io::Result<()> {
        file_write::write_replacing(path, text)
    }
    fn now_local_stamp(&self) -> String {
        clock::local_stamp(clock::now_utc())
    }
    fn now_iso_utc(&self) -> String {
        clock::iso_utc(clock::now_utc())
    }
}

/// An idle PowerStore session over the Windows catalog.
pub fn session() -> Session {
    Session::new(Arc::new(Catalog))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_errors_keep_their_kind_across_the_port() {
        assert_eq!(
            CatalogError::from(ManagerError::Unavailable("no managers".into())),
            CatalogError::Unavailable("no managers".into())
        );
        assert_eq!(
            CatalogError::from(ManagerError::Operation("exit 1".into())),
            CatalogError::Failed("exit 1".into())
        );
        assert_eq!(
            CatalogError::from(ManagerError::Cancelled),
            CatalogError::Cancelled
        );
    }

    #[test]
    fn a_cancel_before_the_operation_starts_is_remembered() {
        let cancel = CancelHandle::default();
        let hook = cancel.clone();
        let request: Arc<dyn Fn() + Send + Sync> = Arc::new(move || hook.cancel());
        assert!(!cancel.is_cancelled());
        request();
        assert!(cancel.is_cancelled());
    }
}
