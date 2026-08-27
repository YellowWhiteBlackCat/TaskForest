//! Bounded scan-driver lane for the `filesystem.directory.usage` capability.
//!
//! The lane thread never performs filesystem I/O itself: it drives the native
//! provider chunk-by-chunk (each chunk bounded inside the provider) under one
//! whole-scan monotonic budget, publishes rate-limited progress, and folds
//! queued `Cancel` requests into the active scan by polling the request
//! receiver between chunks. Cancels for other scans are idempotent no-ops by
//! contract and are acknowledged immediately instead of occupying carry
//! slots; queued scans are carried over in FIFO order within a bounded
//! carry, and one that arrives while the carry is full receives a typed
//! rejected terminal rather than being dropped, reordered, or retained
//! without bound.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use taskmanager_application::{
    CapabilityId, DirectoryUsageEvent, DirectoryUsageRequest, FailureKind, PlatformEvent,
    ProviderFailure, ProviderId, RequestId,
};
use taskmanager_core::{
    DirectoryScanControl, DirectoryScanId, DirectoryScanSpec, DirectoryScanStatus,
    DirectoryScanTotals, DirectoryUsageSnapshot,
};

use crate::channel::Queued;
use crate::delivery::{
    LaneExitGuard, LaneFlow, ProviderPanicContext, RuntimeEventPublisher, execute_isolated,
    recv_or_shutdown, shutdown_requested,
};
use crate::health::CapabilityHealth;
use crate::{WorkerRuntime, WorkerSpawnError};

/// Minimum wall-clock spacing between progress publications while a scan
/// stays in `Scanning` state. Terminal publications always pass immediately.
const MIN_PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// Defensive bound on carried-over requests while a scan is active. The
/// request channel itself is bounded, so this can never be exceeded in
/// practice; it only keeps a hostile reader from growing the carry buffer.
const MAX_CARRIED_REQUESTS: usize = 32;

/// Overall monotonic budget for one scan driven by this lane.
///
/// Each provider chunk is internally bounded, but nothing else caps the total
/// number of chunks, so a pathological FUSE or network mount that keeps
/// reporting progress without converging would hold the storage lane (and its
/// target lease, which every published progress event renews) indefinitely.
/// Ten minutes sits roughly two orders of magnitude above a healthy
/// multi-million-entry local scan while guaranteeing the lane comes back: on
/// expiry the scan is closed with a typed `TimedOut` terminal using the same
/// honest failure publication as a provider fault, never a fabricated
/// partial-as-complete snapshot.
const MAX_SCAN_BUDGET: Duration = Duration::from_secs(600);

/// Spawn the dedicated directory-usage lane: `StartScan` runs the bounded
/// scan loop, `Cancel` cancels the active scan (idempotent no-op otherwise).
pub fn spawn_directory_usage_lane<F>(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<DirectoryUsageRequest>>,
    publisher: Arc<RuntimeEventPublisher>,
    mut scan: F,
    clock_ms: fn() -> u64,
) -> Result<(), WorkerSpawnError>
where
    F: FnMut(
            &DirectoryScanSpec,
            &DirectoryScanControl,
            u64,
        ) -> Result<DirectoryUsageSnapshot, ProviderFailure>
        + Send
        + 'static,
{
    let lane_exits = publisher.lane_exit_counter();
    workers.spawn(CapabilityId::DIRECTORY_USAGE.to_string(), move |shutdown| {
        let _lane_exit = LaneExitGuard::new(lane_exits);
        let mut carry: VecDeque<Queued<DirectoryUsageRequest>> = VecDeque::new();
        loop {
            let queued = match carry.pop_front() {
                Some(queued) => queued,
                None => match recv_or_shutdown(&receiver, &shutdown) {
                    Some(queued) => queued,
                    None => break,
                },
            };
            match queued.payload {
                DirectoryUsageRequest::StartScan(spec) => {
                    drive_scan(
                        DirectoryScanContext {
                            receiver: &receiver,
                            shutdown: &shutdown,
                            carry: &mut carry,
                            publisher: publisher.as_ref(),
                            scan: &mut scan,
                            clock_ms,
                        },
                        DirectoryScanJob {
                            request_id: queued.request_id,
                            capability: queued.capability,
                            provider: queued.provider,
                            spec,
                        },
                        MAX_SCAN_BUDGET,
                    );
                }
                DirectoryUsageRequest::Cancel(_scan_id) => {
                    // No active scan to cancel: an idempotent no-op by
                    // contract (documented on the request type).
                }
            }
        }
    })
}

