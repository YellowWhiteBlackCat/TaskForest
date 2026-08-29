use crossbeam_channel::{TrySendError, bounded};
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::{
    DirectoryScanBounds, DirectoryScanStatus, DirectoryScanTotals, DirectoryUsageEntry,
    DirectoryUsageSnapshot,
};
use taskmanager_platform_contract::{EventEnvelope, RequestScope, RequestTracking, SidebandPolicy};

use super::*;
use crate::channel::Queued;
use crate::config::{CapabilityRoute, DeliveryClass};
use crate::delivery::{RuntimeCapabilityCatalog, RuntimeEventPublisher};

fn fixed_clock() -> u64 {
    77
}

fn queued(request_id: u64, payload: DirectoryUsageRequest) -> Queued<DirectoryUsageRequest> {
    Queued {
        request_id: RequestId::new(request_id).expect("non-zero fixture request id"),
        capability: CapabilityId::DIRECTORY_USAGE,
        provider: ProviderId::borrowed("fixture.directory-usage"),
        payload,
    }
}

fn fixture_publisher() -> (
    Arc<RuntimeEventPublisher>,
    crossbeam_channel::Receiver<crate::delivery::event_queue::QueuedEvent>,
    Arc<RuntimeCapabilityCatalog>,
) {
    let routes = [CapabilityRoute {
        capability: CapabilityId::DIRECTORY_USAGE,
        provider: ProviderId::borrowed("fixture.directory-usage"),
        delivery: DeliveryClass::Observation,
        domain: crate::config::RuntimeDomain::Storage,
        cadence_ms: Some(1_000),
        sideband_policy: SidebandPolicy::Idempotent,
    }];
    let catalog = Arc::new(RuntimeCapabilityCatalog::new(&routes, fixed_clock));
    let (control_tx, _control_rx) = bounded(2);
    let (observation_tx, observation_rx) = bounded(64);
    let publisher = Arc::new(RuntimeEventPublisher::new(
        control_tx,
        observation_tx,
        catalog.clone(),
        Vec::new(),
        fixed_clock,
    ));
    (publisher, observation_rx, catalog)
}

fn reserve_scan(catalog: &RuntimeCapabilityCatalog, request_id: u64, root: &str) {
    assert!(
        catalog
            .ecs_scheduler_handle()
            .lock()
            .expect("scheduler lock")
            .reserve_submission_with_tracking(
                &CapabilityId::DIRECTORY_USAGE,
                RequestId::new(request_id).expect("fixture request ID"),
                0,
                RequestTracking::Target(
                    RequestScope::try_from_str(root).expect("bounded fixture root"),
                ),
            )
    );
}

/// A scripted provider: returns a Scanning snapshot for `chunks` calls
/// then a Completed one; records how many times the control token was
/// observed cancelled.
struct ScriptedProvider {
    chunks_remaining: usize,
    cancel_observations: Arc<AtomicUsize>,
}

impl ScriptedProvider {
    fn chunk(
        &mut self,
        spec: &DirectoryScanSpec,
        control: &DirectoryScanControl,
    ) -> DirectoryUsageSnapshot {
        if control.is_cancelled() {
            self.cancel_observations.fetch_add(1, Ordering::Relaxed);
            return DirectoryUsageSnapshot {
                scan_id: control.scan_id(),
                root: spec.root.clone(),
                status: DirectoryScanStatus::Cancelled,
                entries: Vec::new(),
                totals: DirectoryScanTotals::fresh(10),
            };
        }
        if self.chunks_remaining > 0 {
            self.chunks_remaining -= 1;
            DirectoryUsageSnapshot {
                scan_id: control.scan_id(),
                root: spec.root.clone(),
                status: DirectoryScanStatus::Scanning,
                entries: Vec::new(),
                totals: DirectoryScanTotals::fresh(10),
            }
        } else {
            DirectoryUsageSnapshot {
                scan_id: control.scan_id(),
                root: spec.root.clone(),
                status: DirectoryScanStatus::Completed,
                entries: vec![DirectoryUsageEntry::root(10)],
                totals: DirectoryScanTotals::fresh(10),
            }
        }
    }
}

