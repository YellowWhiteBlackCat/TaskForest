//! Provider execution workers with owned, cooperative lifecycle control.
//!
//! A [`WorkerRuntime`] owns every lane thread spawned for one native runtime.
//! Dropping that owner disconnects a shared shutdown channel, which wakes idle
//! lanes immediately. A lane already blocked inside a provider can only stop
//! after that provider returns; drop therefore never waits for an unfinished
//! thread. Native providers remain responsible for bounding their OS I/O.

use std::any::Any;
use std::error;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, TryRecvError, select};
use taskmanager_application::PlatformEvent;
use taskmanager_platform_contract::{ProviderFailure, RequestId};

use super::catalog::{ProviderPanicContext, ProviderPanicLedger};
use super::publisher::RuntimeEventPublisher;
use crate::channel::Queued;
use crate::health::{CapabilityHealth, ObservationHealth};

/// Defensive ceiling for the standard runtime's independently blocking lanes.
pub const DEFAULT_WORKER_LIMIT: usize = 64;

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
        }
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
    ///
    /// `WorkerRuntime::spawn` performs the same reclamation inside its own
    /// lock before every capacity check, so a lane whose thread died cannot
    /// pin the runtime ceiling; this method remains for callers that want to
    /// reclaim eagerly between spawns.
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
        // Reclaim lanes whose threads already exited before consulting the
        // capacity bound: a dead lane's lingering JoinHandle must not keep
        // occupying the runtime ceiling and turn every rebuild into a
        // `Capacity` rejection.
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
    exits: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl LaneExitGuard {
    pub(crate) fn new(exits: std::sync::Arc<std::sync::atomic::AtomicU64>) -> Self {
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
///
/// Every lane — including the dedicated directory-scan driver — routes its
/// provider call through this seam, so a provider panic degrades into one
/// typed `ProviderFault` publication instead of killing the lane thread and
/// stranding every later request on it as an unrecoverable stall. The panic
/// payload is additionally downcast to text and retained, with the lane and
/// request context, in `panic_notes`: the typed failure alone cannot say what
/// the provider actually panicked on.
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

/// Best-effort text of one panic payload: `panic!` with a literal or
/// formatted message downcasts to `&str`/`String`; a `panic_any` payload of
/// any other type gets the fixed placeholder instead of fabricated details.
fn panic_payload_text(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "(non-string panic payload)".to_owned()
    }
}

/// Lane/request context for one isolated call on a generic request lane.
fn panic_context(lane: &str, queued: &Queued<impl Sized>) -> ProviderPanicContext {
    ProviderPanicContext {
        lane: lane.to_owned(),
        capability: queued.capability.clone(),
        request_id: queued.request_id,
    }
}

/// Attach one independently blocking provider closure to a bounded request lane.
pub fn spawn_lane<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    mut execute: F,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    F: FnMut(R) -> Result<PlatformEvent, ProviderFailure> + Send + 'static,
{
    let lane_exits = publisher.lane_exit_counter();
    let panic_notes = publisher.panic_ledger();
    let lane = worker_name::<R>();
    workers.spawn(lane.clone(), move |shutdown| {
        let _lane_exit = LaneExitGuard::new(lane_exits);
        while let Some(queued) = recv_or_shutdown(&receiver, &shutdown) {
            let result = execute_isolated(&panic_notes, panic_context(&lane, &queued), || {
                execute(queued.payload)
            });
            if shutdown_requested(&shutdown)
                || publisher
                    .publish(
                        queued.request_id,
                        queued.capability,
                        queued.provider,
                        result,
                    )
                    .is_stop()
            {
                break;
            }
        }
    })
}

/// Attach a source-rich observation provider whose execution result and
/// observation health are independent.
pub fn spawn_health_observation_lane<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    mut execute: F,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    F: FnMut(R) -> Result<(PlatformEvent, CapabilityHealth), ProviderFailure> + Send + 'static,
{
    let lane_exits = publisher.lane_exit_counter();
    let panic_notes = publisher.panic_ledger();
    let lane = worker_name::<R>();
    workers.spawn(lane.clone(), move |shutdown| {
        let _lane_exit = LaneExitGuard::new(lane_exits);
        while let Some(queued) = recv_or_shutdown(&receiver, &shutdown) {
            let publication = execute_isolated(&panic_notes, panic_context(&lane, &queued), || {
                execute(queued.payload)
            });
            if shutdown_requested(&shutdown) {
                break;
            }
            let published = match publication {
                Ok((event, health)) => publisher.publish_health(
                    queued.request_id,
                    queued.capability,
                    queued.provider,
                    event,
                    health,
                ),
                Err(failure) => publisher.publish(
                    queued.request_id,
                    queued.capability,
                    queued.provider,
                    Err(failure),
                ),
            };
            if published.is_stop() {
                break;
            }
        }
    })
}

/// Attach a typed observation provider whose health is derived from the exact
/// snapshot mapped into the published domain event.
pub fn spawn_observation_lane<R, S, F, M>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    mut observe: F,
    map_event: M,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    S: ObservationHealth + Send + 'static,
    F: FnMut(R) -> Result<S, ProviderFailure> + Send + 'static,
    M: Fn(S) -> PlatformEvent + Send + 'static,
{
    let lane_exits = publisher.lane_exit_counter();
    let panic_notes = publisher.panic_ledger();
    let lane = worker_name::<R>();
    workers.spawn(lane.clone(), move |shutdown| {
        let _lane_exit = LaneExitGuard::new(lane_exits);
        while let Some(queued) = recv_or_shutdown(&receiver, &shutdown) {
            // The observation health and the event mapping run inside the same
            // isolation boundary as the observation itself: they are pure
            // projections of the snapshot, and a panic in either must degrade
            // to one typed failure rather than escaping the lane thread.
            let publication = execute_isolated(&panic_notes, panic_context(&lane, &queued), || {
                let snapshot = observe(queued.payload)?;
                let health = snapshot.observation_health();
                let event = map_event(snapshot);
                Ok((event, health))
            });
            if shutdown_requested(&shutdown) {
                break;
            }
            let published = match publication {
                Ok((event, health)) => publisher.publish_health(
                    queued.request_id,
                    queued.capability,
                    queued.provider,
                    event,
                    health,
                ),
                Err(failure) => publisher.publish(
                    queued.request_id,
                    queued.capability,
                    queued.provider,
                    Err(failure),
                ),
            };
            if published.is_stop() {
                break;
            }
        }
    })
}

/// Attach a provider closure whose domain event carries its own typed outcome.
pub fn spawn_typed_outcome_lane<R, F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    mut execute: F,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    F: FnMut(RequestId, R) -> (PlatformEvent, Result<(), ProviderFailure>) + Send + 'static,
{
    let lane_exits = publisher.lane_exit_counter();
    let panic_notes = publisher.panic_ledger();
    let lane = worker_name::<R>();
    workers.spawn(lane.clone(), move |shutdown| {
        let _lane_exit = LaneExitGuard::new(lane_exits);
        while let Some(queued) = recv_or_shutdown(&receiver, &shutdown) {
            // Same isolation seam and panic-note semantics as every other
            // lane; the provider result tuple is only wrapped in `Ok` so the
            // typed outcome survives unchanged when the call does not panic.
            let publication = execute_isolated(&panic_notes, panic_context(&lane, &queued), || {
                Ok(execute(queued.request_id, queued.payload))
            });
            if shutdown_requested(&shutdown) {
                break;
            }
            let published = match publication {
                Ok((event, provider_result)) => publisher.publish_typed_outcome(
                    queued.request_id,
                    queued.capability,
                    queued.provider,
                    event,
                    provider_result,
                ),
                Err(failure) => publisher.publish(
                    queued.request_id,
                    queued.capability,
                    queued.provider,
                    Err(failure),
                ),
            };
            if published.is_stop() {
                break;
            }
        }
    })
}

fn worker_name<R>() -> String {
    std::any::type_name::<R>()
        .rsplit("::")
        .next()
        .unwrap_or("provider")
        .to_owned()
}
