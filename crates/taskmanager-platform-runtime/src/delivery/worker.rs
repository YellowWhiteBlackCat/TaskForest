//! Provider execution workers with owned, cooperative lifecycle control.
//!
//! A [`WorkerRuntime`] owns every lane thread spawned for one native runtime.
//! Dropping that owner disconnects a shared shutdown channel, which wakes idle
//! lanes immediately. A lane already blocked inside a provider can only stop
//! after that provider returns; drop therefore never waits for an unfinished
//! thread. Native providers remain responsible for bounding their own OS I/O.

use std::any::Any;
use std::error;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, TryRecvError, select};
use taskmanager_platform_contract::ProviderFailure;

use super::catalog::{ProviderPanicContext, ProviderPanicLedger};
use super::publisher::RuntimeEventPublisher;
use crate::channel::Queued;

mod lanes;
mod registry;

pub use lanes::{
    spawn_health_observation_lane, spawn_lane, spawn_observation_lane, spawn_typed_outcome_lane,
};
pub(crate) use lanes::{
    spawn_lazy_health_observation_lane, spawn_lazy_lane, spawn_lazy_observation_lane,
    spawn_lazy_typed_outcome_lane,
};
pub(crate) use registry::{LaneStartRegistry, recv_or_shutdown_with_idle, spawn_or_register_lane};

/// Defensive ceiling for the standard runtime's independently blocking lanes.
pub const DEFAULT_WORKER_LIMIT: usize = 64;

/// Provider lanes are allowed to block independently, but their default
/// stacks should not reserve the platform's large libc default for every
/// short-lived adapter callback. The limit is deliberately conservative:
/// providers still have a full MiB for native call frames, while the runtime
/// avoids reserving several hundred MiB of virtual address space as routes
/// grow.
const DEFAULT_WORKER_STACK_SIZE: usize = 1024 * 1024;

/// Process-wide ceiling retained by stuck, detached provider threads too.
pub const PROCESS_WORKER_LIMIT: usize = 128;

static PROCESS_WORKER_QUOTA: OnceLock<Arc<WorkerQuota>> = OnceLock::new();

pub(crate) struct WorkerQuota {
    live: AtomicUsize,
    limit: usize,
}

impl WorkerQuota {
    pub(crate) const fn new(limit: usize) -> Self {
        Self {
            live: AtomicUsize::new(0),
            limit,
        }
    }

    fn acquire(self: &Arc<Self>) -> Option<WorkerPermit> {
        let acquired = self
            .live
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |live| {
                (live < self.limit).then_some(live + 1)
            })
            .is_ok();
        acquired.then(|| WorkerPermit {
            quota: Arc::clone(self),
        })
    }
}

struct WorkerPermit {
    quota: Arc<WorkerQuota>,
}

impl Drop for WorkerPermit {
    fn drop(&mut self) {
        self.quota.live.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Typed failure to create a provider worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerSpawnError {
    /// Runtime configuration attempted to exceed its worker cardinality bound.
    Capacity { worker: String, limit: usize },
    /// Stuck workers from current or retired runtimes exhausted the process quota.
    ProcessCapacity { worker: String, limit: usize },
    /// The operating system rejected `std::thread::Builder::spawn`.
    OperatingSystem {
        worker: String,
        kind: std::io::ErrorKind,
        message: String,
    },
    /// The composition attempted to register two workers for one capability.
    Registration { worker: String, message: String },
    /// A lazy starter outlived the runtime owner that should execute it.
    OwnerGone { worker: String },
}

impl fmt::Display for WorkerSpawnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capacity { worker, limit } => write!(
                formatter,
                "runtime worker limit {limit} reached while starting {worker}"
            ),
            Self::ProcessCapacity { worker, limit } => write!(
                formatter,
                "process worker limit {limit} reached while starting {worker}"
            ),
            Self::OperatingSystem {
                worker,
                kind,
                message,
            } => write!(
                formatter,
                "operating system rejected worker for {worker} ({kind:?}): {message}"
            ),
            Self::Registration { worker, message } => write!(
                formatter,
                "worker registration failed for {worker}: {message}"
            ),
            Self::OwnerGone { worker } => {
                write!(formatter, "runtime owner gone before starting {worker}")
            }
        }
    }
}

impl error::Error for WorkerSpawnError {}

/// Process-local owner for all provider lane threads of one platform runtime.
///
/// The owner is attached opaquely to `PlatformHandle`, so clones share its
/// lifetime. Shutdown is cooperative. Completed threads are joined during
/// drop; unfinished threads are detached rather than risking an unbounded wait
/// on provider I/O.
pub struct WorkerRuntime {
    shutdown_tx: Option<Sender<()>>,
    shutdown_rx: Receiver<()>,
    handles: Mutex<Vec<JoinHandle<()>>>,
    limit: usize,
    quota: Arc<WorkerQuota>,
    lane_starters: OnceLock<Arc<registry::LaneStartRegistry>>,
}

impl Default for WorkerRuntime {
    fn default() -> Self {
        Self::new(DEFAULT_WORKER_LIMIT)
    }
}

