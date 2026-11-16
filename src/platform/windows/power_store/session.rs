//! PowerStore UI session: list refresh and serial install/update/uninstall queue.
use super::winget::{WinGetError, WinGetSource};
use crate::power_store::{
    OperationKind, OperationQueue, PackageSummary, QueuedOperation, StoreSection, UpdateFilters,
    filter_updatable,
};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;

pub const MAX_VISIBLE_PACKAGES: usize = 100;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Ready,
    Unavailable,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub available: String,
    pub source: String,
}

impl From<&PackageSummary> for PackageRow {
    fn from(package: &PackageSummary) -> Self {
        Self {
            id: package.key.id.clone(),
            name: package.name.clone(),
            version: package.version.clone().unwrap_or_default(),
            available: package.available_version.clone().unwrap_or_default(),
            source: package.key.source.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpStatus {
    pub running: Option<String>,
    pub pending: usize,
    pub last_result: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub section: StoreSection,
    pub status: Status,
    pub query: String,
    pub packages: Vec<PackageRow>,
    pub truncated: bool,
    pub op: OpStatus,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            section: StoreSection::Installed,
            status: Status::Idle,
            query: String::new(),
            packages: Vec::new(),
            truncated: false,
            op: OpStatus::default(),
        }
    }
}

enum ListOutcome {
    Done {
        section: StoreSection,
        packages: Vec<PackageSummary>,
    },
    Failed {
        section: StoreSection,
        error: String,
        unavailable: bool,
    },
}

enum OpOutcome {
    Finished {
        label: String,
        result: Result<(), String>,
    },
}

pub struct Session {
    section: StoreSection,
    status: Status,
    query: String,
    packages: Vec<PackageSummary>,
    list_receiver: Option<Receiver<ListOutcome>>,
    queue: OperationQueue,
    running: Option<QueuedOperation>,
    last_result: Option<String>,
    op_receiver: Option<Receiver<OpOutcome>>,
    op_worker: Option<JoinHandle<()>>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            section: StoreSection::Installed,
            status: Status::Idle,
            query: String::new(),
            packages: Vec::new(),
            list_receiver: None,
            queue: OperationQueue::default(),
            running: None,
            last_result: None,
            op_receiver: None,
            op_worker: None,
        }
    }
}

impl Session {
    pub fn snapshot(&self) -> Snapshot {
        let total = self.packages.len();
        let packages = self
            .packages
            .iter()
            .take(MAX_VISIBLE_PACKAGES)
            .map(PackageRow::from)
            .collect();
        Snapshot {
            section: self.section,
            status: self.status.clone(),
            query: self.query.clone(),
            packages,
            truncated: total > MAX_VISIBLE_PACKAGES,
            op: OpStatus {
                running: self.running.as_ref().map(operation_label),
                pending: self.queue.len(),
                last_result: self.last_result.clone(),
            },
        }
    }

    pub fn set_section(&mut self, section: StoreSection) {
        if self.section != section {
            self.section = section;
            self.packages.clear();
            if !matches!(self.status, Status::Loading) {
                self.status = Status::Idle;
            }
        }
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
    }

    pub fn enqueue(&mut self, operation: QueuedOperation) {
        self.queue.push(operation);
        self.kick_queue();
    }

    pub fn enqueue_updates(&mut self) {
        let ops: Vec<QueuedOperation> = self
            .packages
            .iter()
            .map(|package| QueuedOperation {
                kind: OperationKind::Update,
                key: package.key.clone(),
                name: package.name.clone(),
                options: crate::power_store::InstallOptions::default(),
            })
            .collect();
        for operation in ops {
            self.queue.push(operation);
        }
        self.kick_queue();
    }

    pub fn clear_pending(&mut self) {
        self.queue.clear();
    }

    #[allow(dead_code)] // Tray/status will report queue occupancy once update counts land.
    pub fn busy(&self) -> bool {
        self.running.is_some() || self.op_receiver.is_some() || !self.queue.is_empty()
    }

    /// Clears pending work and joins a finished worker; does not abort a live WinGet op mid-flight.
    pub fn shutdown(&mut self) {
        self.queue.clear();
        if let Some(receiver) = self.op_receiver.take() {
            let _ = receiver.recv();
        }
        if let Some(handle) = self.op_worker.take() {
            let _ = handle.join();
        }
        self.running = None;
        if let Some(receiver) = self.list_receiver.take() {
            let _ = receiver.recv_timeout(std::time::Duration::from_millis(50));
        }
    }

