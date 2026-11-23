//! Loading the Installed and Discover lists on worker threads and applying their answers.
//! Each list mode has its own worker channel and generation (see `Session`).

use super::*;
use std::path::Path;

pub(super) enum ListOutcome {
    /// The saved list of an earlier run, sent before the managers answer. Not final: the
    /// worker still sends `Done` or `Failed` for the same generation.
    Cached {
        generation: u64,
        cache: InstalledCache,
    },
    Done {
        mode: ListMode,
        generation: u64,
        packages: Vec<PackageSummary>,
        managers: Option<Vec<ManagerStatus>>,
        store_managed: BTreeSet<PackageKey>,
    },
    Failed {
        mode: ListMode,
        generation: u64,
        error: String,
        unavailable: bool,
        managers: Option<Vec<ManagerStatus>>,
    },
}

impl Session {
    /// Reloads installed packages. When `prefer_silent` is true and a previous list
    /// exists, keeps those rows on screen (`refreshing`) instead of `Status::Loading`.
    pub fn refresh_installed(&mut self, prefer_silent: bool) {
        if self.installed_receiver.is_some()
            && (matches!(self.installed_status, Status::Loading) || self.refreshing)
        {
            return;
        }
        let silent = prefer_silent && !self.installed_packages.is_empty();
        self.installed_generation += 1;
        let generation = self.installed_generation;
        let (sender, receiver) = mpsc::channel();
        self.installed_receiver = Some(receiver);
        let cache_path = if silent {
            self.refreshing = true;
            None
        } else {
            self.refreshing = false;
            self.installed_status = Status::Loading;
            self.installed_cache_path.clone()
        };
        let catalog = self.catalog.clone();
        std::thread::spawn(move || {
            if let Some(cache) = cache_path.as_deref().and_then(read_installed_cache) {
                let _ = sender.send(ListOutcome::Cached { generation, cache });
            }
            let outcome = match catalog.list_installed() {
                Ok(list) => ListOutcome::Done {
                    mode: ListMode::Installed,
                    generation,
                    packages: list.packages,
                    managers: Some(list.managers),
                    store_managed: list.store_managed,
                },
                Err(error) => ListOutcome::Failed {
                    mode: ListMode::Installed,
                    generation,
                    unavailable: matches!(error, CatalogError::Unavailable(_)),
                    error: error.to_string(),
                    managers: Some(catalog.manager_statuses()),
                },
            };
            let _ = sender.send(outcome);
        });
    }

    pub(super) fn refresh_discover(&mut self) {
        let trimmed = self.discover_query.trim().to_string();
        if trimmed.is_empty() {
            self.discover_packages.clear();
            self.discover_status = Status::Ready;
            return;
        }
        self.discover_generation += 1;
        let generation = self.discover_generation;
        let source = self.discover_source.clone();
        let (sender, receiver) = mpsc::channel();
        self.discover_receiver = Some(receiver);
        self.discover_status = Status::Loading;
        let catalog = self.catalog.clone();
        std::thread::spawn(move || {
            let outcome = match catalog.search(&trimmed, source.as_deref()) {
                Ok(packages) => ListOutcome::Done {
                    mode: ListMode::Discover,
                    generation,
                    packages,
                    managers: None,
                    store_managed: BTreeSet::new(),
                },
                Err(error) => ListOutcome::Failed {
                    mode: ListMode::Discover,
                    generation,
                    unavailable: matches!(error, CatalogError::Unavailable(_)),
                    error: error.to_string(),
                    managers: None,
                },
            };
            let _ = sender.send(outcome);
        });
    }

    /// Polls both list workers. Each one is checked on every tick, so a Discover search
    /// never hides an Installed load that is still running.
    pub(super) fn poll_list(&mut self) -> bool {
        let installed = self.poll_list_worker(ListMode::Installed);
        let discover = self.poll_list_worker(ListMode::Discover);
        installed || discover
    }

    fn list_receiver_mut(&mut self, mode: ListMode) -> &mut Option<Receiver<ListOutcome>> {
        match mode {
            ListMode::Installed => &mut self.installed_receiver,
            ListMode::Discover => &mut self.discover_receiver,
        }
    }

    fn list_generation(&self, mode: ListMode) -> u64 {
        match mode {
            ListMode::Installed => self.installed_generation,
            ListMode::Discover => self.discover_generation,
        }
    }

    /// Drains the worker: a cached list and the final answer can both be waiting.
    fn poll_list_worker(&mut self, mode: ListMode) -> bool {
        let mut changed = false;
        loop {
            let polled = match self.list_receiver_mut(mode).as_ref() {
                Some(receiver) => receiver.try_recv(),
                None => return changed,
            };
            match polled {
                Ok(outcome) => {
                    let last = !matches!(outcome, ListOutcome::Cached { .. });
                    if last {
                        *self.list_receiver_mut(mode) = None;
                    }
                    self.apply_list_outcome(outcome);
                    changed = true;
                }
                Err(TryRecvError::Empty) => return changed,
                Err(TryRecvError::Disconnected) => {
                    *self.list_receiver_mut(mode) = None;
                    return self.list_worker_stopped(mode) || changed;
                }
            }
        }
    }

