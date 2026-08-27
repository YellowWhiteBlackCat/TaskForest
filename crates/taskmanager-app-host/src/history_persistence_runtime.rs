//! Bounded app-host owner for the active frontend session's history writes.

use std::fmt;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, AtomicU64, Ordering},
};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, Sender, bounded, select, tick};
use taskmanager_core::{HistoricalSample, HistoryRecordSink, HistorySeriesKey};
use taskmanager_history_store::{
    HistoryStoreError, HistoryStoreErrorKind, PersistentHistoryStore, RecordSampleOutcome,
    RetentionPolicy,
};

mod health;
pub use health::{
    HistoryPersistenceFailure, HistoryPersistenceFailureKind, HistoryPersistenceOperation,
    HistoryPersistenceRuntimeMonitor, HistoryPersistenceRuntimeStatus,
    HistoryPersistenceWorkerState,
};
use health::{
    HistoryPersistenceHealth, bounded_failure_detail, map_persistence_failure_kind,
    saturating_increment,
};

use crate::worker_fault::catch_worker_panic;

pub const HISTORY_RECORD_COMMAND_CAPACITY: usize = 4_096;
const HISTORY_FLUSH_INTERVAL: Duration = Duration::from_secs(10);
const HISTORY_BOOTSTRAP_WAIT: Duration = Duration::from_millis(100);
const HISTORY_SHUTDOWN_WAIT: Duration = Duration::from_millis(100);
const HISTORY_PERSISTENCE_SHUTDOWN_WAIT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryPersistenceStartErrorKind {
    Locked,
    Open,
    WorkerStart,
    BootstrapTimedOut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryPersistenceStartError {
    kind: HistoryPersistenceStartErrorKind,
    detail: Arc<str>,
}

impl HistoryPersistenceStartError {
    #[must_use]
    pub const fn kind(&self) -> HistoryPersistenceStartErrorKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for HistoryPersistenceStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            HistoryPersistenceStartErrorKind::Locked => "history writer is already owned",
            HistoryPersistenceStartErrorKind::Open => "history writer could not open",
            HistoryPersistenceStartErrorKind::WorkerStart => {
                "history writer worker failed to start"
            }
            HistoryPersistenceStartErrorKind::BootstrapTimedOut => {
                "history writer bootstrap timed out"
            }
        })
    }
}

struct RecordCommand {
    key: HistorySeriesKey,
    sample: HistoricalSample,
}

struct HistoryPersistenceRuntimeInner {
    record_tx: Sender<RecordCommand>,
    shutdown_tx: Sender<()>,
    done_rx: Receiver<()>,
    worker_state: Arc<AtomicU8>,
    record_lane_drops: Arc<AtomicU64>,
    health: Arc<Mutex<HistoryPersistenceHealth>>,
    join: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryPersistenceShutdownOutcome {
    Released,
    Detached,
}

impl HistoryPersistenceRuntimeInner {
    fn shutdown(&self, wait: Duration) -> HistoryPersistenceShutdownOutcome {
        let _ = self.shutdown_tx.try_send(());
        let mut join = self
            .join
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if join.is_none() {
            return if self.worker_state.load(Ordering::Acquire) == 2 {
                HistoryPersistenceShutdownOutcome::Detached
            } else {
                HistoryPersistenceShutdownOutcome::Released
            };
        }
        if self.done_rx.recv_timeout(wait).is_ok() {
            if let Some(join) = join.take() {
                let _ = join.join();
            }
            HistoryPersistenceShutdownOutcome::Released
        } else {
            self.worker_state.store(2, Ordering::Release);
            HistoryPersistenceShutdownOutcome::Detached
        }
    }
}

impl Drop for HistoryPersistenceRuntimeInner {
    fn drop(&mut self) {
        // A stuck filesystem must not freeze toolkit teardown. The frontend
        // uses its longer explicit shutdown seam before this fallback.
        let _ = self.shutdown(HISTORY_SHUTDOWN_WAIT);
    }
}

struct HistoryRecordIngress {
    inner: Arc<HistoryPersistenceRuntimeInner>,
}

impl HistoryRecordSink for HistoryRecordIngress {
    fn record_sample(&self, key: HistorySeriesKey, sample: HistoricalSample) {
        if self
            .inner
            .record_tx
            .try_send(RecordCommand { key, sample })
            .is_err()
        {
            saturating_increment(&self.inner.record_lane_drops);
        }
    }
}

pub struct HistoryPersistenceCoordinator {
    inner: Arc<HistoryPersistenceRuntimeInner>,
}

impl fmt::Debug for HistoryPersistenceCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HistoryPersistenceCoordinator")
            .finish_non_exhaustive()
    }
}

