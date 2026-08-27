use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::bounded;
use taskmanager_application::{
    CapabilityId, CapabilityScheduler, FailureKind, MAX_PROVIDER_PANIC_NOTES, ProviderFailure,
    ProviderId, RequestId,
};

use super::fixture;
use crate::Queued;
use crate::delivery::RuntimeCapabilityCatalog;
use crate::delivery::worker::{PROCESS_WORKER_LIMIT, WorkerQuota, WorkerRuntime, WorkerSpawnError};

struct WorkerExitSignal(crossbeam_channel::Sender<()>);

impl Drop for WorkerExitSignal {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn worker_limits_fail_closed_across_runtime_owners_and_release_after_exit() {
    let quota = Arc::new(WorkerQuota::new(1));
    let first = WorkerRuntime::with_quota(1, quota.clone());
    let (publisher, _, _, _) = fixture();
    let (request_tx, request_rx) = bounded::<Queued<u8>>(1);
    let (exit_tx, exit_rx) = bounded(1);
    crate::spawn_lane(&first, request_rx, publisher.clone(), {
        let _exit = WorkerExitSignal(exit_tx);
        move |_| {
            let _ = &_exit;
            Err(ProviderFailure::Unsupported)
        }
    })
    .expect("first worker starts");

    let (same_tx, same_rx) = bounded::<Queued<u16>>(1);
    let same_owner_error = crate::spawn_lane(&first, same_rx, publisher.clone(), |_| {
        Err(ProviderFailure::Unsupported)
    })
    .expect_err("one runtime cannot exceed its own worker bound");
    assert!(matches!(
        same_owner_error,
        WorkerSpawnError::Capacity { limit: 1, .. }
    ));

    let second = WorkerRuntime::with_quota(2, quota.clone());
    let (other_tx, other_rx) = bounded::<Queued<u32>>(1);
    let process_error = crate::spawn_lane(&second, other_rx, publisher.clone(), |_| {
        Err(ProviderFailure::Unsupported)
    })
    .expect_err("a second owner cannot bypass the shared process quota");
    assert!(matches!(
        process_error,
        WorkerSpawnError::ProcessCapacity { limit: 1, .. }
    ));

    drop(first);
    exit_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("idle worker observes cooperative shutdown");

    let mut replacement = None;
    for _ in 0..1_024 {
        let (_replacement_tx, replacement_rx) = bounded::<Queued<u64>>(1);
        match crate::spawn_lane(&second, replacement_rx, publisher.clone(), |_| {
            Err(ProviderFailure::Unsupported)
        }) {
            Ok(()) => {
                replacement = Some(());
                break;
            }
            Err(WorkerSpawnError::ProcessCapacity { .. }) => thread::yield_now(),
            Err(error) => panic!("unexpected replacement startup error: {error}"),
        }
    }
    assert_eq!(replacement, Some(()), "exited worker returns its quota");

