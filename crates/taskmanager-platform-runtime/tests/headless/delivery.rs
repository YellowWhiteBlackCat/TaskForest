use std::cell::Cell;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, bounded};
use taskmanager_application::CapabilityScheduler;
use taskmanager_application::{
    CapabilityCatalog, CapabilityId, CapabilityStatus, CpuMetrics, CpuTelemetryObservation,
    EventPort, EventSequence, FailureKind, PartialSourceSnapshot, PlatformEvent, ProcessEvent,
    ProviderFailure, ProviderId, RequestId, RequestScope, RequestTracking, SidebandPolicy,
    SourceOutcome, SourceStatus, SystemTelemetryDomainEvent, SystemTelemetryRevision,
};
use taskmanager_core::{
    DirectoryScanId, DirectoryScanStatus, DirectoryScanTotals, DirectoryUsageSnapshot,
    FrozenProcessIdentity,
};

use super::{FairEventPort, LaneFlow, RuntimeCapabilityCatalog, RuntimeEventPublisher};
use crate::Queued;
use crate::config::{CapabilityRoute, DeliveryClass};
use crate::health::CapabilityHealth;

#[path = "delivery/worker_lifecycle.rs"]
mod worker_lifecycle;

fn fixed_clock() -> u64 {
    42
}

thread_local! {
    static MANUAL_MONOTONIC_MS: Cell<u64> = const { Cell::new(0) };
}

fn manual_monotonic_clock() -> u64 {
    MANUAL_MONOTONIC_MS.get()
}

fn set_manual_monotonic_clock(now_ms: u64) {
    MANUAL_MONOTONIC_MS.set(now_ms);
}

const fn expired_monotonic_clock() -> u64 {
    crate::ecs::DEFAULT_IN_FLIGHT_LEASE_MS
}

fn frozen_process() -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(42, "fixture", 7, 700)
        .expect("fixture identity")
}

type DeliveryFixture = (
    Arc<RuntimeEventPublisher>,
    Receiver<super::event_queue::QueuedEvent>,
    Receiver<super::event_queue::QueuedEvent>,
    Arc<RuntimeCapabilityCatalog>,
);

fn fixture() -> DeliveryFixture {
    let routes = [
        CapabilityRoute {
            capability: CapabilityId::TELEMETRY_CPU,
            provider: ProviderId::borrowed("fixture.telemetry.cpu"),
            delivery: DeliveryClass::Observation,
            domain: crate::config::RuntimeDomain::System,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        },
        CapabilityRoute {
            capability: CapabilityId::PROCESS_CONTROL,
            provider: ProviderId::borrowed("fixture.process-control"),
            delivery: DeliveryClass::Control,
            domain: crate::config::RuntimeDomain::Process,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        },
    ];
    let catalog = Arc::new(RuntimeCapabilityCatalog::new(&routes, fixed_clock));
    let (control_tx, control_rx) = bounded(3);
    let (observation_tx, observation_rx) = bounded(1);
    let publisher = Arc::new(RuntimeEventPublisher::new(
        control_tx,
        observation_tx,
        catalog.clone(),
        vec![CapabilityId::PROCESS_CONTROL],
        fixed_clock,
    ));
    (publisher, control_rx, observation_rx, catalog)
}

fn send_fixture(
    publisher: &RuntimeEventPublisher,
    request_id: RequestId,
    capability: CapabilityId,
) -> bool {
    let provider = if capability == CapabilityId::PROCESS_CONTROL {
        ProviderId::borrowed("fixture.process-control")
    } else {
        ProviderId::borrowed("fixture.telemetry.cpu")
    };
    publisher
        .send(
            request_id,
            capability.clone(),
            provider,
            fixed_clock(),
            Err(ProviderFailure::ProviderFault),
        )
        .is_delivered()
}

fn reserve_fixture_owner(
    catalog: &RuntimeCapabilityCatalog,
    capability: &CapabilityId,
    request_id: RequestId,
) {
    assert!(
        catalog
            .ecs_scheduler_handle()
            .lock()
            .expect("scheduler lock")
            .reserve_submission(capability, request_id, 0),
        "fixture publication must first own the ECS lifecycle"
    );
}