impl HistoryPersistenceCoordinator {
    pub(crate) fn start_path_bounded(
        path: impl AsRef<Path>,
        holder_is_gone: fn(u32) -> bool,
    ) -> Result<Self, HistoryPersistenceStartError> {
        let root = path.as_ref().to_path_buf();
        Self::start_with_opener(
            move || Self::start_path(root, holder_is_gone),
            HISTORY_BOOTSTRAP_WAIT,
        )
    }

    pub(crate) fn start_path(
        path: impl AsRef<Path>,
        holder_is_gone: fn(u32) -> bool,
    ) -> Result<Self, HistoryPersistenceStartError> {
        Self::start_path_with_interval(path, holder_is_gone, HISTORY_FLUSH_INTERVAL)
    }

    fn start_path_with_interval(
        path: impl AsRef<Path>,
        holder_is_gone: fn(u32) -> bool,
        flush_interval: Duration,
    ) -> Result<Self, HistoryPersistenceStartError> {
        let store = PersistentHistoryStore::open(
            path.as_ref().to_path_buf(),
            RetentionPolicy::default(),
            holder_is_gone,
        )
        .map_err(map_start_error)?;
        let backend = Box::new(PersistentBackend { store });
        Self::start_with_backend(backend, flush_interval)
    }

    fn start_with_backend(
        backend: Box<dyn HistoryPersistenceBackend>,
        flush_interval: Duration,
    ) -> Result<Self, HistoryPersistenceStartError> {
        let (record_tx, record_rx) = bounded(HISTORY_RECORD_COMMAND_CAPACITY);
        let (shutdown_tx, shutdown_rx) = bounded(1);
        let (done_tx, done_rx) = bounded(1);
        let worker_state = Arc::new(AtomicU8::new(0));
        let worker_state_for_thread = Arc::clone(&worker_state);
        let record_lane_drops = Arc::new(AtomicU64::new(0));
        let health = Arc::new(Mutex::new(HistoryPersistenceHealth::default()));
        let health_for_thread = Arc::clone(&health);
        let join = std::thread::Builder::new()
            .name("taskforest-history-writer".to_owned())
            .spawn(move || {
                // Whole-loop isolation, deliberately not per-record: the
                // persistent backend owns OS file, buffer and lock state that a
                // panic may leave half-written, so the honest degradation is a
                // registered thread exit the owning frontend observes, never a
                // continue-after-fault on the same store. The exit bookkeeping
                // below runs even after a caught panic — the old post-loop
                // ordering left a dead writer reporting `Running` forever.
                if let Err(detail) = catch_worker_panic(|| {
                    worker_loop(
                        record_rx,
                        shutdown_rx,
                        backend,
                        flush_interval,
                        Arc::clone(&health_for_thread),
                    )
                }) {
                    record_worker_fault(&health_for_thread, detail);
                }
                let _ = worker_state_for_thread.compare_exchange(
                    0,
                    1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                let _ = done_tx.try_send(());
            })
            .map_err(|error| HistoryPersistenceStartError {
                kind: HistoryPersistenceStartErrorKind::WorkerStart,
                detail: bounded_failure_detail(&error.to_string()),
            })?;
        Ok(Self {
            inner: Arc::new(HistoryPersistenceRuntimeInner {
                record_tx,
                shutdown_tx,
                done_rx,
                worker_state,
                record_lane_drops,
                health,
                join: Mutex::new(Some(join)),
            }),
        })
    }

    fn start_with_opener<F>(opener: F, wait: Duration) -> Result<Self, HistoryPersistenceStartError>
    where
        F: FnOnce() -> Result<Self, HistoryPersistenceStartError> + Send + 'static,
    {
        let (result_tx, result_rx) = bounded(1);
        std::thread::Builder::new()
            .name("taskforest-history-bootstrap".to_owned())
            .spawn(move || {
                // If the bounded caller has timed out, sending fails and the
                // coordinator (if opening eventually succeeded) is dropped on
                // this background thread, never transferred into a frontend.
                let _ = result_tx.send(opener());
            })
            .map_err(|error| HistoryPersistenceStartError {
                kind: HistoryPersistenceStartErrorKind::WorkerStart,
                detail: bounded_failure_detail(&error.to_string()),
            })?;
        match result_rx.recv_timeout(wait) {
            Ok(result) => result,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                Err(HistoryPersistenceStartError {
                    kind: HistoryPersistenceStartErrorKind::BootstrapTimedOut,
                    detail: Arc::from(
                        "history directory or lock did not open before the startup deadline",
                    ),
                })
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                Err(HistoryPersistenceStartError {
                    kind: HistoryPersistenceStartErrorKind::WorkerStart,
                    detail: Arc::from("history bootstrap worker stopped before publication"),
                })
            }
        }
    }

