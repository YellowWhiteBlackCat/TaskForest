use taskmanager_application::{
    CapabilityId, CapabilityRequest, CapabilityScheduler, CompositeSourceSnapshot, EventPort,
    HardwareInventoryEvent, HardwareInventoryRequest, MAX_REQUEST_SCOPE_BYTES, PlatformEvent,
    ProviderFailure, RequestId, RequestScope, RequestTracking, RequestTrackingError,
    SidebandPolicy, SourceOutcome, SourceStatus, SubmissionErrorKind,
};
use taskmanager_application::{
    DirectoryScanBounds, DirectoryScanId, DirectoryScanSpec, DirectoryUsageRequest,
};

use super::*;

fn scheduler_handle() -> EcsSchedulerHandle {
    RuntimeEcsSchedulerHandle::new(
        &[crate::config::CapabilityRoute {
            capability: CapabilityId::HARDWARE_INVENTORY,
            provider: ProviderId::borrowed("fixture.runtime"),
            delivery: crate::config::DeliveryClass::Observation,
            domain: crate::config::RuntimeDomain::System,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        }],
        zero_clock,
    )
}

const fn zero_clock() -> u64 {
    0
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScopedFixtureRequest {
    scope: String,
}

impl CapabilityRequest for ScopedFixtureRequest {
    const CAPABILITY: CapabilityId = CapabilityId::HARDWARE_INVENTORY;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        RequestScope::try_from_str(&self.scope).map(RequestTracking::Target)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UnownedFixtureRequest;

impl CapabilityRequest for UnownedFixtureRequest {
    const CAPABILITY: CapabilityId = CapabilityId::HARDWARE_INVENTORY;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        Ok(RequestTracking::Sideband)
    }
}

use crate::delivery::LaneFlow;

#[test]
fn invalid_target_scope_never_reaches_the_lane_or_ecs() {
    let provider = ProviderId::borrowed("fixture.runtime");
    let scheduler = scheduler_handle();
    let (Some(port), Some(receiver)) =
        request_lane::<ScopedFixtureRequest>(1, Some(&provider), scheduler.clone())
    else {
        panic!("present provider must create a typed lane");
    };

    assert_eq!(
        port.try_submit(RequestEnvelope {
            id: RequestId::new(90).expect("fixture request id"),
            capability: CapabilityId::HARDWARE_INVENTORY,
            submitted_at_ms: 0,
            payload: ScopedFixtureRequest {
                scope: "x".repeat(MAX_REQUEST_SCOPE_BYTES + 1),
            },
        })
        .map_err(|error| error.kind),
        Err(SubmissionErrorKind::InvalidRequest)
    );
    assert!(receiver.try_recv().is_err());
    {
        let diagnostics = scheduler.lock().expect("scheduler lock").diagnostics();
        assert_eq!(diagnostics.submission_count(), 0);
        assert_eq!(
            scheduler.lock().expect("scheduler lock").target_job_count(),
            0
        );
    }

    port.try_submit(RequestEnvelope {
        id: RequestId::new(91).expect("fixture request id"),
        capability: CapabilityId::HARDWARE_INVENTORY,
        submitted_at_ms: 1,
        payload: ScopedFixtureRequest {
            scope: "x".repeat(MAX_REQUEST_SCOPE_BYTES),
        },
    })
    .expect("exact boundary remains admissible");
    assert_eq!(
        receiver
            .try_recv()
            .expect("bounded request")
            .payload
            .scope
            .len(),
        MAX_REQUEST_SCOPE_BYTES
    );
    {
        let scheduler = scheduler.lock().expect("scheduler lock");
        assert_eq!(scheduler.diagnostics().submission_count(), 1);
        assert_eq!(scheduler.target_job_count(), 1);
    }
}

#[test]
fn unaudited_sideband_fails_before_the_worker_lane() {
    let provider = ProviderId::borrowed("fixture.runtime");
    let scheduler = scheduler_handle();
    let (Some(port), Some(receiver)) =
        request_lane::<UnownedFixtureRequest>(1, Some(&provider), scheduler.clone())
    else {
        panic!("present provider must create a typed lane");
    };

    assert_eq!(
        port.try_submit(RequestEnvelope {
            id: RequestId::new(92).expect("fixture request id"),
            capability: CapabilityId::HARDWARE_INVENTORY,
            submitted_at_ms: 0,
            payload: UnownedFixtureRequest,
        })
        .map_err(|error| error.kind),
        Err(SubmissionErrorKind::InvalidRequest)
    );
    assert!(receiver.try_recv().is_err());
    assert_eq!(
        scheduler
            .lock()
            .expect("scheduler lock")
            .diagnostics()
            .admission_rejections(EcsAdmissionError::SidebandNotAllowed),
        1
    );
}

#[test]
fn typed_port_validates_capability_and_reports_bounded_backpressure() {
    let provider = ProviderId::borrowed("fixture.runtime");
    let (Some(port), Some(receiver)) =
        request_lane::<HardwareInventoryRequest>(1, Some(&provider), scheduler_handle())
    else {
        panic!("present provider must create a typed lane");
    };
    let first_id = RequestId::new(1).expect("fixture request id");
    assert_eq!(
        port.try_submit(RequestEnvelope {
            id: first_id,
            capability: CapabilityId::PROCESS_LIST,
            submitted_at_ms: 0,
            payload: HardwareInventoryRequest::Refresh,
        })
        .map_err(|error| error.kind),
        Err(SubmissionErrorKind::InvalidRequest),
    );
    port.try_submit(RequestEnvelope {
        id: first_id,
        capability: CapabilityId::HARDWARE_INVENTORY,
        submitted_at_ms: 0,
        payload: HardwareInventoryRequest::Refresh,
    })
    .expect("first bounded submission");
    assert_eq!(
        port.try_submit(RequestEnvelope {
            id: RequestId::new(2).expect("fixture request id"),
            capability: CapabilityId::HARDWARE_INVENTORY,
            submitted_at_ms: 0,
            payload: HardwareInventoryRequest::Refresh,
        })
        .map_err(|error| error.kind),
        Err(SubmissionErrorKind::Busy),
    );

    let queued = receiver.try_recv().expect("queued request");
    assert_eq!(queued.request_id, first_id);
    assert_eq!(queued.capability, CapabilityId::HARDWARE_INVENTORY);
    assert_eq!(queued.provider, provider);
}

#[test]
fn accepted_typed_enqueue_is_observed_by_the_ecs_lifecycle() {
    let provider = ProviderId::borrowed("fixture.runtime");
    let scheduler = RuntimeEcsSchedulerHandle::new(
        &[crate::config::CapabilityRoute {
            capability: CapabilityId::HARDWARE_INVENTORY,
            provider: provider.clone(),
            delivery: crate::config::DeliveryClass::Observation,
            domain: crate::config::RuntimeDomain::System,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        }],
        zero_clock,
    );
    let (Some(port), Some(_receiver)) =
        request_lane::<HardwareInventoryRequest>(1, Some(&provider), scheduler.clone())
    else {
        panic!("present provider must create a typed lane");
    };
    port.try_submit(RequestEnvelope {
        id: RequestId::new(3).expect("fixture request id"),
        capability: CapabilityId::HARDWARE_INVENTORY,
        submitted_at_ms: 12,
        payload: HardwareInventoryRequest::Refresh,
    })
    .expect("typed enqueue");
    let diagnostics = scheduler.lock().expect("scheduler lock").diagnostics();
    assert_eq!(diagnostics.submission_count(), 1);
}

#[test]
fn ecs_admission_rejects_duplicate_in_flight_work_before_lane_capacity() {
    let provider = ProviderId::borrowed("fixture.runtime");
    let scheduler = RuntimeEcsSchedulerHandle::new(
        &[crate::config::CapabilityRoute {
            capability: CapabilityId::HARDWARE_INVENTORY,
            provider: provider.clone(),
            delivery: crate::config::DeliveryClass::Observation,
            domain: crate::config::RuntimeDomain::System,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        }],
        zero_clock,
    );
    let (Some(port), Some(receiver)) =
        request_lane::<HardwareInventoryRequest>(2, Some(&provider), scheduler)
    else {
        panic!("present provider must create a typed lane");
    };

    for id in [10, 11] {
        let result = port.try_submit(RequestEnvelope {
            id: RequestId::new(id).expect("fixture request id"),
            capability: CapabilityId::HARDWARE_INVENTORY,
            submitted_at_ms: 0,
            payload: HardwareInventoryRequest::Refresh,
        });
        if id == 10 {
            result.expect("first typed enqueue");
        } else {
            assert_eq!(
                result.map_err(|error| error.kind),
                Err(SubmissionErrorKind::Busy)
            );
        }
    }

    assert!(receiver.try_recv().is_ok());
    assert!(receiver.try_recv().is_err());
}

#[test]
fn target_tracked_scan_accepts_its_sideband_cancel_without_a_residual_job() {
    let provider = ProviderId::borrowed("fixture.directory");
    let scheduler = RuntimeEcsSchedulerHandle::new(
        &[crate::config::CapabilityRoute {
            capability: CapabilityId::DIRECTORY_USAGE,
            provider: provider.clone(),
            delivery: crate::config::DeliveryClass::Observation,
            domain: crate::config::RuntimeDomain::Storage,
            cadence_ms: None,
            sideband_policy: SidebandPolicy::Idempotent,
        }],
        zero_clock,
    );
    let (Some(port), Some(receiver)) =
        request_lane::<DirectoryUsageRequest>(2, Some(&provider), scheduler.clone())
    else {
        panic!("present provider must create a typed lane");
    };
    let scan_request = RequestId::new(60).expect("fixture request id");
    port.try_submit(RequestEnvelope {
        id: scan_request,
        capability: CapabilityId::DIRECTORY_USAGE,
        submitted_at_ms: 0,
        payload: DirectoryUsageRequest::StartScan(DirectoryScanSpec {
            root: "/data".to_string(),
            bounds: DirectoryScanBounds::default(),
        }),
    })
    .expect("target scan accepted");
    port.try_submit(RequestEnvelope {
        id: RequestId::new(61).expect("fixture request id"),
        capability: CapabilityId::DIRECTORY_USAGE,
        submitted_at_ms: 1,
        payload: DirectoryUsageRequest::Cancel(DirectoryScanId::new(scan_request.get())),
    })
    .expect("sideband cancel accepted while target scan is active");

    assert!(matches!(
        receiver.try_recv().expect("queued scan").payload,
        DirectoryUsageRequest::StartScan(_)
    ));
    assert!(matches!(
        receiver.try_recv().expect("queued cancel").payload,
        DirectoryUsageRequest::Cancel(_)
    ));
    let mut scheduler = scheduler.lock().expect("scheduler lock");
    assert_eq!(scheduler.target_job_count(), 1);
    assert!(
        scheduler
            .record_health(
                &CapabilityId::DIRECTORY_USAGE,
                scan_request,
                crate::CapabilityHealth::Available,
                2,
            )
            .is_accepted()
    );
    assert_eq!(scheduler.target_job_count(), 0);
}

#[test]
fn full_target_lane_rolls_back_the_unqueued_ecs_entity() {
    let provider = ProviderId::borrowed("fixture.directory");
    let scheduler = RuntimeEcsSchedulerHandle::new(
        &[crate::config::CapabilityRoute {
            capability: CapabilityId::DIRECTORY_USAGE,
            provider: provider.clone(),
            delivery: crate::config::DeliveryClass::Observation,
            domain: crate::config::RuntimeDomain::Storage,
            cadence_ms: None,
            sideband_policy: SidebandPolicy::Idempotent,
        }],
        zero_clock,
    );
    let (Some(port), Some(receiver)) =
        request_lane::<DirectoryUsageRequest>(1, Some(&provider), scheduler.clone())
    else {
        panic!("present provider must create a typed lane");
    };
    let scan = |id: u64, root: &str| RequestEnvelope {
        id: RequestId::new(id).expect("fixture request id"),
        capability: CapabilityId::DIRECTORY_USAGE,
        submitted_at_ms: 0,
        payload: DirectoryUsageRequest::StartScan(DirectoryScanSpec {
            root: root.to_string(),
            bounds: DirectoryScanBounds::default(),
        }),
    };

    port.try_submit(scan(70, "/a")).expect("first scan queued");
    assert_eq!(
        port.try_submit(scan(71, "/b")).map_err(|error| error.kind),
        Err(SubmissionErrorKind::Busy)
    );
    assert_eq!(
        scheduler.lock().expect("scheduler lock").target_job_count(),
        1,
        "the rejected enqueue must not leave its target entity behind"
    );

    receiver.try_recv().expect("free bounded lane capacity");
    port.try_submit(scan(72, "/b"))
        .expect("rolled-back target scope is immediately reusable");
    assert_eq!(
        scheduler.lock().expect("scheduler lock").target_job_count(),
        2
    );
}

#[test]
fn disconnected_capability_lane_rolls_back_its_ecs_claim() {
    let provider = ProviderId::borrowed("fixture.runtime");
    let scheduler = scheduler_handle();
    let (Some(port), Some(receiver)) =
        request_lane::<HardwareInventoryRequest>(1, Some(&provider), scheduler.clone())
    else {
        panic!("present provider must create a typed lane");
    };
    drop(receiver);

    assert_eq!(
        port.try_submit(RequestEnvelope {
            id: RequestId::new(80).expect("fixture request id"),
            capability: CapabilityId::HARDWARE_INVENTORY,
            submitted_at_ms: 0,
            payload: HardwareInventoryRequest::Refresh,
        })
        .map_err(|error| error.kind),
        Err(SubmissionErrorKind::RuntimeStopped)
    );

    let (Some(replacement), Some(_receiver)) =
        request_lane::<HardwareInventoryRequest>(1, Some(&provider), scheduler)
    else {
        panic!("replacement lane must exist");
    };
    replacement
        .try_submit(RequestEnvelope {
            id: RequestId::new(81).expect("fixture request id"),
            capability: CapabilityId::HARDWARE_INVENTORY,
            submitted_at_ms: 1,
            payload: HardwareInventoryRequest::Refresh,
        })
        .expect("disconnected enqueue must not strand the capability claim");
}

#[test]
fn disconnected_target_lane_retires_the_unqueued_ecs_entity() {
    let provider = ProviderId::borrowed("fixture.directory");
    let scheduler = RuntimeEcsSchedulerHandle::new(
        &[crate::config::CapabilityRoute {
            capability: CapabilityId::DIRECTORY_USAGE,
            provider: provider.clone(),
            delivery: crate::config::DeliveryClass::Observation,
            domain: crate::config::RuntimeDomain::Storage,
            cadence_ms: None,
            sideband_policy: SidebandPolicy::Idempotent,
        }],
        zero_clock,
    );
    let (Some(port), Some(receiver)) =
        request_lane::<DirectoryUsageRequest>(1, Some(&provider), scheduler.clone())
    else {
        panic!("present provider must create a typed lane");
    };
    drop(receiver);

    assert_eq!(
        port.try_submit(RequestEnvelope {
            id: RequestId::new(82).expect("fixture request id"),
            capability: CapabilityId::DIRECTORY_USAGE,
            submitted_at_ms: 0,
            payload: DirectoryUsageRequest::StartScan(DirectoryScanSpec {
                root: "/disconnected".to_string(),
                bounds: DirectoryScanBounds::default(),
            }),
        })
        .map_err(|error| error.kind),
        Err(SubmissionErrorKind::RuntimeStopped)
    );
    assert_eq!(
        scheduler.lock().expect("scheduler lock").target_job_count(),
        0,
        "a disconnected lane must not retain an unqueued target entity"
    );
}

#[test]
fn failed_scheduled_submission_cannot_rollback_an_interleaved_explicit_owner() {
    let capability = CapabilityId::HARDWARE_INVENTORY;
    let provider = ProviderId::borrowed("fixture.runtime");
    let routes = [crate::config::CapabilityRoute {
        capability: capability.clone(),
        provider: provider.clone(),
        delivery: crate::config::DeliveryClass::Observation,
        domain: crate::config::RuntimeDomain::System,
        cadence_ms: Some(1_000),
        sideband_policy: SidebandPolicy::Denied,
    }];
    let catalog = Arc::new(crate::delivery::RuntimeCapabilityCatalog::new(
        &routes, zero_clock,
    ));
    let scheduler = catalog.ecs_scheduler_handle();
    let (Some(port), Some(receiver)) =
        request_lane::<HardwareInventoryRequest>(1, Some(&provider), scheduler)
    else {
        panic!("present provider must create a typed lane");
    };
    let request = |id| RequestEnvelope {
        id: RequestId::new(id).expect("fixture request id"),
        capability: capability.clone(),
        submitted_at_ms: 0,
        payload: HardwareInventoryRequest::Refresh,
    };

    assert_eq!(
        CapabilityScheduler::poll_due(catalog.as_ref(), 0),
        vec![capability.clone()],
        "the automatic route must first become Ready"
    );
    let explicit = RequestId::new(101).expect("fixture request id");
    port.try_submit(request(explicit.get()))
        .expect("explicit request claims the planned route first");
    assert_eq!(
        receiver
            .try_recv()
            .expect("provider receives explicit work")
            .request_id,
        explicit
    );
    assert_eq!(
        port.try_submit(request(102)).map_err(|error| error.kind),
        Err(SubmissionErrorKind::Busy),
        "the later scheduled request loses the admission race"
    );

    CapabilityScheduler::mark_submission_failed(catalog.as_ref(), &capability, 0);
    assert_eq!(
        port.try_submit(request(103)).map_err(|error| error.kind),
        Err(SubmissionErrorKind::Busy),
        "scheduled rollback must not clear the explicit in-flight owner"
    );

    let (control_tx, control_rx) = bounded(1);
    let (observation_tx, observation_rx) = bounded(1);
    let queues = catalog.event_queue_state();
    let publisher = crate::delivery::RuntimeEventPublisher::new(
        control_tx,
        observation_tx,
        catalog.clone(),
        Vec::new(),
        zero_clock,
    );
    let terminal_event = || {
        PlatformEvent::HardwareInventory(HardwareInventoryEvent::Snapshot(Box::new(
            CompositeSourceSnapshot::new(
                taskmanager_core::HardwareInfo::default(),
                vec![SourceStatus {
                    provider: ProviderId::borrowed("fixture.runtime"),
                    outcome: SourceOutcome::Unavailable(
                        taskmanager_application::FailureKind::TemporarilyUnavailable,
                    ),
                    item_count: 0,
                }],
            ),
        )))
    };
    assert_eq!(
        publisher.publish_health(
            explicit,
            capability.clone(),
            provider.clone(),
            terminal_event(),
            crate::health::CapabilityHealth::Unavailable(ProviderFailure::TemporarilyUnavailable,),
        ),
        LaneFlow::Continue
    );
    assert_eq!(
        publisher.publish_health(
            explicit,
            capability.clone(),
            provider,
            terminal_event(),
            crate::health::CapabilityHealth::Unavailable(ProviderFailure::TemporarilyUnavailable,),
        ),
        LaneFlow::Continue,
        "a superseded terminal publication is tolerated as stale"
    );
    assert_eq!(
        CapabilityScheduler::scheduling_snapshot(catalog.as_ref())
            .budgets
            .pending_deliveries,
        1,
        "terminal retention owns its permit until frontend drain"
    );

    let events =
        crate::delivery::FairEventPort::new(control_rx, observation_rx, queues, catalog.clone());
    assert_eq!(
        events
            .try_recv()
            .expect("event port")
            .expect("explicit terminal remains reachable")
            .request_id,
        explicit
    );
    assert!(events.try_recv().expect("event port").is_none());
    assert_eq!(
        CapabilityScheduler::scheduling_snapshot(catalog.as_ref())
            .budgets
            .pending_deliveries,
        0,
        "draining the unique terminal releases the delivery permit"
    );
    port.try_submit(request(104))
        .expect("the completed owner releases the route for a later request");
}