#[test]
fn full_nonterminal_observation_coalesces_without_blocking_control() {
    let (publisher, control_rx, observation_rx, _) = fixture();
    assert!(send_fixture(
        &publisher,
        RequestId::new(1).expect("fixture id"),
        CapabilityId::TELEMETRY_CPU,
    ));

    let blocked = Arc::new(Barrier::new(2));
    let observation_publisher = publisher.clone();
    let observation_started = blocked.clone();
    let observation_thread = thread::spawn(move || {
        observation_started.wait();
        send_fixture(
            &observation_publisher,
            RequestId::new(2).expect("fixture id"),
            CapabilityId::TELEMETRY_CPU,
        )
    });
    blocked.wait();

    let (completed_tx, completed_rx) = bounded(1);
    let control_publisher = publisher.clone();
    let control_thread = thread::spawn(move || {
        let sent = send_fixture(
            &control_publisher,
            RequestId::new(3).expect("fixture id"),
            CapabilityId::PROCESS_CONTROL,
        );
        let _ = completed_tx.send(sent);
    });
    assert_eq!(
        completed_rx.recv_timeout(Duration::from_millis(100)),
        Ok(true),
        "a full observation queue must not block control delivery"
    );
    assert_eq!(
        control_rx
            .recv_timeout(Duration::from_millis(100))
            .map(|event| event.request_id),
        Ok(RequestId::new(3).expect("fixture id"))
    );

    // The enqueue path is non-blocking, so joining before the drain proves
    // the full queue alone cannot stall the lane; draining afterwards pins
    // which event was retained regardless of scheduling order.
    assert!(!observation_thread.join().expect("observation publisher"));
    assert_eq!(
        observation_rx
            .recv_timeout(Duration::from_millis(100))
            .map(|event| event.request_id),
        Ok(RequestId::new(1).expect("fixture id")),
        "the queued observation stays delivered while the overflow coalesces"
    );
    control_thread.join().expect("control publisher");
}

#[test]
fn event_port_prioritizes_control_without_dropping_observations() {
    let (publisher, control_rx, observation_rx, catalog) = fixture();
    let observation_id = RequestId::new(10).expect("fixture id");
    let control_id = RequestId::new(11).expect("fixture id");
    assert!(send_fixture(
        &publisher,
        observation_id,
        CapabilityId::TELEMETRY_CPU
    ));
    assert!(send_fixture(
        &publisher,
        control_id,
        CapabilityId::PROCESS_CONTROL
    ));

    let port = FairEventPort::new(
        control_rx,
        observation_rx,
        catalog.event_queue_state(),
        catalog,
    );
    assert_eq!(
        port.try_recv()
            .expect("event port")
            .map(|event| event.request_id),
        Some(control_id)
    );
    assert_eq!(
        port.try_recv()
            .expect("event port")
            .map(|event| event.request_id),
        Some(observation_id)
    );
}

#[test]
fn event_port_priority_is_fair_under_sustained_control_load() {
    let (publisher, control_rx, observation_rx, catalog) = fixture();
    let first_control = RequestId::new(20).expect("fixture id");
    let second_control = RequestId::new(21).expect("fixture id");
    let observation = RequestId::new(22).expect("fixture id");
    assert!(send_fixture(
        &publisher,
        first_control,
        CapabilityId::PROCESS_CONTROL
    ));
    assert!(send_fixture(
        &publisher,
        second_control,
        CapabilityId::PROCESS_CONTROL
    ));
    assert!(send_fixture(
        &publisher,
        observation,
        CapabilityId::TELEMETRY_CPU
    ));

    let port = FairEventPort::new(
        control_rx,
        observation_rx,
        catalog.event_queue_state(),
        catalog,
    );
    let received = (0..3)
        .map(|_| {
            port.try_recv()
                .expect("event port")
                .expect("queued event")
                .request_id
        })
        .collect::<Vec<_>>();

    assert_eq!(
        received,
        [first_control, observation, second_control],
        "control keeps first-delivery priority without starving observations"
    );
}