    /// Applies a list answer unless a newer load of the same mode has replaced it.
    fn apply_list_outcome(&mut self, outcome: ListOutcome) {
        match outcome {
            ListOutcome::Cached { generation, cache } => {
                // An update check may already have put a fresh list on screen.
                if generation == self.installed_generation && self.installed_packages.is_empty() {
                    self.apply_cached(cache);
                }
            }
            ListOutcome::Done {
                mode,
                generation,
                mut packages,
                managers,
                store_managed,
            } => {
                if generation != self.list_generation(mode) {
                    return;
                }
                if let Some(managers) = managers {
                    self.managers = managers;
                }
                match mode {
                    ListMode::Installed => {
                        self.apply_installed(packages, store_managed);
                        self.refreshing = false;
                    }
                    ListMode::Discover => {
                        let installed_keys: BTreeSet<&PackageKey> =
                            self.installed_packages.iter().map(|pkg| &pkg.key).collect();
                        packages.retain(|pkg| !installed_keys.contains(&pkg.key));
                        self.discover_packages = packages;
                        self.discover_status = Status::Ready;
                        self.prune_selection();
                        self.start_icon_load();
                    }
                }
            }
            ListOutcome::Failed {
                mode,
                generation,
                error,
                unavailable,
                managers,
            } => {
                if generation != self.list_generation(mode) {
                    return;
                }
                if let Some(managers) = managers {
                    self.managers = managers;
                }
                let status = if unavailable {
                    Status::Unavailable(error.clone())
                } else {
                    Status::Failed(error.clone())
                };
                match mode {
                    ListMode::Installed => {
                        // Keep the previous rows when a silent refresh fails, and say
                        // they are not current.
                        if self.refreshing && !self.installed_packages.is_empty() {
                            self.refreshing = false;
                            self.notice = Some(Notice::RefreshFailed(error));
                        } else {
                            self.installed_packages.clear();
                            self.installed_status = status;
                            self.refreshing = false;
                            self.selected.clear();
                        }
                    }
                    ListMode::Discover => {
                        self.discover_packages.clear();
                        self.discover_status = status;
                        self.selected.clear();
                    }
                }
            }
        }
    }

    /// The worker ended without an answer; the load fails in its own mode.
    fn list_worker_stopped(&mut self, mode: ListMode) -> bool {
        let failed = Status::Failed(LIST_WORKER_STOPPED.into());
        match mode {
            ListMode::Installed
                if matches!(self.installed_status, Status::Loading) || self.refreshing =>
            {
                if self.refreshing && !self.installed_packages.is_empty() {
                    self.refreshing = false;
                    self.notice = Some(Notice::RefreshFailed(LIST_WORKER_STOPPED.into()));
                } else {
                    self.installed_status = failed;
                    self.refreshing = false;
                }
                true
            }
            ListMode::Discover if matches!(self.discover_status, Status::Loading) => {
                self.discover_status = failed;
                true
            }
            _ => false,
        }
    }

    /// The one path for a finished Installed list, whether a list load or a background
    /// update check produced it: rows, updates, logos and selection stay in step.
    pub(super) fn apply_installed(
        &mut self,
        mut packages: Vec<PackageSummary>,
        store_managed: BTreeSet<PackageKey>,
    ) {
        self.overlay_history_dates(&mut packages);
        carry_icons(
            self.catalog.as_ref(),
            &self.installed_packages,
            &mut packages,
        );
        let checked_at = self.catalog.now_local_stamp();
        self.save_installed_cache(&packages, &store_managed, &checked_at);
        self.installed_status = Status::Ready;
        self.last_checked = Some(checked_at);
        self.from_cache = false;
        // An unchanged answer only confirms the rows already on screen.
        if packages == self.installed_packages && store_managed == self.store_managed {
            return;
        }
        self.store_updates(&packages);
        self.store_managed = store_managed;
        self.installed_packages = packages;
        self.prune_selection();
        self.start_icon_load();
    }

    /// Shows the saved list while the managers are still answering (`refreshing`).
    fn apply_cached(&mut self, cache: InstalledCache) {
        let mut packages = cache.packages;
        self.overlay_history_dates(&mut packages);
        self.store_updates(&packages);
        self.store_managed = cache.store_managed;
        self.installed_packages = packages;
        self.installed_status = Status::Ready;
        self.last_checked = Some(cache.checked_at).filter(|stamp| !stamp.is_empty());
        self.refreshing = true;
        self.from_cache = true;
        self.start_icon_load();
    }

    /// Writes on a worker so a slow disk never stalls the UI thread.
    fn save_installed_cache(
        &self,
        packages: &[PackageSummary],
        store_managed: &BTreeSet<PackageKey>,
        checked_at: &str,
    ) {
        let Some(path) = self.installed_cache_path.clone() else {
            return;
        };
        let text = InstalledCache {
            checked_at: checked_at.to_string(),
            packages: packages.to_vec(),
            store_managed: store_managed.clone(),
        }
        .serialize();
        let catalog = self.catalog.clone();
        std::thread::spawn(move || {
            let _ = catalog.write_text(&path, &text);
        });
    }
}

/// `None` when nothing was saved yet, the file is unreadable or holds no rows.
fn read_installed_cache(path: &Path) -> Option<InstalledCache> {
    let text = std::fs::read_to_string(path).ok()?;
    InstalledCache::parse(&text).filter(|cache| !cache.packages.is_empty())
}
