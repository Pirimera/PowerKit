//! Install, update and uninstall operations: kinds, progress and the serial queue.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Install,
    Update,
    Uninstall,
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Update => "update",
            Self::Uninstall => "uninstall",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "install" => Self::Install,
            "update" => Self::Update,
            "uninstall" => Self::Uninstall,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressStage {
    Queued,
    Downloading,
    Installing,
    Uninstalling,
    Finishing,
}

impl ProgressStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Installing => "installing",
            Self::Uninstalling => "uninstalling",
            Self::Finishing => "finishing",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "queued" => Self::Queued,
            "downloading" => Self::Downloading,
            "installing" => Self::Installing,
            "uninstalling" => Self::Uninstalling,
            "finishing" => Self::Finishing,
            _ => return None,
        })
    }
}

/// Progress of the running operation. `percent` is absent when the manager reports none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationProgress {
    pub stage: ProgressStage,
    pub percent: Option<u8>,
}

impl OperationProgress {
    /// `fraction` is the manager's 0.0–1.0 value; zero, negative and NaN mean "unknown".
    pub fn new(stage: ProgressStage, fraction: f64) -> Self {
        Self {
            stage,
            percent: (fraction > 0.0).then(|| (fraction.min(1.0) * 100.0).round() as u8),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedOperation {
    pub kind: OperationKind,
    pub key: PackageKey,
    pub name: String,
    pub options: InstallOptions,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OperationQueue {
    items: VecDeque<QueuedOperation>,
}

impl OperationQueue {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn push(&mut self, operation: QueuedOperation) {
        if operation.key.is_own_package()
            && matches!(
                operation.kind,
                OperationKind::Update | OperationKind::Uninstall
            )
        {
            return;
        }
        // Page and tray "Update all" can both name the same package; it runs once.
        if self
            .items
            .iter()
            .any(|queued| queued.kind == operation.kind && queued.key == operation.key)
        {
            return;
        }
        self.items.push_back(operation);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn pop_front(&mut self) -> Option<QueuedOperation> {
        self.items.pop_front()
    }

    pub fn remove_at(&mut self, index: usize) -> Option<QueuedOperation> {
        if index >= self.items.len() {
            return None;
        }
        self.items.remove(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &QueuedOperation> {
        self.items.iter()
    }
}
