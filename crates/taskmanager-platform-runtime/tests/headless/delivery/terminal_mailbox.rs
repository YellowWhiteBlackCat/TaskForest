use std::sync::Arc;

use crossbeam_channel::bounded;
use taskmanager_application::{
    CapabilityId, CapabilityScheduler, CpuMetrics, CpuTelemetryObservation, EventPort,
    PlatformEvent, ProcessEvent, ProviderId, RequestId, SystemTelemetryDomainEvent,
    SystemTelemetryRevision,
};
use taskmanager_core::FrozenProcessIdentity;

use super::event_queue::EventQueueState;
use super::worker::{WorkerQuota, WorkerRuntime};
use super::{FairEventPort, LaneFlow, RuntimeCapabilityCatalog, RuntimeEventPublisher, spawn_lane};
use crate::Queued;
use crate::config::{CapabilityRoute, DeliveryClass, RuntimeBudgets, RuntimeDomain};

fn fixed_clock() -> u64 {
    42
}

fn request_id(value: u64) -> RequestId {
    RequestId::new(value).expect("fixture request ID")
}

fn cpu_event(revision: u64) -> PlatformEvent {
    PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Cpu {
        revision: SystemTelemetryRevision::new(revision),
        observation: Box::new(CpuTelemetryObservation::current(
            CpuMetrics::default(),
            fixed_clock(),
            Vec::new(),
        )),
    })
}

