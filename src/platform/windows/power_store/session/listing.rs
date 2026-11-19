//! Loading the Installed and Discover lists on worker threads and applying their answers.
//! Each list mode has its own worker channel and generation (see `Session`).

use super::*;

pub(super) enum ListOutcome {
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
        if silent {
            self.refreshing = true;
        } else {
            self.refreshing = false;
            self.installed_status = Status::Loading;
        }
        std::thread::spawn(move || {
            let outcome = match catalog::list_installed() {
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
                    unavailable: matches!(error, ManagerError::Unavailable(_)),
                    error: error.to_string(),
                    managers: Some(catalog::manager_statuses()),
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
        std::thread::spawn(move || {
            let outcome = match catalog::search_discover(&trimmed, source.as_deref()) {
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
                    unavailable: matches!(error, ManagerError::Unavailable(_)),
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

    fn poll_list_worker(&mut self, mode: ListMode) -> bool {
        let polled = match self.list_receiver_mut(mode).as_ref() {
            Some(receiver) => receiver.try_recv(),
            None => return false,
        };
        match polled {
            Ok(outcome) => {
                *self.list_receiver_mut(mode) = None;
                self.apply_list_outcome(outcome);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                *self.list_receiver_mut(mode) = None;
                self.list_worker_stopped(mode)
            }
        }
    }

    /// Applies a list answer unless a newer load of the same mode has replaced it.
    fn apply_list_outcome(&mut self, outcome: ListOutcome) {
        match outcome {
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
                    Status::Unavailable(error)
                } else {
                    Status::Failed(error)
                };
                match mode {
                    ListMode::Installed => {
                        // Keep the previous rows when a silent refresh fails.
                        if self.refreshing && !self.installed_packages.is_empty() {
                            self.refreshing = false;
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
        carry_icons(&self.installed_packages, &mut packages);
        self.store_updates(&packages);
        self.store_managed = store_managed;
        self.installed_packages = packages;
        self.installed_status = Status::Ready;
        self.last_checked = Some(clock::local_stamp(clock::now_utc()));
        self.prune_selection();
        self.start_icon_load();
    }
}
