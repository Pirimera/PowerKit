//! Network and download work stays off the Win32/power thread.
//! No GitHub credentials are accepted or stored in the shipped application.
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant},
};
use velopack::{UpdateCheck, UpdateManager, UpdateOptions, VelopackAsset, sources::GithubSource};

const REPOSITORY: &str = "https://github.com/Pirimera/PowerKit";
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const MANUAL_INTERVAL: Duration = Duration::from_secs(60);
#[cfg(target_arch = "aarch64")]
const CHANNEL: &str = "win-arm64";
#[cfg(target_arch = "x86_64")]
const CHANNEL: &str = "win-x64";

pub(crate) fn channel() -> &'static str {
    CHANNEL
}

struct Pending {
    manager: UpdateManager,
    asset: VelopackAsset,
}
enum Outcome {
    Unpackaged,
    Current,
    Ready(Box<Pending>),
    Unavailable,
}
pub struct Updates {
    receiver: Option<Receiver<Outcome>>,
    pending: Option<Box<Pending>>,
    last_check: Option<Instant>,
    enabled: bool,
    status: &'static str,
}
impl Default for Updates {
    fn default() -> Self {
        Self {
            receiver: None,
            pending: None,
            last_check: None,
            enabled: true,
            status: "Güncellemeler henüz denetlenmedi.",
        }
    }
}
impl Updates {
    pub fn status(&self) -> &'static str {
        self.status
    }
    pub fn ready(&self) -> bool {
        self.pending.is_some()
    }

    pub fn check(&mut self, manual: bool) {
        let delay = if manual {
            MANUAL_INTERVAL
        } else {
            CHECK_INTERVAL
        };
        if !self.enabled
            || self.receiver.is_some()
            || self.pending.is_some()
            || self.last_check.is_some_and(|t| t.elapsed() < delay)
        {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.last_check = Some(Instant::now());
        self.status = "Güncelleme denetleniyor / indiriliyor…";
        match std::thread::Builder::new()
            .name("powerkit-update".into())
            .spawn(move || {
                let _ = tx.send(check_and_download());
            }) {
            Ok(_) => self.receiver = Some(rx),
            Err(_) => self.status = "Güncelleme denetimi başlatılamadı.",
        }
    }

    pub fn poll(&mut self) {
        let result = match &self.receiver {
            Some(rx) => rx.try_recv(),
            None => return,
        };
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Outcome::Unavailable,
        };
        self.receiver = None;
        match outcome {
            Outcome::Unpackaged => {
                self.enabled = false;
                self.status = "Otomatik güncelleme için kurulum paketini kullanın.";
            }
            Outcome::Current => self.status = "PowerKit güncel.",
            Outcome::Unavailable => {
                self.status = "Güncelleme kaynağına erişilemiyor (ağ / private repo).";
            }
            Outcome::Ready(pending) => {
                self.pending = Some(pending);
                self.status = "Güncelleme hazır; PowerKit’ten çıkınca uygulanacak.";
            }
        }
    }

    // Only call after UI-thread power cleanup. Never terminate an AlwaysOn session here.
    pub fn apply_on_exit(&mut self, restart: bool) -> Result<(), String> {
        if let Some(pending) = self.pending.take() {
            pending
                .manager
                .wait_exit_then_apply_updates(&pending.asset, true, restart, Vec::<String>::new())
                .map_err(|e| format!("Güncelleme uygulanamadı; mevcut sürüm korunuyor: {e}"))?;
        }
        Ok(())
    }
}
fn check_and_download() -> Outcome {
    let source = GithubSource::new(REPOSITORY, None, false);
    let options = UpdateOptions {
        ExplicitChannel: Some(CHANNEL.into()),
        AllowVersionDowngrade: false,
        ..Default::default()
    };
    let Ok(manager) = UpdateManager::new(source, Some(options), None) else {
        return Outcome::Unpackaged;
    };
    download_from(manager)
}
fn download_from(manager: UpdateManager) -> Outcome {
    // A completed download survives an offline restart. Never apply a partial file.
    if let Some(asset) = manager.get_update_pending_restart() {
        return Outcome::Ready(Box::new(Pending { manager, asset }));
    }
    match manager.check_for_updates() {
        Ok(UpdateCheck::UpdateAvailable(info)) => {
            if manager.download_updates(&info, None).is_err() {
                return Outcome::Unavailable;
            }
            Outcome::Ready(Box::new(Pending {
                manager,
                asset: info.TargetFullRelease,
            }))
        }
        Ok(UpdateCheck::NoUpdateAvailable) => Outcome::Current,
        _ => Outcome::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_worker_does_not_mark_current_or_disable_future_checks() {
        let (tx, rx) = mpsc::channel();
        drop(tx);
        let mut updates = Updates {
            receiver: Some(rx),
            ..Default::default()
        };
        updates.poll();
        assert!(updates.receiver.is_none());
        assert!(updates.enabled);
        assert!(updates.status().contains("erişilemiyor"));
        assert!(!updates.ready());
        assert!(updates.apply_on_exit(false).is_ok());
    }
    #[test]
    fn unpackaged_build_never_contacts_release_server_again() {
        let (tx, rx) = mpsc::channel();
        tx.send(Outcome::Unpackaged).ok().unwrap();
        let mut updates = Updates {
            receiver: Some(rx),
            ..Default::default()
        };
        updates.poll();
        updates.check(true);
        assert!(!updates.enabled);
        assert!(updates.receiver.is_none());
        assert!(updates.last_check.is_none());
    }
    #[test]
    fn repeated_manual_clicks_are_rate_limited() {
        let mut updates = Updates {
            last_check: Some(Instant::now()),
            ..Default::default()
        };
        updates.check(true);
        assert!(updates.receiver.is_none());
        updates.check(false);
        assert!(updates.receiver.is_none());
    }

    // Runs against the actual vpk output after packaging, without executing an installer/updater.
    #[test]
    #[ignore = "requires POWERKIT_TEST_FEED from scripts/Package.ps1 output"]
    fn packaged_feed_download_cache_and_corruption() {
        use std::{fs, path::PathBuf, time::SystemTime};
        use velopack::{locator::VelopackLocatorConfig, sources::FileSource};
        let feed = PathBuf::from(std::env::var_os("POWERKIT_TEST_FEED").expect("feed path"));
        let root = std::env::temp_dir().join(format!(
            "powerkit-feed-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Scratch(root.clone());
        fs::write(root.join("Update.exe"), b"test placeholder; never executed").unwrap();
        fs::write(root.join("sq.version"), format!(
            "<package><metadata><id>Pirimera.PowerKit</id><version>0.0.1</version><mainExe>powerkit.exe</mainExe><channel>{CHANNEL}</channel></metadata></package>"
        )).unwrap();
        let manager = |source: &std::path::Path, cache: &str| {
            let packages = root.join(cache);
            fs::create_dir_all(&packages).unwrap();
            UpdateManager::new(
                FileSource::new(source),
                Some(UpdateOptions {
                    ExplicitChannel: Some(CHANNEL.into()),
                    AllowVersionDowngrade: false,
                    ..Default::default()
                }),
                Some(VelopackLocatorConfig {
                    RootAppDir: root.clone(),
                    UpdateExePath: root.join("Update.exe"),
                    PackagesDir: packages,
                    ManifestPath: root.join("sq.version"),
                    CurrentBinaryDir: root.clone(),
                    IsPortable: true,
                }),
            )
            .unwrap()
        };
        let downloaded = download_from(manager(&feed, "cache"));
        let Outcome::Ready(pending) = downloaded else {
            panic!("valid package did not become ready");
        };
        assert_eq!(pending.asset.Version, env!("CARGO_PKG_VERSION"));
        assert_eq!(pending.asset.PackageId, "Pirimera.PowerKit");
        assert!(pending.asset.FileName.contains(CHANNEL));
        let (tx, rx) = mpsc::channel();
        tx.send(Outcome::Ready(pending)).ok().unwrap();
        let mut updates = Updates {
            receiver: Some(rx),
            ..Default::default()
        };
        updates.poll();
        assert!(updates.ready());
        updates.check(true);
        assert!(updates.receiver.is_none()); // no redundant download once ready
        // A cached complete update remains available with an unreachable source.
        assert!(matches!(
            download_from(manager(&root.join("offline"), "cache")),
            Outcome::Ready(_)
        ));
        // Same feed and file name, truncated payload: never expose it as ready.
        let corrupt = root.join("corrupt");
        fs::create_dir(&corrupt).unwrap();
        fs::copy(
            feed.join(format!("releases.{CHANNEL}.json")),
            corrupt.join(format!("releases.{CHANNEL}.json")),
        )
        .unwrap();
        for entry in fs::read_dir(&feed).unwrap() {
            let entry = entry.unwrap();
            if entry.path().extension().is_some_and(|e| e == "nupkg") {
                fs::write(corrupt.join(entry.file_name()), b"truncated package").unwrap();
            }
        }
        assert!(matches!(
            download_from(manager(&corrupt, "bad-cache")),
            Outcome::Unavailable
        ));
        assert!(
            manager(&corrupt, "bad-cache")
                .get_update_pending_restart()
                .is_none()
        );
    }
}