fn process_event() -> PlatformEvent {
    PlatformEvent::Processes(ProcessEvent::EndTaskCompleted(
        FrozenProcessIdentity::from_authoritative_parts(42, "fixture", 7, 700)
            .expect("fixture process identity"),
    ))
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
fn terminal_backlog_is_bounded_and_one_drain_reopens_admission() {
    let budgets = RuntimeBudgets {
        route_limit: 1,
        active_target_limit: 1,
        active_target_limit_per_capability: 1,
        active_target_limit_per_domain: 1,
        target_scope_byte_limit: taskmanager_application::MAX_REQUEST_SCOPE_BYTES,
        pending_delivery_limit: 3,
        control_delivery_reserve: 1,
        max_stalled_lifetime_ms: RuntimeBudgets::DEFAULT.max_stalled_lifetime_ms,
    };
    let routes = [CapabilityRoute {
        capability: CapabilityId::TELEMETRY_CPU,
        provider: ProviderId::borrowed("fixture.cpu"),
        delivery: DeliveryClass::Observation,
        domain: RuntimeDomain::System,
        cadence_ms: None,
        sideband_policy: taskmanager_application::SidebandPolicy::Denied,
    }];
    let queues = Arc::new(EventQueueState::new(budgets.pending_delivery_limit));
    let catalog = Arc::new(RuntimeCapabilityCatalog::with_resources(
        &routes,
        fixed_clock,
        budgets,
        queues.clone(),
    ));
    let (control_tx, control_rx) = bounded(1);
    let (observation_tx, observation_rx) = bounded(1);
    let publisher = RuntimeEventPublisher::with_event_queues(
        control_tx,
        observation_tx,
        queues.clone(),
        catalog.clone(),
        Vec::new(),
        fixed_clock,
    );

    for value in 1..=2 {
        let request = request_id(value);
        assert_eq!(
            reserve(&catalog, &CapabilityId::TELEMETRY_CPU, request),
            Ok(())
        );
        assert_eq!(
            publisher.publish_health(
                request,
                CapabilityId::TELEMETRY_CPU,
                ProviderId::borrowed("fixture.cpu"),
                cpu_event(value),
                crate::health::CapabilityHealth::Available,
            ),
            LaneFlow::Continue
        );
    }
    assert_eq!(
        reserve(&catalog, &CapabilityId::TELEMETRY_CPU, request_id(3),),
        Err(crate::ecs::EcsAdmissionError::ObservationDeliveryCapacity)
    );
    let pressure = CapabilityScheduler::scheduling_snapshot(catalog.as_ref());
    assert_eq!(pressure.budgets.pending_deliveries, 2);
    assert_eq!(pressure.budgets.pending_observation_deliveries, 2);
    assert_eq!(pressure.budgets.pending_control_deliveries, 0);
    assert_eq!(pressure.event_queues.terminal_mailbox_pending, 1);
    assert_eq!(pressure.event_queues.terminal_mailbox_high_water, 1);
    assert_eq!(pressure.event_queues.observation_pending, 1);
    assert_eq!(pressure.admission.delivery_capacity, 1);
    assert_eq!(pressure.admission.observation_delivery_capacity, 1);
    assert_eq!(pressure.admission.control_delivery_capacity, 0);

    let port = FairEventPort::new(control_rx, observation_rx, queues, catalog.clone());
    assert_eq!(
        port.try_recv()
            .expect("event port")
            .expect("first retained terminal")
            .request_id,
        request_id(1)
    );
    assert_eq!(
        CapabilityScheduler::scheduling_snapshot(catalog.as_ref())
            .budgets
            .pending_deliveries,
        1
    );
    assert_eq!(
        reserve(&catalog, &CapabilityId::TELEMETRY_CPU, request_id(3),),
        Ok(())
    );
}

#[test]
fn primary_refill_cannot_overtake_an_older_retained_terminal() {
    let routes = [CapabilityRoute {
        capability: CapabilityId::TELEMETRY_CPU,
        provider: ProviderId::borrowed("fixture.cpu"),
        delivery: DeliveryClass::Observation,
        domain: RuntimeDomain::System,
        cadence_ms: None,
        sideband_policy: taskmanager_application::SidebandPolicy::Denied,
    }];
    let queues = Arc::new(EventQueueState::new(
        RuntimeBudgets::DEFAULT.pending_delivery_limit,
    ));
    let catalog = Arc::new(RuntimeCapabilityCatalog::with_resources(
        &routes,
        fixed_clock,
        RuntimeBudgets::DEFAULT,
        queues.clone(),
    ));
    let (control_tx, control_rx) = bounded(1);
    let (observation_tx, observation_rx) = bounded(1);
    let publisher = RuntimeEventPublisher::with_event_queues(
        control_tx,
        observation_tx,
        queues.clone(),
        catalog.clone(),
        Vec::new(),
        fixed_clock,
    );

    for value in 21..=22 {
        let request = request_id(value);
        assert_eq!(
            reserve(&catalog, &CapabilityId::TELEMETRY_CPU, request),
            Ok(())
        );
        assert_eq!(
            publisher.publish_health(
                request,
                CapabilityId::TELEMETRY_CPU,
                ProviderId::borrowed("fixture.cpu"),
                cpu_event(value),
                crate::health::CapabilityHealth::Available,
            ),
            LaneFlow::Continue
        );
    }
    let port = FairEventPort::new(control_rx, observation_rx, queues, catalog.clone());
    assert_eq!(
        port.try_recv()
            .expect("event port")
            .expect("first primary")
            .request_id,
        request_id(21)
    );

    let third = request_id(23);
    assert_eq!(
        reserve(&catalog, &CapabilityId::TELEMETRY_CPU, third),
        Ok(())
    );
    assert_eq!(
        publisher.publish_health(
            third,
            CapabilityId::TELEMETRY_CPU,
            ProviderId::borrowed("fixture.cpu"),
            cpu_event(23),
            crate::health::CapabilityHealth::Available,
        ),
        LaneFlow::Continue
    );

    assert_eq!(
        port.try_recv()
            .expect("event port")
            .expect("older mailbox event")
            .request_id,
        request_id(22),
        "an older mailbox sequence must beat a later primary refill"
    );
    assert_eq!(
        port.try_recv()
            .expect("event port")
            .expect("later primary event")
            .request_id,
        third
    );
}

#[test]
fn observation_backlog_cannot_consume_the_control_delivery_reserve() {
    let routes = [
        CapabilityRoute {
            capability: CapabilityId::TELEMETRY_CPU,
            provider: ProviderId::borrowed("fixture.cpu"),
            delivery: DeliveryClass::Observation,
            domain: RuntimeDomain::System,
            cadence_ms: None,
            sideband_policy: taskmanager_application::SidebandPolicy::Denied,
        },
        CapabilityRoute {
            capability: CapabilityId::PROCESS_CONTROL,
            provider: ProviderId::borrowed("fixture.control"),
            delivery: DeliveryClass::Control,
            domain: RuntimeDomain::Process,
            cadence_ms: None,
            sideband_policy: taskmanager_application::SidebandPolicy::Denied,
        },
    ];
    let budgets = RuntimeBudgets {
        route_limit: 2,
        active_target_limit: 1,
        active_target_limit_per_capability: 1,
        active_target_limit_per_domain: 1,
        target_scope_byte_limit: taskmanager_application::MAX_REQUEST_SCOPE_BYTES,
        pending_delivery_limit: 4,
        control_delivery_reserve: 1,
        max_stalled_lifetime_ms: RuntimeBudgets::DEFAULT.max_stalled_lifetime_ms,
    };
    let queues = Arc::new(EventQueueState::new(budgets.pending_delivery_limit));
    let catalog = Arc::new(RuntimeCapabilityCatalog::with_resources(
        &routes,
        fixed_clock,
        budgets,
        queues.clone(),
    ));
    let (control_tx, control_rx) = bounded(1);
    let (observation_tx, observation_rx) = bounded(1);
    let publisher = RuntimeEventPublisher::with_event_queues(
        control_tx,
        observation_tx,
        queues.clone(),
        catalog.clone(),
        vec![CapabilityId::PROCESS_CONTROL],
        fixed_clock,
    );

    for value in 31..=33 {
        let request = request_id(value);
        assert_eq!(
            reserve(&catalog, &CapabilityId::TELEMETRY_CPU, request),
            Ok(())
        );
        assert_eq!(
            publisher.publish_health(
                request,
                CapabilityId::TELEMETRY_CPU,
                ProviderId::borrowed("fixture.cpu"),
                cpu_event(value),
                crate::health::CapabilityHealth::Available,
            ),
            LaneFlow::Continue
        );
    }
    assert_eq!(
        reserve(&catalog, &CapabilityId::TELEMETRY_CPU, request_id(34)),
        Err(crate::ecs::EcsAdmissionError::ObservationDeliveryCapacity),
        "observation owners must stop before consuming control headroom"
    );

    let control = request_id(35);
    assert_eq!(
        reserve(&catalog, &CapabilityId::PROCESS_CONTROL, control),
        Ok(()),
        "one control owner remains admissible under saturated observations"
    );
    assert_eq!(
        publisher.publish_health(
            control,
            CapabilityId::PROCESS_CONTROL,
            ProviderId::borrowed("fixture.control"),
            process_event(),
            crate::health::CapabilityHealth::Available,
        ),
        LaneFlow::Continue
    );
    let snapshot = CapabilityScheduler::scheduling_snapshot(catalog.as_ref());
    assert_eq!(snapshot.budgets.pending_observation_deliveries, 3);
    assert_eq!(snapshot.budgets.pending_control_deliveries, 1);
    assert_eq!(snapshot.budgets.control_delivery_reserve, 1);
    assert_eq!(snapshot.admission.observation_delivery_capacity, 1);
    assert_eq!(snapshot.admission.control_delivery_capacity, 0);

    let port = FairEventPort::new(control_rx, observation_rx, queues, catalog);
    assert_eq!(
        port.try_recv()
            .expect("event port")
            .expect("reserved control terminal")
            .request_id,
        control,
        "control delivery remains live under observation backlog"
    );
}

#[test]
fn retained_control_and_observation_terminals_remain_fair_and_fifo() {
    let routes = [
        CapabilityRoute {
            capability: CapabilityId::PROCESS_CONTROL,
            provider: ProviderId::borrowed("fixture.control"),
            delivery: DeliveryClass::Control,
            domain: RuntimeDomain::Process,
            cadence_ms: None,
            sideband_policy: taskmanager_application::SidebandPolicy::Denied,
        },
        CapabilityRoute {
            capability: CapabilityId::TELEMETRY_CPU,
            provider: ProviderId::borrowed("fixture.cpu"),
            delivery: DeliveryClass::Observation,
            domain: RuntimeDomain::System,
            cadence_ms: None,
            sideband_policy: taskmanager_application::SidebandPolicy::Denied,
        },
    ];
    let budgets = RuntimeBudgets {
        pending_delivery_limit: 10,
        control_delivery_reserve: 2,
        route_limit: 2,
        active_target_limit: 6,
        active_target_limit_per_capability: 2,
        active_target_limit_per_domain: 3,
        target_scope_byte_limit: 6 * taskmanager_application::MAX_REQUEST_SCOPE_BYTES,
        max_stalled_lifetime_ms: RuntimeBudgets::DEFAULT.max_stalled_lifetime_ms,
    };
    let queues = Arc::new(EventQueueState::new(budgets.pending_delivery_limit));
    let catalog = Arc::new(RuntimeCapabilityCatalog::with_resources(
        &routes,
        fixed_clock,
        budgets,
        queues.clone(),
    ));
    let (control_tx, control_rx) = bounded(1);
    let (observation_tx, observation_rx) = bounded(1);
    let publisher = RuntimeEventPublisher::with_event_queues(
        control_tx,
        observation_tx,
        queues.clone(),
        catalog.clone(),
        vec![CapabilityId::PROCESS_CONTROL],
        fixed_clock,
    );

    for index in 0..4 {
        let control_id = request_id(index * 2 + 1);
        assert_eq!(
            reserve(&catalog, &CapabilityId::PROCESS_CONTROL, control_id),
            Ok(())
        );
        assert_eq!(
            publisher.publish_health(
                control_id,
                CapabilityId::PROCESS_CONTROL,
                ProviderId::borrowed("fixture.control"),
                process_event(),
                crate::health::CapabilityHealth::Available,
            ),
            LaneFlow::Continue
        );
        let observation_id = request_id(index * 2 + 2);
        assert_eq!(
            reserve(&catalog, &CapabilityId::TELEMETRY_CPU, observation_id),
            Ok(())
        );
        assert_eq!(
            publisher.publish_health(
                observation_id,
                CapabilityId::TELEMETRY_CPU,
                ProviderId::borrowed("fixture.cpu"),
                cpu_event(index + 1),
                crate::health::CapabilityHealth::Available,
            ),
            LaneFlow::Continue
        );
    }

    let port = FairEventPort::new(control_rx, observation_rx, queues, catalog);
    let delivered = (0..8)
        .map(|_| {
            port.try_recv()
                .expect("event port")
                .expect("retained terminal")
                .request_id
        })
        .collect::<Vec<_>>();
    assert_eq!(
        delivered,
        (1..=8).map(request_id).collect::<Vec<_>>(),
        "class fairness and per-class FIFO preserve the interleaved publication order"
    );
}

#[test]
fn undrained_terminal_does_not_pin_a_worker_quota_after_lane_shutdown() {
    let routes = [CapabilityRoute {
        capability: CapabilityId::TELEMETRY_CPU,
        provider: ProviderId::borrowed("fixture.cpu"),
        delivery: DeliveryClass::Observation,
        domain: RuntimeDomain::System,
        cadence_ms: None,
        sideband_policy: taskmanager_application::SidebandPolicy::Denied,
    }];
    let queues = Arc::new(EventQueueState::new(
        RuntimeBudgets::DEFAULT.pending_delivery_limit,
    ));
    let catalog = Arc::new(RuntimeCapabilityCatalog::with_resources(
        &routes,
        fixed_clock,
        RuntimeBudgets::DEFAULT,
        queues.clone(),
    ));
    let (control_tx, control_rx) = bounded(1);
    let (observation_tx, observation_rx) = bounded(1);
    let queued_request = request_id(98);
    assert_eq!(
        reserve(&catalog, &CapabilityId::TELEMETRY_CPU, queued_request),
        Ok(())
    );
    let prefill_publisher = RuntimeEventPublisher::with_event_queues(
        control_tx.clone(),
        observation_tx.clone(),
        queues.clone(),
        catalog.clone(),
        Vec::new(),
        fixed_clock,
    );
    assert_eq!(
        prefill_publisher.publish_health(
            queued_request,
            CapabilityId::TELEMETRY_CPU,
            ProviderId::borrowed("fixture.cpu"),
            cpu_event(98),
            crate::health::CapabilityHealth::Available,
        ),
        LaneFlow::Continue
    );
    let publisher = Arc::new(RuntimeEventPublisher::with_event_queues(
        control_tx,
        observation_tx,
        queues.clone(),
        catalog.clone(),
        Vec::new(),
        fixed_clock,
    ));
    let quota = Arc::new(WorkerQuota::new(1));
    let workers = WorkerRuntime::with_quota(1, quota.clone());
    let (request_tx, request_rx) = bounded(1);
    spawn_lane(&workers, request_rx, publisher, |()| Ok(cpu_event(1)))
        .expect("provider lane starts");
    let request = request_id(99);
    assert_eq!(
        reserve(&catalog, &CapabilityId::TELEMETRY_CPU, request),
        Ok(())
    );
    request_tx
        .send(Queued {
            request_id: request,
            capability: CapabilityId::TELEMETRY_CPU,
            provider: ProviderId::borrowed("fixture.cpu"),
            payload: (),
        })
        .expect("queued provider work");
    drop(request_tx);

    let mut terminal_visible = false;
    for _ in 0..10_000 {
        if CapabilityScheduler::scheduling_snapshot(catalog.as_ref())
            .event_queues
            .terminal_mailbox_pending
            == 1
        {
            terminal_visible = true;
            break;
        }
        std::thread::yield_now();
    }
    assert!(
        terminal_visible,
        "provider must transfer its terminal result"
    );
    let mut worker_reaped = false;
    for _ in 0..10_000 {
        if workers.reap_finished() == 1 {
            worker_reaped = true;
            break;
        }
        std::thread::yield_now();
    }
    assert!(worker_reaped, "disconnected idle lane must terminate");
    let challenger = WorkerRuntime::with_quota(1, quota);
    challenger
        .spawn("quota-reuse".into(), |_| {})
        .expect("finished lane returned its process permit");

    let port = FairEventPort::new(control_rx, observation_rx, queues, catalog);
    assert_eq!(
        port.try_recv()
            .expect("event port")
            .expect("primary terminal remains reachable")
            .request_id,
        queued_request
    );
    assert_eq!(
        port.try_recv()
            .expect("event port")
            .expect("mailbox terminal survives worker exit")
            .request_id,
        request,
    );
}