/// Drive one scan to its terminal state on the lane thread.
struct DirectoryScanContext<'a, F> {
    receiver: &'a Receiver<Queued<DirectoryUsageRequest>>,
    shutdown: &'a Receiver<()>,
    carry: &'a mut VecDeque<Queued<DirectoryUsageRequest>>,
    publisher: &'a RuntimeEventPublisher,
    scan: &'a mut F,
    clock_ms: fn() -> u64,
}

struct DirectoryScanJob {
    request_id: RequestId,
    capability: CapabilityId,
    provider: ProviderId,
    spec: DirectoryScanSpec,
}

fn drive_scan<F>(context: DirectoryScanContext<'_, F>, job: DirectoryScanJob, budget: Duration)
where
    F: FnMut(
        &DirectoryScanSpec,
        &DirectoryScanControl,
        u64,
    ) -> Result<DirectoryUsageSnapshot, ProviderFailure>,
{
    let DirectoryScanContext {
        receiver,
        shutdown,
        carry,
        publisher,
        scan,
        clock_ms,
    } = context;
    let DirectoryScanJob {
        request_id,
        capability,
        provider,
        spec,
    } = job;
    let scan_id = DirectoryScanId::new(request_id.get());
    let cancelled = Arc::new(AtomicBool::new(false));
    let control = DirectoryScanControl::new(scan_id, cancelled.clone());
    let panic_notes = publisher.panic_ledger();
    let lane = capability.to_string();
    let mut last_published: Option<Instant> = None;
    // Fail closed: a deadline that cannot be represented is already expired.
    let deadline = Instant::now().checked_add(budget).unwrap_or(Instant::now());

    loop {
        if shutdown_requested(shutdown) {
            return;
        }
        if Instant::now() >= deadline {
            // Whole-scan budget exhausted: publish the typed timeout terminal
            // through the same honest failure path as a provider fault (no
            // fabricated entries, no partial state presented as complete).
            publish_update(
                publisher,
                request_id,
                provider,
                DirectoryUsageSnapshot {
                    scan_id,
                    root: spec.root.clone(),
                    status: DirectoryScanStatus::Failed(FailureKind::TimedOut),
                    entries: Vec::new(),
                    totals: DirectoryScanTotals::fresh(clock_ms()),
                },
            );
            return;
        }
        // The native walker runs under the same panic isolation as every
        // other lane's provider call: a panicking chunk degrades to one typed
        // terminal failure instead of stranding the lane.
        match execute_isolated(
            &panic_notes,
            ProviderPanicContext {
                lane: lane.clone(),
                capability: capability.clone(),
                request_id,
            },
            || scan(&spec, &control, clock_ms()),
        ) {
            Ok(snapshot) => {
                if shutdown_requested(shutdown) {
                    return;
                }
                let terminal = snapshot.is_terminal();
                let publish_now = terminal
                    || last_published.is_none_or(|at| at.elapsed() >= MIN_PROGRESS_INTERVAL);
                if publish_now {
                    if publish_update(publisher, request_id, provider.clone(), snapshot).is_stop() {
                        return;
                    }
                    last_published = Some(Instant::now());
                }
                if terminal {
                    return;
                }
            }
            Err(failure) => {
                // Provider-level failure: publish a typed terminal failure
                // with the honest partial state (no fabricated entries).
                publish_update(
                    publisher,
                    request_id,
                    provider,
                    DirectoryUsageSnapshot {
                        scan_id,
                        root: spec.root.clone(),
                        status: DirectoryScanStatus::Failed(failure.kind()),
                        entries: Vec::new(),
                        totals: DirectoryScanTotals::fresh(clock_ms()),
                    },
                );
                return;
            }
        }

        // Fold queued requests into the active scan between chunks.
        if fold_queued_requests(receiver, carry, scan_id, &cancelled, publisher, clock_ms).is_stop()
        {
            return;
        }
    }
}

/// Drain the request channel into the active scan between chunks.
///
/// The channel is drained unconditionally — even while the carry buffer is
/// full — so a `Cancel` for the active scan takes effect within one chunk
/// instead of starving behind carried work for the rest of the scan. A
/// cancel addressing any other scan is an idempotent no-op by contract and
/// is acknowledged here without occupying a carry slot, which keeps the whole
/// carry budget available for `StartScan` requests; those keep their FIFO
/// order. A `StartScan` that arrives while the carry is full is completed
/// immediately with a typed rejected terminal — the honest bounded-lane
/// admission outcome — rather than being dropped, reordered, or retained
/// without bound. Returns [`LaneFlow::Stop`] only when the event transport
/// is gone.
fn fold_queued_requests(
    receiver: &Receiver<Queued<DirectoryUsageRequest>>,
    carry: &mut VecDeque<Queued<DirectoryUsageRequest>>,
    scan_id: DirectoryScanId,
    cancelled: &AtomicBool,
    publisher: &RuntimeEventPublisher,
    clock_ms: fn() -> u64,
) -> LaneFlow {
    loop {
        match receiver.try_recv() {
            Ok(queued) => match queued.payload {
                DirectoryUsageRequest::Cancel(id) if id == scan_id => {
                    cancelled.store(true, Ordering::Relaxed);
                    return LaneFlow::Continue;
                }
                DirectoryUsageRequest::Cancel(_) => {
                    // Foreign cancel: resolving it as a no-op now is
                    // observably identical to carrying it past this scan,
                    // which would also end as a no-op.
                }
                DirectoryUsageRequest::StartScan(spec) => {
                    if carry.len() < MAX_CARRIED_REQUESTS {
                        carry.push_back(Queued {
                            request_id: queued.request_id,
                            capability: queued.capability,
                            provider: queued.provider,
                            payload: DirectoryUsageRequest::StartScan(spec),
                        });
                    } else if publish_update(
                        publisher,
                        queued.request_id,
                        queued.provider,
                        DirectoryUsageSnapshot {
                            scan_id: DirectoryScanId::new(queued.request_id.get()),
                            root: spec.root.clone(),
                            status: DirectoryScanStatus::Failed(FailureKind::Rejected),
                            entries: Vec::new(),
                            totals: DirectoryScanTotals::fresh(clock_ms()),
                        },
                    )
                    .is_stop()
                    {
                        return LaneFlow::Stop;
                    }
                }
            },
            Err(_) => return LaneFlow::Continue,
        }
    }
}

/// Publish one scan update. Progress publications skip the capability-health
/// record; terminal ones record it, so the catalog reflects the final state
/// of the scan (available for completed/cancelled, unavailable for failures).
fn publish_update(
    publisher: &RuntimeEventPublisher,
    request_id: RequestId,
    provider: ProviderId,
    snapshot: DirectoryUsageSnapshot,
) -> LaneFlow {
    let event = PlatformEvent::DirectoryUsage(DirectoryUsageEvent::Update(snapshot.clone()));
    if snapshot.is_terminal() {
        let health = match snapshot.status {
            DirectoryScanStatus::Completed | DirectoryScanStatus::Cancelled => {
                CapabilityHealth::Available
            }
            DirectoryScanStatus::Failed(failure) => {
                CapabilityHealth::Unavailable(ProviderFailure::from_kind(failure))
            }
            DirectoryScanStatus::Scanning => CapabilityHealth::Available,
        };
        publisher.publish_health(
            request_id,
            CapabilityId::DIRECTORY_USAGE,
            provider,
            event,
            health,
        )
    } else {
        publisher.publish_progress(request_id, CapabilityId::DIRECTORY_USAGE, provider, event)
    }
}

#[cfg(test)]
#[path = "../../tests/headless/runtime_storage_directory_scan_tests.rs"]
mod tests;