#[test]
fn capability_health_and_correlation_update_from_one_publication() {
    let (publisher, control_rx, _, catalog) = fixture();
    let request_id = RequestId::new(30).expect("fixture id");
    reserve_fixture_owner(&catalog, &CapabilityId::PROCESS_CONTROL, request_id);
    assert_eq!(
        publisher.publish(
            request_id,
            CapabilityId::PROCESS_CONTROL,
            ProviderId::borrowed("fixture.process-control"),
            Ok(PlatformEvent::Processes(ProcessEvent::EndTaskCompleted(
                frozen_process(),
            ))),
        ),
        LaneFlow::Continue
    );

    let event = control_rx.try_recv().expect("published event");
    assert_eq!(event.request_id, request_id);
    assert_eq!(event.observed_at_ms, fixed_clock());
    assert_eq!(event.sequence, EventSequence::new(1));
    let descriptor = catalog
        .snapshot()
        .get(&CapabilityId::PROCESS_CONTROL)
        .cloned()
        .expect("process control capability");
    assert_eq!(descriptor.status, CapabilityStatus::Available);
    assert_eq!(descriptor.last_success_at_ms, Some(fixed_clock()));
}

#[test]
fn disconnected_event_lane_does_not_advance_catalog_or_ecs_lifecycle() {
    let routes = [CapabilityRoute {
        capability: CapabilityId::PROCESS_CONTROL,
        provider: ProviderId::borrowed("fixture.process-control"),
        delivery: DeliveryClass::Control,
        domain: crate::config::RuntimeDomain::Process,
        cadence_ms: Some(1_000),
        sideband_policy: SidebandPolicy::Denied,
    }];
    let catalog = Arc::new(RuntimeCapabilityCatalog::new(&routes, fixed_clock));
    let (control_tx, control_rx) = bounded(1);
    let (observation_tx, observation_rx) = bounded(1);
    drop(control_rx);
    drop(observation_rx);
    let publisher = RuntimeEventPublisher::new(
        control_tx,
        observation_tx,
        catalog.clone(),
        vec![CapabilityId::PROCESS_CONTROL],
        fixed_clock,
    );

    let request_id = RequestId::new(34).expect("fixture id");
    reserve_fixture_owner(&catalog, &CapabilityId::PROCESS_CONTROL, request_id);
    assert!(
        publisher
            .publish(
                request_id,
                CapabilityId::PROCESS_CONTROL,
                ProviderId::borrowed("fixture.process-control"),
                Ok(PlatformEvent::Processes(ProcessEvent::EndTaskCompleted(
                    frozen_process(),
                ))),
            )
            .is_stop()
    );
    let descriptor = catalog
        .snapshot()
        .get(&CapabilityId::PROCESS_CONTROL)
        .cloned()
        .expect("process control capability");
    assert_eq!(descriptor.status, CapabilityStatus::TemporarilyUnavailable);
    assert_eq!(descriptor.last_success_at_ms, None);
}

#[test]
fn expired_ecs_lease_is_visible_through_the_capability_catalog() {
    let routes = [CapabilityRoute {
        capability: CapabilityId::TELEMETRY_CPU,
        provider: ProviderId::borrowed("fixture.telemetry.cpu"),
        delivery: DeliveryClass::Observation,
        domain: crate::config::RuntimeDomain::System,
        cadence_ms: Some(1_000),
        sideband_policy: SidebandPolicy::Denied,
    }];
    let catalog = RuntimeCapabilityCatalog::new(&routes, expired_monotonic_clock);
    let request = RequestId::new(35).expect("fixture id");
    assert!(
        catalog
            .ecs_scheduler_handle()
            .lock()
            .expect("scheduler lock")
            .reserve_submission(&CapabilityId::TELEMETRY_CPU, request, 0)
    );
    let wall_observed_at_ms = 7;
    let _ = CapabilityScheduler::poll_due(&catalog, wall_observed_at_ms);
    let descriptor = catalog
        .snapshot()
        .get(&CapabilityId::TELEMETRY_CPU)
        .cloned()
        .expect("CPU capability");
    assert_eq!(descriptor.status, CapabilityStatus::TemporarilyUnavailable);
    assert_eq!(descriptor.observed_at_ms, wall_observed_at_ms,);
    assert_eq!(descriptor.last_success_at_ms, None);
}