    /// Capability set for the active frontend session. It owns the writer and
    /// health monitor but no replay or boot-comparison client.
    #[must_use]
    pub fn into_persistence_writer(self) -> HistoryPersistenceWriter {
        HistoryPersistenceWriter {
            record_sink: self.record_sink(),
            monitor: self.monitor(),
            owner: self,
        }
    }

    fn record_sink(&self) -> Arc<dyn HistoryRecordSink> {
        Arc::new(HistoryRecordIngress {
            inner: Arc::clone(&self.inner),
        })
    }

    #[must_use]
    pub fn monitor(&self) -> HistoryPersistenceRuntimeMonitor {
        HistoryPersistenceRuntimeMonitor {
            state: Arc::clone(&self.inner.worker_state),
            record_lane_drops: Arc::clone(&self.inner.record_lane_drops),
            health: Arc::clone(&self.inner.health),
        }
    }
}

pub struct HistoryPersistenceWriter {
    pub record_sink: Arc<dyn HistoryRecordSink>,
    pub monitor: HistoryPersistenceRuntimeMonitor,
    owner: HistoryPersistenceCoordinator,
}

impl fmt::Debug for HistoryPersistenceWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HistoryPersistenceWriter")
            .field("status", &self.monitor.status())
            .finish_non_exhaustive()
    }
}

impl HistoryPersistenceWriter {
    /// Flush and release this frontend process's unique writer generation. A
    /// detached result keeps the lock owner observable instead of claiming a
    /// disabled state while a filesystem worker still owns it.
    #[must_use]
    pub fn shutdown(self) -> HistoryPersistenceShutdownOutcome {
        self.owner.inner.shutdown(HISTORY_PERSISTENCE_SHUTDOWN_WAIT)
    }
}

trait HistoryPersistenceBackend: Send {
    fn record(&mut self, command: RecordCommand) -> BackendRecordOutcome;
    fn flush(&mut self, now_ms: u64) -> Result<(), HistoryStoreError>;
}

struct PersistentBackend {
    store: PersistentHistoryStore,
}

impl HistoryPersistenceBackend for PersistentBackend {
    fn record(&mut self, command: RecordCommand) -> BackendRecordOutcome {
        match self.store.try_record_sample(command.key, command.sample) {
            RecordSampleOutcome::Accepted => BackendRecordOutcome::Accepted,
            RecordSampleOutcome::AcceptedWithBackpressure { dropped_samples } => {
                BackendRecordOutcome::AcceptedWithBackpressure { dropped_samples }
            }
            RecordSampleOutcome::DuplicateRevision => BackendRecordOutcome::Duplicate,
            RecordSampleOutcome::Rejected(rejection) => {
                BackendRecordOutcome::Rejected(Arc::from(format!("{rejection:?}")))
            }
        }
    }

    fn flush(&mut self, now_ms: u64) -> Result<(), HistoryStoreError> {
        self.store.flush(now_ms).map(|_| ())
    }
}