fn snapshot_of(envelope: &EventEnvelope<PlatformEvent>) -> DirectoryUsageSnapshot {
    let Ok(event) = &envelope.outcome else {
        panic!("scan must not fail: {:?}", envelope.outcome);
    };
    match event {
        PlatformEvent::DirectoryUsage(DirectoryUsageEvent::Update(snapshot)) => snapshot.clone(),
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn scan_publishes_progress_then_terminal_completion() {
    let (tx, rx) = bounded(4);
    let (publisher, event_rx, catalog) = fixture_publisher();
    let mut provider = ScriptedProvider {
        chunks_remaining: 2,
        cancel_observations: Arc::new(AtomicUsize::new(0)),
    };
    let spec = DirectoryScanSpec {
        root: "/fixture".to_string(),
        bounds: DirectoryScanBounds::default(),
    };
    reserve_scan(&catalog, 7, &spec.root);
    tx.send(queued(7, DirectoryUsageRequest::StartScan(spec.clone())))
        .expect("send start");
    drop(tx);
    let workers = crate::WorkerRuntime::default();
    spawn_directory_usage_lane(
        &workers,
        rx,
        publisher,
        move |spec, control, _observed_at_ms| Ok(provider.chunk(spec, control)),
        fixed_clock,
    )
    .expect("directory worker starts");

    let mut updates = Vec::new();
    while let Ok(envelope) = event_rx.recv_timeout(std::time::Duration::from_secs(5)) {
        updates.push(snapshot_of(&envelope));
    }
    assert!(
        updates
            .iter()
            .any(|s| s.status == DirectoryScanStatus::Scanning),
        "progress publication missing: {updates:?}"
    );
    assert!(
        updates
            .iter()
            .any(|s| s.status == DirectoryScanStatus::Completed),
        "terminal publication missing: {updates:?}"
    );
    assert!(
        updates
            .last()
            .is_some_and(|s| s.status == DirectoryScanStatus::Completed),
        "terminal publication must be last"
    );
}

#[test]
fn cancel_mid_scan_sets_the_control_flag_and_publishes_cancelled() {
    let (tx, rx) = bounded(4);
    let (publisher, event_rx, catalog) = fixture_publisher();
    let observations = Arc::new(AtomicUsize::new(0));
    let provider_observations = observations.clone();
    let spec = DirectoryScanSpec {
        root: "/fixture".to_string(),
        bounds: DirectoryScanBounds::default(),
    };
    let scan_id = DirectoryScanId::new(7);
    reserve_scan(&catalog, 7, &spec.root);
    tx.send(queued(7, DirectoryUsageRequest::StartScan(spec.clone())))
        .expect("send start");
    tx.send(queued(8, DirectoryUsageRequest::Cancel(scan_id)))
        .expect("send cancel");
    drop(tx);
    let workers = crate::WorkerRuntime::default();
    spawn_directory_usage_lane(
        &workers,
        rx,
        publisher,
        move |spec, control, _observed_at_ms| {
            // Simulate a long-running bounded scan: each chunk returns
            // Scanning until the cancel flag is observed.
            if control.is_cancelled() {
                provider_observations.fetch_add(1, Ordering::Relaxed);
                return Ok(DirectoryUsageSnapshot {
                    scan_id: control.scan_id(),
                    root: spec.root.clone(),
                    status: DirectoryScanStatus::Cancelled,
                    entries: Vec::new(),
                    totals: DirectoryScanTotals::fresh(10),
                });
            }
            std::thread::sleep(Duration::from_millis(20));
            Ok(DirectoryUsageSnapshot {
                scan_id: control.scan_id(),
                root: spec.root.clone(),
                status: DirectoryScanStatus::Scanning,
                entries: Vec::new(),
                totals: DirectoryScanTotals::fresh(10),
            })
        },
        fixed_clock,
    )
    .expect("directory worker starts");

    let mut saw_cancelled = false;
    while let Ok(envelope) = event_rx.recv_timeout(std::time::Duration::from_secs(5)) {
        let snapshot = snapshot_of(&envelope);
        if snapshot.status == DirectoryScanStatus::Cancelled {
            saw_cancelled = true;
            break;
        }
    }
    assert!(saw_cancelled, "cancelled terminal state missing");
    assert!(
        observations.load(Ordering::Relaxed) >= 1,
        "the provider must observe the cancel flag"
    );
}

#[test]
fn cancel_of_an_unknown_scan_is_a_noop_that_never_breaks_the_lane() {
    let (tx, rx) = bounded(4);
    let (publisher, event_rx, catalog) = fixture_publisher();
    let spec = DirectoryScanSpec {
        root: "/fixture".to_string(),
        bounds: DirectoryScanBounds::default(),
    };
    reserve_scan(&catalog, 7, &spec.root);
    // The foreign cancel id (999) matches nothing.
    tx.send(queued(
        999,
        DirectoryUsageRequest::Cancel(DirectoryScanId::new(999)),
    ))
    .expect("send cancel");
    tx.send(queued(7, DirectoryUsageRequest::StartScan(spec.clone())))
        .expect("send start");
    drop(tx);
    let workers = crate::WorkerRuntime::default();
    spawn_directory_usage_lane(
        &workers,
        rx,
        publisher,
        move |spec, control, _observed_at_ms| {
            Ok(DirectoryUsageSnapshot {
                scan_id: control.scan_id(),
                root: spec.root.clone(),
                status: DirectoryScanStatus::Completed,
                entries: Vec::new(),
                totals: DirectoryScanTotals::fresh(10),
            })
        },
        fixed_clock,
    )
    .expect("directory worker starts");

    let envelope = event_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the queued scan must still run after the foreign cancel");
    let snapshot = snapshot_of(&envelope);
    assert_eq!(snapshot.status, DirectoryScanStatus::Completed);
    assert_eq!(snapshot.scan_id, DirectoryScanId::new(7));
}

#[test]
fn a_foreign_cancel_flood_never_wedges_the_active_scan() {
    const RECEIVER_CAPACITY: usize = 4;
    const SCANNING_CHUNKS: usize = 96;

    let (tx, rx) = bounded(RECEIVER_CAPACITY);
    let producer = tx.clone();
    let (publisher, event_rx, catalog) = fixture_publisher();
    let spec = DirectoryScanSpec {
        root: "/fixture/bounded-carry".to_string(),
        bounds: DirectoryScanBounds::default(),
    };
    reserve_scan(&catalog, 7, &spec.root);
    tx.send(queued(7, DirectoryUsageRequest::StartScan(spec.clone())))
        .expect("send start");
    drop(tx);

    // Every flooded request is a cancel for an unknown scan: an idempotent
    // no-op by contract. The lane resolves them inside the active scan, so
    // the flood can never occupy the carry buffer or starve the lane.
    let successful_foreign_sends = Arc::new(AtomicUsize::new(0));
    let successful = successful_foreign_sends.clone();
    let chunk = Arc::new(AtomicUsize::new(0));
    let chunk_index = chunk.clone();
    let workers = crate::WorkerRuntime::default();
    spawn_directory_usage_lane(
        &workers,
        rx,
        publisher,
        move |spec, control, _observed_at_ms| {
            let index = chunk_index.fetch_add(1, Ordering::Relaxed);
            if index < SCANNING_CHUNKS {
                match producer.try_send(queued(
                    1_000 + index as u64,
                    DirectoryUsageRequest::Cancel(DirectoryScanId::new(999)),
                )) {
                    Ok(()) => {
                        successful.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => {
                        panic!("active directory lane disconnected unexpectedly");
                    }
                }
                return Ok(DirectoryUsageSnapshot {
                    scan_id: control.scan_id(),
                    root: spec.root.clone(),
                    status: DirectoryScanStatus::Scanning,
                    entries: Vec::new(),
                    totals: DirectoryScanTotals::fresh(10),
                });
            }
            Ok(DirectoryUsageSnapshot {
                scan_id: control.scan_id(),
                root: spec.root.clone(),
                status: DirectoryScanStatus::Completed,
                entries: Vec::new(),
                totals: DirectoryScanTotals::fresh(10),
            })
        },
        fixed_clock,
    )
    .expect("directory worker starts");

    let mut completed = false;
    while let Ok(envelope) = event_rx.recv_timeout(Duration::from_secs(5)) {
        if snapshot_of(&envelope).status == DirectoryScanStatus::Completed {
            completed = true;
            break;
        }
    }
    assert!(
        completed,
        "the long scan must still reach its terminal event"
    );
    assert!(
        successful_foreign_sends.load(Ordering::Relaxed) > 0,
        "the flood must actually have reached the lane"
    );
}

#[test]
fn a_panicking_scan_chunk_degrades_to_a_typed_failure_and_keeps_the_lane() {
    let (tx, rx) = bounded(4);
    let (publisher, event_rx, catalog) = fixture_publisher();
    let spec = DirectoryScanSpec {
        root: "/boom".to_string(),
        bounds: DirectoryScanBounds::default(),
    };
    reserve_scan(&catalog, 21, &spec.root);
    tx.send(queued(21, DirectoryUsageRequest::StartScan(spec.clone())))
        .expect("send panicking scan");
    // The second scan proves the lane survived the first scan's panic.
    let calm_spec = DirectoryScanSpec {
        root: "/calm".to_string(),
        bounds: DirectoryScanBounds::default(),
    };
    reserve_scan(&catalog, 22, &calm_spec.root);
    tx.send(queued(22, DirectoryUsageRequest::StartScan(calm_spec)))
        .expect("send calm scan");
    drop(tx);
    let workers = crate::WorkerRuntime::default();
    spawn_directory_usage_lane(
        &workers,
        rx,
        publisher,
        move |spec, control, _observed_at_ms| {
            if spec.root == "/boom" {
                panic!("provider walker panicked mid-chunk");
            }
            Ok(DirectoryUsageSnapshot {
                scan_id: control.scan_id(),
                root: spec.root.clone(),
                status: DirectoryScanStatus::Completed,
                entries: Vec::new(),
                totals: DirectoryScanTotals::fresh(10),
            })
        },
        fixed_clock,
    )
    .expect("directory worker starts");

    let mut outcomes = Vec::new();
    while let Ok(envelope) = event_rx.recv_timeout(std::time::Duration::from_secs(5)) {
        outcomes.push(envelope);
    }
    let terminal_statuses: Vec<(u64, &DirectoryScanStatus)> = outcomes
        .iter()
        .filter(|envelope| envelope.outcome.is_ok())
        .filter_map(|envelope| {
            let Ok(event) = &envelope.outcome else {
                return None;
            };
            let PlatformEvent::DirectoryUsage(DirectoryUsageEvent::Update(snapshot)) = event else {
                return None;
            };
            let terminal = matches!(
                snapshot.status,
                DirectoryScanStatus::Completed
                    | DirectoryScanStatus::Cancelled
                    | DirectoryScanStatus::Failed(_)
            );
            terminal.then(|| (envelope.request_id.get(), &snapshot.status))
        })
        .collect();
    assert!(
        terminal_statuses
            .iter()
            .any(|(id, status)| { *id == 21 && matches!(status, DirectoryScanStatus::Failed(_)) }),
        "the panicking scan must publish a typed Failed terminal: {terminal_statuses:?}"
    );
    assert!(
        terminal_statuses
            .iter()
            .any(|(id, status)| *id == 22 && *status == &DirectoryScanStatus::Completed),
        "the lane must keep serving after a provider panic: {terminal_statuses:?}"
    );
}

/// Scaffolding for driving one scan directly on the test thread with a
/// synthetic request receiver, carry buffer and shutdown channel, so the
/// fold behavior can be exercised with a pre-filled carry.
struct DirectScan {
    request_id: u64,
    root: String,
    receiver: crossbeam_channel::Receiver<Queued<DirectoryUsageRequest>>,
    _shutdown_tx: crossbeam_channel::Sender<()>,
    shutdown: crossbeam_channel::Receiver<()>,
    carry: VecDeque<Queued<DirectoryUsageRequest>>,
    publisher: Arc<RuntimeEventPublisher>,
    event_rx: crossbeam_channel::Receiver<crate::delivery::event_queue::QueuedEvent>,
    catalog: Arc<RuntimeCapabilityCatalog>,
}

fn direct_scan_fixture(request_id: u64, root: &str) -> DirectScan {
    let (publisher, event_rx, catalog) = fixture_publisher();
    let (tx, rx) = bounded(4);
    drop(tx);
    let (shutdown_tx, shutdown) = bounded::<()>(1);
    reserve_scan(&catalog, request_id, root);
    DirectScan {
        request_id,
        root: root.to_string(),
        receiver: rx,
        _shutdown_tx: shutdown_tx,
        shutdown,
        carry: VecDeque::new(),
        publisher,
        event_rx,
        catalog,
    }
}

fn prefill_carry(fixture: &mut DirectScan) {
    for index in 0..MAX_CARRIED_REQUESTS {
        let spec = DirectoryScanSpec {
            root: format!("/fixture/carried/{index}"),
            bounds: DirectoryScanBounds::default(),
        };
        fixture.carry.push_back(queued(
            100 + index as u64,
            DirectoryUsageRequest::StartScan(spec),
        ));
    }
}

fn run_direct_scan<F>(
    fixture: DirectScan,
    provider: &mut F,
    budget: Duration,
) -> (
    Vec<crate::delivery::event_queue::QueuedEvent>,
    VecDeque<Queued<DirectoryUsageRequest>>,
)
where
    F: FnMut(
        &DirectoryScanSpec,
        &DirectoryScanControl,
        u64,
    ) -> Result<DirectoryUsageSnapshot, ProviderFailure>,
{
    let DirectScan {
        request_id,
        root,
        receiver,
        _shutdown_tx,
        shutdown,
        mut carry,
        publisher,
        event_rx,
        catalog: _,
    } = fixture;
    let job = DirectoryScanJob {
        request_id: RequestId::new(request_id).expect("non-zero fixture request id"),
        capability: CapabilityId::DIRECTORY_USAGE,
        provider: ProviderId::borrowed("fixture.directory-usage"),
        spec: DirectoryScanSpec {
            root,
            bounds: DirectoryScanBounds::default(),
        },
    };
    drive_scan(
        DirectoryScanContext {
            receiver: &receiver,
            shutdown: &shutdown,
            carry: &mut carry,
            publisher: publisher.as_ref(),
            scan: provider,
            clock_ms: fixed_clock,
        },
        job,
        budget,
    );
    let envelopes = event_rx.try_iter().collect();
    (envelopes, carry)
}

#[test]
fn cancel_for_the_active_scan_takes_effect_even_when_the_carry_is_full() {
    let mut fixture = direct_scan_fixture(7, "/fixture/full-carry");
    prefill_carry(&mut fixture);
    // The cancel reaches the receiver only after the carry is already full:
    // under the old fold behavior the lane stopped draining once the carry
    // filled, and this cancel starved until the scan ended naturally.
    let (cancel_tx, cancel_rx) = bounded(4);
    cancel_tx
        .send(queued(
            8,
            DirectoryUsageRequest::Cancel(DirectoryScanId::new(7)),
        ))
        .expect("send cancel for the active scan");
    drop(cancel_tx);
    fixture.receiver = cancel_rx;

    let mut observed_cancel = 0usize;
    let mut chunks_left = 5usize;
    let mut provider =
        |spec: &DirectoryScanSpec, control: &DirectoryScanControl, _observed_at_ms: u64| {
            if control.is_cancelled() {
                observed_cancel += 1;
                return Ok(DirectoryUsageSnapshot {
                    scan_id: control.scan_id(),
                    root: spec.root.clone(),
                    status: DirectoryScanStatus::Cancelled,
                    entries: Vec::new(),
                    totals: DirectoryScanTotals::fresh(10),
                });
            }
            chunks_left -= 1;
            let status = if chunks_left == 0 {
                DirectoryScanStatus::Completed
            } else {
                DirectoryScanStatus::Scanning
            };
            Ok(DirectoryUsageSnapshot {
                scan_id: control.scan_id(),
                root: spec.root.clone(),
                status,
                entries: Vec::new(),
                totals: DirectoryScanTotals::fresh(10),
            })
        };
    let (envelopes, carry) = run_direct_scan(fixture, &mut provider, MAX_SCAN_BUDGET);

    let snapshots: Vec<DirectoryUsageSnapshot> =
        envelopes.iter().map(|queued| snapshot_of(queued)).collect();
    assert!(
        snapshots
            .iter()
            .any(|snapshot| snapshot.status == DirectoryScanStatus::Cancelled),
        "the cancelled terminal must be published despite the full carry: {snapshots:?}"
    );
    assert!(
        observed_cancel >= 1,
        "the provider must observe the cancel flag"
    );
    assert_eq!(
        carry.len(),
        MAX_CARRIED_REQUESTS,
        "the carried scans must stay queued"
    );
}

#[test]
fn an_expired_whole_scan_budget_publishes_the_typed_timeout_terminal() {
    let fixture = direct_scan_fixture(9, "/fixture/never-ends");
    let mut provider =
        |spec: &DirectoryScanSpec, control: &DirectoryScanControl, _observed_at_ms: u64| {
            // A pathological walker that never converges: every chunk reports
            // progress. Each chunk sleeps for far longer than the budget, so the
            // deadline provably expires mid-scan after a real chunk ran.
            std::thread::sleep(Duration::from_millis(50));
            Ok(DirectoryUsageSnapshot {
                scan_id: control.scan_id(),
                root: spec.root.clone(),
                status: DirectoryScanStatus::Scanning,
                entries: Vec::new(),
                totals: DirectoryScanTotals::fresh(10),
            })
        };
    let (envelopes, _carry) = run_direct_scan(fixture, &mut provider, Duration::from_millis(5));

    let snapshots: Vec<DirectoryUsageSnapshot> =
        envelopes.iter().map(|queued| snapshot_of(queued)).collect();
    let terminal = snapshots
        .iter()
        .find(|snapshot| snapshot.is_terminal())
        .expect("the expired scan must publish a terminal");
    assert_eq!(
        terminal.status,
        DirectoryScanStatus::Failed(FailureKind::TimedOut)
    );
    assert!(
        terminal.entries.is_empty(),
        "the timeout terminal must not fabricate partial entries"
    );
}

#[test]
fn a_start_scan_overflowing_the_full_carry_is_rejected_with_a_typed_terminal() {
    let mut fixture = direct_scan_fixture(7, "/fixture/overflowing");
    prefill_carry(&mut fixture);
    reserve_scan(&fixture.catalog, 200, "/fixture/overflowed-start");
    let (overflow_tx, overflow_rx) = bounded(4);
    overflow_tx
        .send(queued(
            200,
            DirectoryUsageRequest::StartScan(DirectoryScanSpec {
                root: "/fixture/overflowed-start".to_string(),
                bounds: DirectoryScanBounds::default(),
            }),
        ))
        .expect("send overflow start");
    drop(overflow_tx);
    fixture.receiver = overflow_rx;

    let mut chunks_left = 2usize;
    let mut provider =
        |spec: &DirectoryScanSpec, control: &DirectoryScanControl, _observed_at_ms: u64| {
            chunks_left -= 1;
            let status = if chunks_left == 0 {
                DirectoryScanStatus::Completed
            } else {
                DirectoryScanStatus::Scanning
            };
            Ok(DirectoryUsageSnapshot {
                scan_id: control.scan_id(),
                root: spec.root.clone(),
                status,
                entries: Vec::new(),
                totals: DirectoryScanTotals::fresh(10),
            })
        };
    let (envelopes, carry) = run_direct_scan(fixture, &mut provider, MAX_SCAN_BUDGET);

    let overflow = envelopes
        .iter()
        .find(|envelope| envelope.request_id.get() == 200)
        .expect("the overflowed start must receive a typed terminal");
    assert_eq!(
        snapshot_of(overflow).status,
        DirectoryScanStatus::Failed(FailureKind::Rejected)
    );
    assert!(
        envelopes.iter().any(|envelope| {
            envelope.request_id.get() == 7
                && snapshot_of(envelope).status == DirectoryScanStatus::Completed
        }),
        "the active scan must still complete normally"
    );
    assert_eq!(
        carry.len(),
        MAX_CARRIED_REQUESTS,
        "the overflow must not grow the retained carry"
    );
}