#[test]
fn wall_clock_jumps_cannot_expire_an_ecs_lease() {
    set_manual_monotonic_clock(0);
    let routes = [CapabilityRoute {
        capability: CapabilityId::TELEMETRY_CPU,
        provider: ProviderId::borrowed("fixture.telemetry.cpu"),
        delivery: DeliveryClass::Observation,
        domain: crate::config::RuntimeDomain::System,
        cadence_ms: Some(1_000),
        sideband_policy: SidebandPolicy::Denied,
    }];
    let catalog = RuntimeCapabilityCatalog::new(&routes, manual_monotonic_clock);
    let scheduler = catalog.ecs_scheduler_handle();
    assert!(
        scheduler
            .lock()
            .expect("scheduler lock")
            .reserve_submission(
                &CapabilityId::TELEMETRY_CPU,
                RequestId::new(36).expect("fixture id"),
                0,
            )
    );

    let _ = CapabilityScheduler::poll_due(&catalog, u64::MAX);
    let _ = CapabilityScheduler::poll_due(&catalog, 1);

    assert_eq!(
        scheduler
            .lock()
            .expect("scheduler lock")
            .diagnostics()
            .stalled_count(),
        0,
        "neither a wall-clock forward jump nor rollback may advance a lease"
    );

    set_manual_monotonic_clock(crate::ecs::DEFAULT_IN_FLIGHT_LEASE_MS);
    let _ = CapabilityScheduler::poll_due(&catalog, 2);
    assert_eq!(
        scheduler
            .lock()
            .expect("scheduler lock")
            .diagnostics()
            .stalled_count(),
        1,
        "the monotonic deadline still expires the lease after wall-clock rollback"
    );
}

#[test]
fn successfully_published_progress_renews_the_target_lease_from_monotonic_time() {
    set_manual_monotonic_clock(0);
    let routes = [CapabilityRoute {
        capability: CapabilityId::DIRECTORY_USAGE,
        provider: ProviderId::borrowed("fixture.directory"),
        delivery: DeliveryClass::Observation,
        domain: crate::config::RuntimeDomain::Storage,
        cadence_ms: None,
        sideband_policy: SidebandPolicy::Idempotent,
    }];
    let catalog = Arc::new(RuntimeCapabilityCatalog::new(
        &routes,
        manual_monotonic_clock,
    ));
    let scheduler = catalog.ecs_scheduler_handle();
    let request = RequestId::new(39).expect("fixture id");
    assert!(
        scheduler
            .lock()
            .expect("scheduler lock")
            .reserve_submission_with_tracking(
                &CapabilityId::DIRECTORY_USAGE,
                request,
                0,
                RequestTracking::Target(
                    RequestScope::try_from_str("/fixture/long-running")
                        .expect("bounded target fixture"),
                ),
            )
    );
    let (control_tx, _control_rx) = bounded(1);
    let (observation_tx, observation_rx) = bounded(1);
    let publisher =
        RuntimeEventPublisher::new(control_tx, observation_tx, catalog, Vec::new(), fixed_clock);

    set_manual_monotonic_clock(crate::ecs::DEFAULT_IN_FLIGHT_LEASE_MS - 1);
    assert_eq!(
        publisher.publish_progress(
            request,
            CapabilityId::DIRECTORY_USAGE,
            ProviderId::borrowed("fixture.directory"),
            PlatformEvent::DirectoryUsage(taskmanager_application::DirectoryUsageEvent::Update(
                DirectoryUsageSnapshot {
                    scan_id: DirectoryScanId::new(request.get()),
                    root: "/fixture/long-running".into(),
                    status: DirectoryScanStatus::Scanning,
                    entries: Vec::new(),
                    totals: DirectoryScanTotals::fresh(1),
                },
            )),
        ),
        LaneFlow::Continue
    );
    let _published = observation_rx.try_recv().expect("published progress");

    assert!(
        scheduler
            .lock()
            .expect("scheduler lock")
            .tick_plan(crate::ecs::DEFAULT_IN_FLIGHT_LEASE_MS)
            .stalled
            .is_empty(),
        "the original lease deadline must no longer stall active progress"
    );
    let renewed_deadline = crate::ecs::DEFAULT_IN_FLIGHT_LEASE_MS
        .saturating_mul(2)
        .saturating_sub(1);
    let stalled = scheduler
        .lock()
        .expect("scheduler lock")
        .tick_plan(renewed_deadline);
    assert_eq!(stalled.stalled.len(), 1, "the renewed lease still expires");
}