fn worker_loop(
    record_rx: Receiver<RecordCommand>,
    shutdown_rx: Receiver<()>,
    mut backend: Box<dyn HistoryPersistenceBackend>,
    flush_interval: Duration,
    health: Arc<Mutex<HistoryPersistenceHealth>>,
) {
    let flush_tick = tick(flush_interval);
    loop {
        if shutdown_rx.try_recv().is_ok() {
            drain_before_shutdown(&record_rx, backend.as_mut(), &health);
            flush_backend(backend.as_mut(), &health);
            return;
        }
        // Crossbeam's unbiased selector randomizes among ready operations.
        // Shutdown gets the explicit pre-check above; flush and record
        // admission cannot starve one another under a hot record lane.
        select! {
            recv(shutdown_rx) -> _ => {
                drain_before_shutdown(&record_rx, backend.as_mut(), &health);
                flush_backend(backend.as_mut(), &health);
                return;
            },
            recv(record_rx) -> command => {
                let Ok(command) = command else { return };
                record_backend(backend.as_mut(), command, &health);
            },
            recv(flush_tick) -> _ => {
                flush_backend(backend.as_mut(), &health);
            },
        }
    }
}

/// Record the bounded isolation detail of a writer fault into the health
/// partition. The typed `Stopped` worker state remains the primary signal;
/// this detail only explains it and is already char-bounded upstream.
fn record_worker_fault(health: &Mutex<HistoryPersistenceHealth>, detail: Arc<str>) {
    let mut health = health
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    health.worker_fault = Some(detail);
}

fn drain_before_shutdown(
    record_rx: &Receiver<RecordCommand>,
    backend: &mut dyn HistoryPersistenceBackend,
    health: &Mutex<HistoryPersistenceHealth>,
) {
    while let Ok(command) = record_rx.try_recv() {
        record_backend(backend, command, health);
    }
}

enum BackendRecordOutcome {
    Accepted,
    AcceptedWithBackpressure { dropped_samples: usize },
    Duplicate,
    Rejected(Arc<str>),
}

fn record_backend(
    backend: &mut dyn HistoryPersistenceBackend,
    command: RecordCommand,
    health: &Mutex<HistoryPersistenceHealth>,
) {
    let outcome = backend.record(command);
    let mut health = health
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match outcome {
        BackendRecordOutcome::Accepted => {
            health.record_failure = None;
        }
        BackendRecordOutcome::AcceptedWithBackpressure { dropped_samples } => {
            health.store_backpressure_drops = health
                .store_backpressure_drops
                .saturating_add(u64::try_from(dropped_samples).unwrap_or(u64::MAX));
            health.record_failure = Some(HistoryPersistenceFailure {
                operation: HistoryPersistenceOperation::Record,
                kind: HistoryPersistenceFailureKind::Backpressure,
                observed_at_ms: unix_now_ms(),
                detail: Arc::from("history store dropped older pending samples under backpressure"),
            });
        }
        BackendRecordOutcome::Duplicate => {
            health.duplicate_records = health.duplicate_records.saturating_add(1);
        }
        BackendRecordOutcome::Rejected(detail) => {
            health.store_rejections = health.store_rejections.saturating_add(1);
            health.record_failure = Some(HistoryPersistenceFailure {
                operation: HistoryPersistenceOperation::Record,
                kind: HistoryPersistenceFailureKind::ResourceLimit,
                observed_at_ms: unix_now_ms(),
                detail: bounded_failure_detail(&detail),
            });
        }
    }
}

fn flush_backend(
    backend: &mut dyn HistoryPersistenceBackend,
    health: &Mutex<HistoryPersistenceHealth>,
) {
    let now_ms = unix_now_ms();
    let outcome = backend.flush(now_ms);
    let mut health = health
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match outcome {
        Ok(()) => {
            health.flushes = health.flushes.saturating_add(1);
            health.flush_failure = None;
        }
        Err(error) => {
            health.flush_failures = health.flush_failures.saturating_add(1);
            health.flush_failure = Some(HistoryPersistenceFailure {
                operation: HistoryPersistenceOperation::Flush,
                kind: map_persistence_failure_kind(error.kind()),
                observed_at_ms: now_ms,
                detail: bounded_failure_detail(error.detail()),
            });
        }
    }
}

fn map_start_error(error: HistoryStoreError) -> HistoryPersistenceStartError {
    HistoryPersistenceStartError {
        kind: if error.kind() == HistoryStoreErrorKind::Locked {
            HistoryPersistenceStartErrorKind::Locked
        } else {
            HistoryPersistenceStartErrorKind::Open
        },
        detail: bounded_failure_detail(error.detail()),
    }
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "../tests/headless/history_persistence_runtime_tests.rs"]
mod tests;
