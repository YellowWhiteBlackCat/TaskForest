//! Typed, independently recoverable writer-health partitions.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

use taskmanager_history_store::HistoryStoreErrorKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryPersistenceWorkerState {
    Running,
    Stopped,
    Detached,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryPersistenceRuntimeStatus {
    pub worker: HistoryPersistenceWorkerState,
    /// Bounded isolation detail of the panic that flipped `worker` off
    /// `Running`; `None` while no fault has been caught. The typed worker
    /// state stays the primary signal — this field only explains it.
    pub worker_fault: Option<Arc<str>>,
    pub record_lane_drops: u64,
    pub store_backpressure_drops: u64,
    pub store_rejections: u64,
    pub duplicate_records: u64,
    pub flushes: u64,
    pub flush_failures: u64,
    pub record_failure: Option<HistoryPersistenceFailure>,
    pub flush_failure: Option<HistoryPersistenceFailure>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryPersistenceOperation {
    Record,
    Flush,
}

impl HistoryPersistenceOperation {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Flush => "flush",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryPersistenceFailureKind {
    Read,
    Write,
    ResourceLimit,
    Backpressure,
    Locked,
}

impl HistoryPersistenceFailureKind {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::ResourceLimit => "resource_limit",
            Self::Backpressure => "backpressure",
            Self::Locked => "locked",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryPersistenceFailure {
    pub operation: HistoryPersistenceOperation,
    pub kind: HistoryPersistenceFailureKind,
    pub observed_at_ms: u64,
    pub detail: Arc<str>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct HistoryPersistenceHealth {
    pub(super) worker_fault: Option<Arc<str>>,
    pub(super) store_backpressure_drops: u64,
    pub(super) store_rejections: u64,
    pub(super) duplicate_records: u64,
    pub(super) flushes: u64,
    pub(super) flush_failures: u64,
    pub(super) record_failure: Option<HistoryPersistenceFailure>,
    pub(super) flush_failure: Option<HistoryPersistenceFailure>,
}

#[derive(Clone, Debug)]
pub struct HistoryPersistenceRuntimeMonitor {
    pub(super) state: Arc<AtomicU8>,
    pub(super) record_lane_drops: Arc<AtomicU64>,
    pub(super) health: Arc<Mutex<HistoryPersistenceHealth>>,
}

impl HistoryPersistenceRuntimeMonitor {
    #[must_use]
    pub fn status(&self) -> HistoryPersistenceRuntimeStatus {
        let worker = match self.state.load(Ordering::Acquire) {
            1 => HistoryPersistenceWorkerState::Stopped,
            2 => HistoryPersistenceWorkerState::Detached,
            _ => HistoryPersistenceWorkerState::Running,
        };
        let health = self
            .health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        HistoryPersistenceRuntimeStatus {
            worker,
            worker_fault: health.worker_fault,
            record_lane_drops: self.record_lane_drops.load(Ordering::Acquire),
            store_backpressure_drops: health.store_backpressure_drops,
            store_rejections: health.store_rejections,
            duplicate_records: health.duplicate_records,
            flushes: health.flushes,
            flush_failures: health.flush_failures,
            record_failure: health.record_failure,
            flush_failure: health.flush_failure,
        }
    }
}

pub(super) fn map_persistence_failure_kind(
    kind: HistoryStoreErrorKind,
) -> HistoryPersistenceFailureKind {
    match kind {
        HistoryStoreErrorKind::ResourceLimit => HistoryPersistenceFailureKind::ResourceLimit,
        HistoryStoreErrorKind::Locked => HistoryPersistenceFailureKind::Locked,
        HistoryStoreErrorKind::CreateDirectory
        | HistoryStoreErrorKind::Open
        | HistoryStoreErrorKind::Write
        | HistoryStoreErrorKind::Remove
        | HistoryStoreErrorKind::Rename
        | HistoryStoreErrorKind::Encode => HistoryPersistenceFailureKind::Write,
        HistoryStoreErrorKind::Read | HistoryStoreErrorKind::Decode => {
            HistoryPersistenceFailureKind::Read
        }
    }
}

pub(super) fn saturating_increment(value: &AtomicU64) {
    let _ = value.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        Some(current.saturating_add(1))
    });
}

pub(super) fn bounded_failure_detail(detail: &str) -> Arc<str> {
    const MAX_CHARS: usize = 512;
    if detail.chars().count() <= MAX_CHARS {
        Arc::from(detail)
    } else {
        Arc::from(detail.chars().take(MAX_CHARS).collect::<String>())
    }
}