#[test]
fn cadence_and_retry_follow_only_the_monotonic_clock() {
    set_manual_monotonic_clock(100);
    let routes = [
        CapabilityRoute {
            capability: CapabilityId::TELEMETRY_CPU,
            provider: ProviderId::borrowed("fixture.telemetry.cpu"),
            delivery: DeliveryClass::Observation,
            domain: crate::config::RuntimeDomain::System,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        },
        CapabilityRoute {
            capability: CapabilityId::PROCESS_CONTROL,
            provider: ProviderId::borrowed("fixture.process-control"),
            delivery: DeliveryClass::Control,
            domain: crate::config::RuntimeDomain::Process,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        },
    ];
    let catalog = RuntimeCapabilityCatalog::new(&routes, manual_monotonic_clock);
    let scheduler = catalog.ecs_scheduler_handle();
    {
        let mut scheduler = scheduler.lock().expect("scheduler lock");
        assert!(scheduler.reserve_submission(
            &CapabilityId::TELEMETRY_CPU,
            RequestId::new(37).expect("fixture id"),
            100,
        ));
        assert!(scheduler.reserve_submission(
            &CapabilityId::PROCESS_CONTROL,
            RequestId::new(38).expect("fixture id"),
            100,
        ));
        assert!(
            scheduler
                .claim_terminal_delivery(
                    &CapabilityId::TELEMETRY_CPU,
                    RequestId::new(37).expect("fixture id"),
                )
                .is_accepted()
        );
        assert!(
            scheduler
                .claim_terminal_delivery(
                    &CapabilityId::PROCESS_CONTROL,
                    RequestId::new(38).expect("fixture id"),
                )
                .is_accepted()
        );
    }
    assert!(
        catalog
            .record(
                &CapabilityId::TELEMETRY_CPU,
                CapabilityHealth::Available,
                u64::MAX,
                RequestId::new(37).expect("fixture id"),
            )
            .is_accepted()
    );
    assert!(
        catalog
            .record(
                &CapabilityId::PROCESS_CONTROL,
                CapabilityHealth::Unavailable(ProviderFailure::TemporarilyUnavailable),
                u64::MAX,
                RequestId::new(38).expect("fixture id"),
            )
            .is_accepted()
    );

    set_manual_monotonic_clock(1_099);
    assert!(CapabilityScheduler::poll_due(&catalog, 0).is_empty());
    set_manual_monotonic_clock(1_100);
    assert_eq!(
        CapabilityScheduler::poll_due(&catalog, 0),
        vec![CapabilityId::PROCESS_CONTROL, CapabilityId::TELEMETRY_CPU],
        "wall time cannot delay cadence or retry; the monotonic deadline releases both"
    );
}