impl WorkerRuntime {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        let quota = Arc::clone(
            PROCESS_WORKER_QUOTA.get_or_init(|| Arc::new(WorkerQuota::new(PROCESS_WORKER_LIMIT))),
        );
        Self::with_quota(limit, quota)
    }

    pub(crate) fn with_quota(limit: usize, quota: Arc<WorkerQuota>) -> Self {
        let (shutdown_tx, shutdown_rx) = crossbeam_channel::bounded(0);
        Self {
            shutdown_tx: Some(shutdown_tx),
            shutdown_rx,
            handles: Mutex::new(Vec::new()),
            limit,
            quota,
            lane_starters: OnceLock::new(),
        }
    }

    /// Install the shared lazy-lane registry before native assembly registers
    /// any provider closure. The registry only keeps a weak reference back to
    /// this owner, so dropping the last platform handle still shuts down the
    /// runtime without a reference cycle.
    pub(crate) fn install_lane_starters(&self, starters: Arc<registry::LaneStartRegistry>) {
        let _ = self.lane_starters.set(starters);
    }

    fn lane_starters(&self) -> Option<Arc<registry::LaneStartRegistry>> {
        self.lane_starters.get().cloned()
    }

    /// Number of workers successfully created for this runtime.
    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Join and remove workers that have already returned without waiting on
    /// any provider that is still executing.
    pub fn reap_finished(&self) -> usize {
        let mut handles = self
            .handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reap_finished_locked(&mut handles)
    }

    pub(crate) fn spawn<F>(&self, worker: String, run: F) -> Result<(), WorkerSpawnError>
    where
        F: FnOnce(Receiver<()>) + Send + 'static,
    {
        let mut handles = self
            .handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reap_finished_locked(&mut handles);
        if handles.len() >= self.limit {
            return Err(WorkerSpawnError::Capacity {
                worker,
                limit: self.limit,
            });
        }
        let Some(permit) = self.quota.acquire() else {
            return Err(WorkerSpawnError::ProcessCapacity {
                worker,
                limit: self.quota.limit,
            });
        };
        let shutdown = self.shutdown_rx.clone();
        let name = format!("taskforest-{worker}");
        let handle = thread::Builder::new()
            .name(name)
            .stack_size(DEFAULT_WORKER_STACK_SIZE)
            .spawn(move || {
                let _permit = permit;
                run(shutdown);
            })
            .map_err(|error| WorkerSpawnError::OperatingSystem {
                worker,
                kind: error.kind(),
                message: error.to_string(),
            })?;
        handles.push(handle);
        Ok(())
    }
}

impl Drop for WorkerRuntime {
    fn drop(&mut self) {
        // Disconnecting the sole sender wakes every receiver clone. Never send
        // one token: a rendezvous token would wake only one lane.
        drop(self.shutdown_tx.take());
        let handles = self
            .handles
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for handle in handles.drain(..) {
            if handle.is_finished() {
                let _ = handle.join();
            }
            // Dropping an unfinished JoinHandle intentionally detaches it.
            // It still observes shutdown after its current provider call, but
            // this Drop path must not wait on unbounded native I/O.
        }
    }
}

pub(crate) fn shutdown_requested(shutdown: &Receiver<()>) -> bool {
    matches!(
        shutdown.try_recv(),
        Ok(()) | Err(TryRecvError::Disconnected)
    )
}

/// Join and remove handles whose threads already returned. The caller must
/// hold the `handles` lock; `is_finished` guarantees `join` cannot block on a
/// provider that is still executing.
fn reap_finished_locked(handles: &mut Vec<JoinHandle<()>>) -> usize {
    let mut reaped = 0;
    let mut index = 0;
    while index < handles.len() {
        if handles[index].is_finished() {
            let handle = handles.swap_remove(index);
            let _ = handle.join();
            reaped += 1;
        } else {
            index += 1;
        }
    }
    reaped
}

pub(crate) fn recv_or_shutdown<R>(
    receiver: &Receiver<Queued<R>>,
    shutdown: &Receiver<()>,
) -> Option<Queued<R>> {
    if shutdown_requested(shutdown) {
        return None;
    }
    select! {
        recv(shutdown) -> _ => None,
        recv(receiver) -> queued => queued.ok(),
    }
}

/// Records one lane thread exit into the shared ledger when the thread ends,
/// whether it returned from its loop, broke on a gone transport, or panicked
/// outside the provider isolation boundary.
pub(crate) struct LaneExitGuard {
    exits: Arc<std::sync::atomic::AtomicU64>,
}

impl LaneExitGuard {
    pub(crate) fn new(exits: Arc<std::sync::atomic::AtomicU64>) -> Self {
        Self { exits }
    }
}

impl Drop for LaneExitGuard {
    fn drop(&mut self) {
        self.exits
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }
}

/// Run a provider closure with panic isolation and bounded panic diagnostics.
pub(crate) fn execute_isolated<T, F>(
    panic_notes: &ProviderPanicLedger,
    context: ProviderPanicContext,
    run: F,
) -> Result<T, ProviderFailure>
where
    F: FnOnce() -> Result<T, ProviderFailure>,
{
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(inner) => inner,
        Err(payload) => {
            panic_notes.record(context, panic_payload_text(&*payload));
            Err(ProviderFailure::ProviderFault)
        }
    }
}

fn panic_payload_text(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "(non-string panic payload)".to_owned()
    }
}

fn panic_context(lane: &str, queued: &Queued<impl Sized>) -> ProviderPanicContext {
    ProviderPanicContext {
        lane: lane.to_owned(),
        capability: queued.capability.clone(),
        request_id: queued.request_id,
    }
}

pub(super) fn worker_name<R>() -> String {
    std::any::type_name::<R>()
        .rsplit("::")
        .next()
        .unwrap_or("provider")
        .to_owned()
}