    pub fn poll(&mut self) -> bool {
        let list_changed = self.poll_list();
        let op_changed = self.poll_ops();
        list_changed || op_changed
    }

    pub fn refresh(&mut self, filters: UpdateFilters) {
        if self.list_receiver.is_some() {
            return;
        }
        let section = self.section;
        let query = self.query.clone();
        let (sender, receiver) = mpsc::channel();
        self.list_receiver = Some(receiver);
        self.status = Status::Loading;
        std::thread::spawn(move || {
            let _ = sender.send(run_list_job(section, query, filters));
        });
    }

    fn poll_list(&mut self) -> bool {
        let Some(receiver) = &self.list_receiver else {
            return false;
        };
        match receiver.try_recv() {
            Ok(outcome) => {
                self.list_receiver = None;
                match outcome {
                    ListOutcome::Done { section, packages } => {
                        if section == self.section {
                            self.packages = packages;
                            self.status = Status::Ready;
                        }
                    }
                    ListOutcome::Failed {
                        section,
                        error,
                        unavailable,
                    } => {
                        if section == self.section {
                            self.packages.clear();
                            self.status = if unavailable {
                                Status::Unavailable
                            } else {
                                Status::Failed(error)
                            };
                        }
                    }
                }
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.list_receiver = None;
                if matches!(self.status, Status::Loading) {
                    self.status = Status::Failed("PowerStore worker stopped".into());
                    true
                } else {
                    false
                }
            }
        }
    }

    fn poll_ops(&mut self) -> bool {
        let Some(receiver) = &self.op_receiver else {
            return false;
        };
        match receiver.try_recv() {
            Ok(OpOutcome::Finished { label, result }) => {
                self.op_receiver = None;
                if let Some(handle) = self.op_worker.take() {
                    let _ = handle.join();
                }
                self.running = None;
                self.last_result = Some(match result {
                    Ok(()) => format!("{label}: ok"),
                    Err(error) => format!("{label}: {error}"),
                });
                self.kick_queue();
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.op_receiver = None;
                if let Some(handle) = self.op_worker.take() {
                    let _ = handle.join();
                }
                self.running = None;
                self.last_result = Some("PowerStore operation worker stopped".into());
                self.kick_queue();
                true
            }
        }
    }

    fn kick_queue(&mut self) {
        if self.op_receiver.is_some() || self.running.is_some() {
            return;
        }
        let Some(operation) = self.queue.pop_front() else {
            return;
        };
        let label = operation_label(&operation);
        self.running = Some(operation.clone());
        let (sender, receiver) = mpsc::channel();
        self.op_receiver = Some(receiver);
        self.op_worker = Some(std::thread::spawn(move || {
            let result = run_operation_job(operation).map_err(|error| error.to_string());
            let _ = sender.send(OpOutcome::Finished { label, result });
        }));
    }
}

fn operation_label(operation: &QueuedOperation) -> String {
    let kind = match operation.kind {
        OperationKind::Install => "install",
        OperationKind::Update => "update",
        OperationKind::Uninstall => "uninstall",
    };
    format!("{kind} {}", operation.name)
}

fn run_list_job(section: StoreSection, query: String, filters: UpdateFilters) -> ListOutcome {
    let source = match WinGetSource::connect() {
        Ok(source) => source,
        Err(error) => {
            return ListOutcome::Failed {
                section,
                unavailable: matches!(error, WinGetError::Unavailable(_)),
                error: error.to_string(),
            };
        }
    };
    let result = match section {
        StoreSection::Discover => {
            let trimmed = query.trim();
            if trimmed.is_empty() {
                Ok(Vec::new())
            } else if trimmed.contains('.') && !trimmed.contains(' ') {
                source.search_equals_id(trimmed)
            } else {
                source.search_query(trimmed)
            }
        }
        StoreSection::Installed => source.list_installed(),
        StoreSection::Updates => source.list_installed().map(|packages| {
            filter_updatable(&packages, &filters)
                .into_iter()
                .cloned()
                .collect()
        }),
    };
    match result {
        Ok(packages) => ListOutcome::Done { section, packages },
        Err(error) => ListOutcome::Failed {
            section,
            unavailable: matches!(error, WinGetError::Unavailable(_)),
            error: error.to_string(),
        },
    }
}

fn run_operation_job(operation: QueuedOperation) -> Result<(), WinGetError> {
    let source = WinGetSource::connect()?;
    source.run_operation(&operation)
}