#[test]
fn duplicate_catalog_routes_keep_one_descriptor_and_first_provider_authority() {
    let routes = [
        CapabilityRoute {
            capability: CapabilityId::TELEMETRY_CPU,
            provider: ProviderId::borrowed("fixture.first"),
            delivery: DeliveryClass::Observation,
            domain: crate::config::RuntimeDomain::System,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        },
        CapabilityRoute {
            capability: CapabilityId::TELEMETRY_CPU,
            provider: ProviderId::borrowed("fixture.duplicate"),
            delivery: DeliveryClass::Control,
            domain: crate::config::RuntimeDomain::Integration,
            cadence_ms: None,
            sideband_policy: SidebandPolicy::Denied,
        },
    ];
    let catalog = RuntimeCapabilityCatalog::new(&routes, fixed_clock);
    let snapshot = catalog.snapshot();
    assert_eq!(snapshot.iter().count(), 1);
    assert_eq!(
        snapshot
            .get(&CapabilityId::TELEMETRY_CPU)
            .map(|descriptor| descriptor.providers.clone()),
        Some(vec![ProviderId::borrowed("fixture.first")])
    );
}

#[test]
fn mismatched_success_payloads_fail_closed_before_health_publication() {
    let (publisher, control_rx, _, catalog) = fixture();
    let provider = ProviderId::borrowed("fixture.process-control");
    let mismatched = || PlatformEvent::Shell(taskmanager_application::ShellEvent::TargetOpened);
    let first = RequestId::new(40).expect("fixture id");
    reserve_fixture_owner(&catalog, &CapabilityId::PROCESS_CONTROL, first);
    assert_eq!(
        publisher.publish(
            first,
            CapabilityId::PROCESS_CONTROL,
            provider.clone(),
            Ok(mismatched()),
        ),
        LaneFlow::Continue
    );
    assert_eq!(
        control_rx
            .try_recv()
            .expect("mismatched publication")
            .envelope
            .outcome
            .expect_err("mismatched payload must fail")
            .kind,
        FailureKind::ProviderFault
    );

    let second = RequestId::new(41).expect("fixture id");
    reserve_fixture_owner(&catalog, &CapabilityId::PROCESS_CONTROL, second);
    assert_eq!(
        publisher.publish_typed_outcome(
            second,
            CapabilityId::PROCESS_CONTROL,
            provider.clone(),
            mismatched(),
            Ok(()),
        ),
        LaneFlow::Continue
    );
    assert_eq!(
        control_rx
            .try_recv()
            .expect("mismatched typed publication")
            .envelope
            .outcome
            .expect_err("mismatched typed payload must fail")
            .kind,
        FailureKind::ProviderFault
    );

    let third = RequestId::new(42).expect("fixture id");
    reserve_fixture_owner(&catalog, &CapabilityId::PROCESS_CONTROL, third);
    assert_eq!(
        publisher.publish_health(
            third,
            CapabilityId::PROCESS_CONTROL,
            provider,
            mismatched(),
            CapabilityHealth::Available,
        ),
        LaneFlow::Continue
    );
    assert_eq!(
        control_rx
            .try_recv()
            .expect("mismatched health publication")
            .envelope
            .outcome
            .expect_err("mismatched health payload must fail")
            .kind,
        FailureKind::ProviderFault
    );

    let descriptor = catalog
        .snapshot()
        .get(&CapabilityId::PROCESS_CONTROL)
        .cloned()
        .expect("process control capability");
    assert_eq!(descriptor.status, CapabilityStatus::Stale);
    assert_eq!(descriptor.last_success_at_ms, None);
}

#[test]
fn partial_observation_is_delivered_and_catalogued_as_degraded() {
    let (publisher, _, observation_rx, catalog) = fixture();
    let request_id = RequestId::new(32).expect("fixture id");
    reserve_fixture_owner(&catalog, &CapabilityId::TELEMETRY_CPU, request_id);
    assert_eq!(
        publisher.publish_health(
            request_id,
            CapabilityId::TELEMETRY_CPU,
            ProviderId::borrowed("fixture.telemetry.cpu"),
            PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Cpu {
                revision: SystemTelemetryRevision::new(1),
                observation: Box::new(CpuTelemetryObservation::current(
                    CpuMetrics::default(),
                    fixed_clock(),
                    Vec::new(),
                )),
            }),
            CapabilityHealth::Degraded(taskmanager_application::FailureKind::PermissionDenied),
        ),
        LaneFlow::Continue
    );

    let event = observation_rx.try_recv().expect("published event");
    assert!(event.outcome.is_ok());
    let descriptor = catalog
        .snapshot()
        .get(&CapabilityId::TELEMETRY_CPU)
        .cloned()
        .expect("telemetry capability");
    assert_eq!(
        descriptor.status,
        CapabilityStatus::Degraded(taskmanager_application::FailureKind::PermissionDenied)
    );
    assert_eq!(descriptor.last_success_at_ms, Some(fixed_clock()));
}