    drop(request_tx);
    drop(same_tx);
    drop(other_tx);
}

#[test]
fn dropping_runtime_never_waits_for_a_blocked_provider_and_keeps_its_quota() {
    let quota = Arc::new(WorkerQuota::new(1));
    let workers = WorkerRuntime::with_quota(1, quota.clone());
    let (publisher, _, observation_rx, _) = fixture();
    let (request_tx, request_rx) = bounded(1);
    let (entered_tx, entered_rx) = bounded(1);
    let (release_tx, release_rx) = bounded(1);
    let (provider_done_tx, provider_done_rx) = bounded(1);
    crate::spawn_lane(&workers, request_rx, publisher.clone(), move |_: u8| {
        entered_tx
            .send(())
            .map_err(|_| ProviderFailure::ProviderFault)?;
        release_rx
            .recv()
            .map_err(|_| ProviderFailure::TemporarilyUnavailable)?;
        provider_done_tx
            .send(())
            .map_err(|_| ProviderFailure::ProviderFault)?;
        Err(ProviderFailure::Unsupported)
    })
    .expect("blocking worker starts");
    request_tx
        .send(Queued {
            request_id: RequestId::new(70).expect("fixture id"),
            capability: CapabilityId::TELEMETRY_CPU,
            provider: ProviderId::borrowed("fixture.blocking"),
            payload: 1_u8,
        })
        .expect("blocking request queued");
    entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("provider entered its blocking call");

    let (drop_done_tx, drop_done_rx) = bounded(1);
    let dropper = thread::spawn(move || {
        drop(workers);
        let _ = drop_done_tx.send(());
    });
    drop_done_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("runtime drop must not join a blocked provider");

    let replacement = WorkerRuntime::with_quota(1, quota);
    let (_replacement_tx, replacement_rx) = bounded::<Queued<u16>>(1);
    let error = crate::spawn_lane(&replacement, replacement_rx, publisher.clone(), |_| {
        Err(ProviderFailure::Unsupported)
    })
    .expect_err("detached blocked worker must retain its process permit");
    assert!(matches!(
        error,
        WorkerSpawnError::ProcessCapacity { limit: 1, .. }
    ));

    release_tx.send(()).expect("release blocked provider");
    provider_done_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("provider returns after release");
    dropper.join().expect("drop observer exits");

    let mut restarted = false;
    for _ in 0..1_024 {
        let (_tx, rx) = bounded::<Queued<u32>>(1);
        match crate::spawn_lane(&replacement, rx, publisher.clone(), |_| {
            Err(ProviderFailure::Unsupported)
        }) {
            Ok(()) => {
                restarted = true;
                break;
            }
            Err(WorkerSpawnError::ProcessCapacity { .. }) => thread::yield_now(),
            Err(error) => panic!("unexpected restart error: {error}"),
        }
    }
    assert!(
        restarted,
        "provider exit returns the retained process permit"
    );
    assert!(
        observation_rx.try_recv().is_err(),
        "a provider returning after shutdown cannot publish a late result"
    );
}

fn reserve(
    catalog: &RuntimeCapabilityCatalog,
    capability: &CapabilityId,
    request: RequestId,
) -> Result<(), crate::ecs::EcsAdmissionError> {
    catalog
        .ecs_scheduler_handle()
        .lock()
        .expect("scheduler lock")
        .admit_submission_with_tracking(
            capability,
            request,
            0,
            taskmanager_application::RequestTracking::Capability,
        )
}

#[test]
fn spawn_reclaims_a_dead_lane_before_the_capacity_check() {
    let workers = WorkerRuntime::with_quota(1, Arc::new(WorkerQuota::new(PROCESS_WORKER_LIMIT)));

    workers
        .spawn("short-lived".into(), |_| {})
        .expect("first lane starts under the runtime ceiling");

    // The lane thread exits by itself, but its JoinHandle lingers until the
    // next spawn reclaims it. Before reclaim-on-spawn this loop could only
    // end in `Capacity`, so a dead lane permanently pinned the ceiling.
    let mut replacement = None;
    for _ in 0..10_000 {
        match workers.spawn("replacement".into(), |_| {}) {
            Ok(()) => {
                replacement = Some(());
                break;
            }
            Err(WorkerSpawnError::Capacity { .. }) => thread::yield_now(),
            Err(error) => panic!("unexpected replacement startup error: {error}"),
        }
    }
    assert!(
        replacement.is_some(),
        "a dead lane handle must not pin the runtime worker ceiling"
    );
}

#[test]
fn panicking_provider_degrades_to_typed_fault_with_bounded_panic_notes() {
    // Silence only the intentional provider panics on `taskforest-*` lane
    // threads; every other panic (including assertion failures on the test
    // thread) keeps its default reporting.
    std::panic::set_hook(Box::new(|info| {
        if !std::thread::current()
            .name()
            .is_some_and(|name| name.starts_with("taskforest-"))
        {
            eprintln!("panicked: {info}");
        }
    }));
    let (publisher, _control_rx, observation_rx, catalog) = fixture();
    let capability = CapabilityId::TELEMETRY_CPU;
    let workers = WorkerRuntime::with_quota(1, Arc::new(WorkerQuota::new(PROCESS_WORKER_LIMIT)));
    let (request_tx, request_rx) = bounded::<Queued<u8>>(1);
    crate::spawn_lane(&workers, request_rx, publisher, |payload: u8| {
        if payload == 0 {
            std::panic::panic_any(7_u32);
        }
        panic!("provider exploded: {payload}");
    })
    .expect("panicking lane starts");

    for nth in 1_u64..=10 {
        let request = RequestId::new(90 + nth).expect("fixture id");
        // The previous request's owner retires only when its terminal health
        // record lands (claim -> enqueue -> record), so admission may briefly
        // still see `CapabilityInFlight`; retry until the owner is released
        // instead of assuming a fixed interleaving.
        let mut admitted = false;
        for _ in 0..10_000 {
            if reserve(&catalog, &capability, request).is_ok() {
                admitted = true;
                break;
            }
            thread::yield_now();
        }
        assert!(admitted, "panicked terminal {nth} must retire its owner");
        request_tx
            .send(Queued {
                request_id: request,
                capability: capability.clone(),
                provider: ProviderId::borrowed("fixture.telemetry.cpu"),
                payload: u8::try_from(nth - 1).expect("fixture payload"),
            })
            .expect("panicking request queued");
        // The panic note is recorded before publication, so also wait for the
        // terminal delivery before reserving the next request: one capability
        // owns at most one in-flight request at a time.
        let mut published = false;
        for _ in 0..10_000 {
            let snapshot = CapabilityScheduler::scheduling_snapshot(catalog.as_ref());
            let delivered = snapshot.event_queues.observation_pending
                + snapshot.event_queues.terminal_mailbox_pending;
            if snapshot.provider_panics == nth && delivered >= nth {
                published = true;
                break;
            }
            thread::yield_now();
        }
        assert!(
            published,
            "panicking request {nth} still degrades to one terminal"
        );

        if nth == 1 {
            let snapshot = CapabilityScheduler::scheduling_snapshot(catalog.as_ref());
            assert_eq!(snapshot.recent_provider_panics.len(), 1);
            let note = &snapshot.recent_provider_panics[0];
            assert_eq!(note.lane, "u8");
            assert_eq!(note.capability, capability);
            assert_eq!(note.request_id, request);
            assert_eq!(note.message, "(non-string panic payload)");
            assert_eq!(note.sequence, 1);
        }
    }

    let first = observation_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("panicking provider still publishes one terminal");
    assert_eq!(first.request_id, RequestId::new(91).expect("fixture id"));
    match &first.outcome {
        Err(failure) => {
            assert_eq!(failure.kind, FailureKind::ProviderFault);
            assert_eq!(failure.request_id, RequestId::new(91).expect("fixture id"));
        }
        Ok(_) => panic!("a panicking provider must degrade to a typed ProviderFault"),
    }

    let snapshot = CapabilityScheduler::scheduling_snapshot(catalog.as_ref());
    assert_eq!(snapshot.provider_panics, 10);
    assert_eq!(
        snapshot.recent_provider_panics.len(),
        MAX_PROVIDER_PANIC_NOTES,
        "the panic ring keeps only the bounded recent tail"
    );
    let sequences: Vec<u64> = snapshot
        .recent_provider_panics
        .iter()
        .map(|note| note.sequence)
        .collect();
    assert_eq!(sequences, (3..=10).collect::<Vec<u64>>());
    for note in &snapshot.recent_provider_panics {
        assert_eq!(note.lane, "u8");
        assert_eq!(note.capability, capability);
        assert_eq!(
            note.request_id,
            RequestId::new(90 + note.sequence).expect("fixture id")
        );
        assert_eq!(
            note.message,
            format!("provider exploded: {}", note.sequence - 1)
        );
    }
}