#[test]
fn typed_observation_worker_derives_health_before_mapping_the_event() {
    let (publisher, _, observation_rx, catalog) = fixture();
    let (request_tx, request_rx) = bounded(1);
    let workers = crate::WorkerRuntime::default();
    super::spawn_observation_lane(
        &workers,
        request_rx,
        publisher,
        |()| {
            Ok(PartialSourceSnapshot::new(
                Vec::<()>::new(),
                vec![SourceStatus {
                    provider: ProviderId::borrowed("fixture.source"),
                    outcome: SourceOutcome::Unavailable(FailureKind::PermissionDenied),
                    item_count: 0,
                }],
            ))
        },
        |_| {
            PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Cpu {
                revision: SystemTelemetryRevision::new(1),
                observation: Box::new(CpuTelemetryObservation::current(
                    CpuMetrics::default(),
                    fixed_clock(),
                    Vec::new(),
                )),
            })
        },
    )
    .expect("observation worker starts");
    let request_id = RequestId::new(33).expect("fixture id");
    reserve_fixture_owner(&catalog, &CapabilityId::TELEMETRY_CPU, request_id);
    request_tx
        .send(Queued {
            request_id,
            capability: CapabilityId::TELEMETRY_CPU,
            provider: ProviderId::borrowed("fixture.telemetry.cpu"),
            payload: (),
        })
        .expect("typed observation request");

    let event = observation_rx
        .recv_timeout(Duration::from_millis(100))
        .expect("mapped observation event");
    assert!(
        event.outcome.is_ok(),
        "typed unavailable snapshot is retained"
    );
    assert_eq!(
        catalog
            .snapshot()
            .get(&CapabilityId::TELEMETRY_CPU)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::PermissionRequired),
        "terminal visibility includes its capability-health commit"
    );
}

#[test]
fn failed_publication_retains_the_envelope_sequence_after_detachment() {
    let (publisher, _, observation_rx, catalog) = fixture();
    let request_id = RequestId::new(31).expect("fixture id");
    reserve_fixture_owner(&catalog, &CapabilityId::TELEMETRY_CPU, request_id);
    assert_eq!(
        publisher.publish(
            request_id,
            CapabilityId::TELEMETRY_CPU,
            ProviderId::borrowed("fixture.telemetry.cpu"),
            Err(ProviderFailure::TimedOut),
        ),
        LaneFlow::Continue
    );

    let envelope = observation_rx.try_recv().expect("published failure");
    let sequence = envelope.sequence;
    let failure = envelope
        .envelope
        .outcome
        .expect_err("provider result must stay failed");
    assert_eq!(sequence, EventSequence::new(1));
    assert_eq!(failure.sequence, sequence);
    assert_eq!(failure.request_id, request_id);
    assert_eq!(failure.kind, taskmanager_application::FailureKind::TimedOut);
}

#[test]
fn lane_panic_is_isolated_and_publishes_a_typed_failure() {
    let (publisher, _control_rx, observation_rx, catalog) = fixture();
    let (request_tx, request_rx) = bounded(2);
    let workers = crate::WorkerRuntime::default();
    super::spawn_lane(
        &workers,
        request_rx,
        publisher,
        |payload: u8| match payload {
            0 => panic!("boom: provider panicked inside execute"),
            _ => Ok(PlatformEvent::SystemTelemetry(
                SystemTelemetryDomainEvent::Cpu {
                    revision: SystemTelemetryRevision::new(1),
                    observation: Box::new(CpuTelemetryObservation::current(
                        CpuMetrics::default(),
                        fixed_clock(),
                        Vec::new(),
                    )),
                },
            )),
        },
    )
    .expect("provider worker starts");

    let panic_id = RequestId::new(50).expect("fixture id");
    let ok_id = RequestId::new(51).expect("fixture id");
    let provider = ProviderId::borrowed("fixture.telemetry.cpu");
    reserve_fixture_owner(&catalog, &CapabilityId::TELEMETRY_CPU, panic_id);
    request_tx
        .send(Queued {
            request_id: panic_id,
            capability: CapabilityId::TELEMETRY_CPU,
            provider: provider.clone(),
            payload: 0,
        })
        .expect("panic request queued");
    let panic_event = observation_rx
        .recv_timeout(Duration::from_millis(500))
        .expect("panic must still publish a failure envelope");
    assert_eq!(panic_event.request_id, panic_id);
    let failure = panic_event
        .envelope
        .outcome
        .expect_err("panic must be surfaced as a typed failure");
    assert_eq!(failure.kind, FailureKind::ProviderFault);

    // The panicked request's owner retires only when its terminal health
    // record lands (claim -> enqueue -> record), while the event above is
    // already observable after the enqueue; retry admission across that
    // documented microsecond window instead of assuming one interleaving.
    let mut ok_admitted = false;
    for _ in 0..10_000 {
        if catalog
            .ecs_scheduler_handle()
            .lock()
            .expect("scheduler lock")
            .reserve_submission(&CapabilityId::TELEMETRY_CPU, ok_id, 0)
        {
            ok_admitted = true;
            break;
        }
        std::thread::yield_now();
    }
    assert!(ok_admitted, "panicked terminal must retire its owner");
    request_tx
        .send(Queued {
            request_id: ok_id,
            capability: CapabilityId::TELEMETRY_CPU,
            provider,
            payload: 1,
        })
        .expect("ok request queued");

    let ok_event = observation_rx
        .recv_timeout(Duration::from_millis(500))
        .expect("lane must serve the request after a provider panic");
    assert_eq!(ok_event.request_id, ok_id);
    assert!(
        ok_event.outcome.is_ok(),
        "lane stays alive after a provider panic"
    );
}

#[test]
fn stale_terminal_publications_do_not_stop_the_lane() {
    let (publisher, _control_rx, observation_rx, catalog) = fixture();
    let (request_tx, request_rx) = bounded(2);
    let workers = crate::WorkerRuntime::default();
    let served = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let marker = served.clone();
    let cpu_event = |payload: u8| {
        PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Cpu {
            revision: SystemTelemetryRevision::new(u64::from(payload) + 1),
            observation: Box::new(CpuTelemetryObservation::current(
                CpuMetrics::default(),
                fixed_clock(),
                Vec::new(),
            )),
        })
    };
    super::spawn_lane(&workers, request_rx, publisher, move |payload: u8| {
        marker.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(cpu_event(payload))
    })
    .expect("provider worker starts");

    // Neither request was ever admitted through the scheduler, so both
    // terminal publications fail their delivery claim as stale owners.
    for id in [77_u64, 78] {
        request_tx
            .send(Queued {
                request_id: RequestId::new(id).expect("fixture id"),
                capability: CapabilityId::TELEMETRY_CPU,
                provider: ProviderId::borrowed("fixture.telemetry.cpu"),
                payload: 0,
            })
            .expect("request queued");
    }
    drop(request_tx);
    let mut waited = 0;
    while served.load(std::sync::atomic::Ordering::SeqCst) < 2 && waited < 500 {
        thread::sleep(Duration::from_millis(10));
        waited += 1;
    }
    assert_eq!(
        served.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "the lane must keep serving requests after stale publications"
    );
    assert!(
        observation_rx.try_recv().is_err(),
        "a stale publication must not deliver an event"
    );
    assert_eq!(
        catalog.scheduling_snapshot().stale_terminal_publications,
        2,
        "both tolerated publications must be counted"
    );
}
