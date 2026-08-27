use std::thread;
use std::time::Duration;

use taskmanager_application::{
    CapabilityStatus, EventEnvelope, FailureKind, LatestControlRequest, PlatformHandle, ProviderId,
    RequestEnvelope, RequestId, ServiceControlRequest, ServiceDependenciesRequest, ServiceEvent,
    ServiceInventoryRequest, ServiceLogSnapshotRequest, ServiceLogStreamRequest, ServiceUpdate,
    SourceOutcome, SourceStatus,
};
use taskmanager_core::{
    ServiceAction, ServiceDeps, ServiceItem, ServiceLogErrorKind, ServiceLogFailure,
    ServiceLogLevelFilter, ServiceLogQuery, ServiceLogState, ServiceLogStreamEnd,
    ServiceLogStreamState, ServiceLogTimeFilter, ServiceStatus,
};

use super::*;
use crate::{ProviderBinding, RuntimeConfig, RuntimeProviderBindings};

fn fixed_clock() -> u64 {
    71
}

fn service_bindings() -> RuntimeProviderBindings {
    let mut bindings = RuntimeProviderBindings::default();
    bindings.service.inventory =
        ProviderBinding::present(ProviderId::borrowed("fixture.service.inventory"));
    bindings.service.dependencies =
        ProviderBinding::present(ProviderId::borrowed("fixture.service.dependencies"));
    bindings.service.control =
        ProviderBinding::present(ProviderId::borrowed("fixture.service.control"));
    bindings.service.log_snapshot =
        ProviderBinding::present(ProviderId::borrowed("fixture.service.log-snapshot"));
    bindings.service.log_stream =
        ProviderBinding::present(ProviderId::borrowed("fixture.service.log-stream"));
    bindings
}

fn wait_event(handle: &PlatformHandle) -> EventEnvelope<PlatformEvent> {
    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("connected event port") {
            return event;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("service runtime event did not arrive");
}

fn registered_service_provider(capability: &CapabilityId) -> ProviderId {
    if capability == &CapabilityId::SERVICES {
        ProviderId::borrowed("fixture.service.inventory")
    } else if capability == &CapabilityId::SERVICE_DEPENDENCIES {
        ProviderId::borrowed("fixture.service.dependencies")
    } else if capability == &CapabilityId::SERVICE_CONTROL {
        ProviderId::borrowed("fixture.service.control")
    } else if capability == &CapabilityId::SERVICE_LOGS {
        ProviderId::borrowed("fixture.service.log-snapshot")
    } else if capability == &CapabilityId::SERVICE_LOG_STREAM {
        ProviderId::borrowed("fixture.service.log-stream")
    } else {
        panic!("unexpected service capability {capability}");
    }
}

fn assert_registered_service_provider(event: &EventEnvelope<PlatformEvent>) {
    assert_eq!(
        event.provider,
        Some(registered_service_provider(&event.capability))
    );
}

fn service_query() -> ServiceLogQuery {
    ServiceLogQuery {
        service_id: "demo".into(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: Some("cursor".into()),
    }
}

#[test]
fn service_catalog_keeps_five_distinct_registered_provider_identities() {
    let runtime = crate::ChannelRuntime::new(service_bindings(), RuntimeConfig::new(fixed_clock));
    let capabilities = runtime.handle.capabilities().snapshot();

    for capability in [
        CapabilityId::SERVICES,
        CapabilityId::SERVICE_DEPENDENCIES,
        CapabilityId::SERVICE_CONTROL,
        CapabilityId::SERVICE_LOGS,
        CapabilityId::SERVICE_LOG_STREAM,
    ] {
        assert_eq!(
            capabilities
                .get(&capability)
                .map(|descriptor| descriptor.providers.clone()),
            Some(vec![registered_service_provider(&capability)])
        );
    }
}

#[test]
fn pending_service_group_promotes_atomically_and_reports_exact_missing_lane() {
    let complete = crate::ChannelRuntime::new(service_bindings(), RuntimeConfig::new(fixed_clock));
    assert_eq!(complete.lanes.service.missing_capabilities().count(), 0);
    assert!(complete.lanes.service.try_complete().is_some());

    let mut incomplete_bindings = service_bindings();
    incomplete_bindings.service.log_stream = ProviderBinding::absent();
    let incomplete =
        crate::ChannelRuntime::new(incomplete_bindings, RuntimeConfig::new(fixed_clock));
    assert_eq!(
        incomplete
            .lanes
            .service
            .missing_capabilities()
            .collect::<Vec<_>>(),
        [CapabilityId::SERVICE_LOG_STREAM]
    );
    assert!(incomplete.lanes.service.try_complete().is_none());
}

#[test]
fn inventory_health_is_derived_from_the_published_source_snapshot() {
    let runtime = crate::ChannelRuntime::new(service_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_service_lanes(
        &workers,
        lanes
            .service
            .try_complete()
            .expect("complete service lanes"),
        ServiceExecutors::new(
            || {
                Ok(PartialSourceSnapshot::new(
                    vec![ServiceItem::from_inventory(
                        "",
                        "demo",
                        ServiceStatus::Unknown,
                        "",
                        "",
                        "",
                        "",
                    )],
                    vec![SourceStatus {
                        provider: ProviderId::borrowed("fixture.service.inventory"),
                        outcome: SourceOutcome::Partial(FailureKind::TimedOut),
                        item_count: 1,
                    }],
                ))
            },
            |_service| Err(ProviderFailure::Unsupported),
            |_service, _action| Err(ProviderFailure::Unsupported),
            |_service| Err(ProviderFailure::Unsupported),
            |_query, _observed_at_ms| Err(ProviderFailure::Unsupported),
        ),
        publisher,
        fixed_clock,
    )
    .expect("service workers start");
    handle
        .service_inventory()
        .expect("service inventory port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(1).expect("fixture id"),
            capability: CapabilityId::SERVICES,
            submitted_at_ms: 1,
            payload: ServiceInventoryRequest::Refresh,
        })
        .expect("inventory request accepted");

    let event = wait_event(&handle);
    assert_registered_service_provider(&event);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::Services(ServiceEvent::Snapshot(ref snapshot)))
            if snapshot.items.len() == 1
    ));
    assert_eq!(
        handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::SERVICES)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::Degraded(FailureKind::TimedOut))
    );
}

#[test]
fn dependencies_and_action_publish_domain_completion_on_provider_failure() {
    let runtime = crate::ChannelRuntime::new(service_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_service_lanes(
        &workers,
        lanes
            .service
            .try_complete()
            .expect("complete service lanes"),
        ServiceExecutors::new(
            || Err(ProviderFailure::Unsupported),
            |_service| Err(ProviderFailure::PermissionDenied),
            |_service, _action| Err(ProviderFailure::Rejected),
            |_service| Err(ProviderFailure::Unsupported),
            |_query, _observed_at_ms| Err(ProviderFailure::Unsupported),
        ),
        publisher,
        fixed_clock,
    )
    .expect("service workers start");
    handle
        .service_dependencies()
        .expect("dependencies port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(2).expect("fixture id"),
            capability: CapabilityId::SERVICE_DEPENDENCIES,
            submitted_at_ms: 1,
            payload: ServiceDependenciesRequest {
                service_id: "demo".into(),
            },
        })
        .expect("dependencies request accepted");
    let mut control_generations = LatestControlRequest::default();
    let service_control_id = control_generations.begin();
    handle
        .service_control()
        .expect("control port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(3).expect("fixture id"),
            capability: CapabilityId::SERVICE_CONTROL,
            submitted_at_ms: 1,
            payload: ServiceControlRequest {
                request_id: service_control_id,
                service_id: "demo".into(),
                action: ServiceAction::Restart,
            },
        })
        .expect("control request accepted");

    let mut dependencies_seen = false;
    let mut control_seen = false;
    for _ in 0..2 {
        let event = wait_event(&handle);
        assert_registered_service_provider(&event);
        let envelope_request_id = event.request_id;
        match event.outcome {
            Ok(PlatformEvent::Services(ServiceEvent::Update(
                ServiceUpdate::DependenciesUnavailable {
                    request_id,
                    error: FailureKind::PermissionDenied,
                    ..
                },
            ))) if request_id == envelope_request_id => dependencies_seen = true,
            Ok(PlatformEvent::Services(ServiceEvent::Update(ServiceUpdate::Action(outcome))))
                if outcome.request_id == service_control_id
                    && outcome.service_id.as_str() == "demo"
                    && outcome.action == ServiceAction::Restart
                    && outcome.result == Err(FailureKind::Rejected) =>
            {
                control_seen = true;
            }
            other => panic!("unexpected service completion: {other:?}"),
        }
    }
    assert!(dependencies_seen && control_seen);
}

#[test]
fn log_truth_controls_domain_event_and_capability_health_together() {
    let runtime = crate::ChannelRuntime::new(service_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_service_lanes(
        &workers,
        lanes
            .service
            .try_complete()
            .expect("complete service lanes"),
        ServiceExecutors::new(
            || Err(ProviderFailure::Unsupported),
            |_service| Ok(ServiceDeps::default()),
            |_service, _action| Ok(()),
            |_service| {
                Ok(ServiceLogState::Unavailable(
                    ServiceLogFailure::with_detail(
                        ServiceLogErrorKind::PermissionDenied,
                        "fixture denied",
                    ),
                ))
            },
            |_query, observed_at_ms| {
                assert_eq!(observed_at_ms, fixed_clock());
                Ok(ServiceLogStreamState::Ended(
                    ServiceLogStreamEnd::disconnected("fixture disconnected"),
                ))
            },
        ),
        publisher,
        fixed_clock,
    )
    .expect("service workers start");
    handle
        .service_log_snapshot()
        .expect("log snapshot port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(4).expect("fixture id"),
            capability: CapabilityId::SERVICE_LOGS,
            submitted_at_ms: 1,
            payload: ServiceLogSnapshotRequest {
                service_id: "demo".into(),
            },
        })
        .expect("snapshot request accepted");
    handle
        .service_log_stream()
        .expect("log stream port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(5).expect("fixture id"),
            capability: CapabilityId::SERVICE_LOG_STREAM,
            submitted_at_ms: 1,
            payload: ServiceLogStreamRequest {
                query: service_query(),
            },
        })
        .expect("stream request accepted");

    let mut snapshot_seen = false;
    let mut stream_seen = false;
    for _ in 0..2 {
        let event = wait_event(&handle);
        assert_registered_service_provider(&event);
        let envelope_request_id = event.request_id;
        match event.outcome {
            Ok(PlatformEvent::Services(ServiceEvent::Update(ServiceUpdate::Logs(snapshot))))
                if matches!(
                    snapshot.state,
                    ServiceLogState::Unavailable(ServiceLogFailure {
                        kind: ServiceLogErrorKind::PermissionDenied,
                        ..
                    })
                ) =>
            {
                snapshot_seen = true;
            }
            Ok(PlatformEvent::Services(ServiceEvent::Update(ServiceUpdate::LogStream {
                request_id,
                snapshot,
                ..
            }))) if request_id == envelope_request_id
                && matches!(
                    snapshot.state,
                    ServiceLogStreamState::Ended(ServiceLogStreamEnd::Disconnected { .. })
                ) =>
            {
                stream_seen = true;
            }
            other => panic!("unexpected log completion: {other:?}"),
        }
    }
    assert!(snapshot_seen && stream_seen);
    let capabilities = handle.capabilities().snapshot();
    assert_eq!(
        capabilities
            .get(&CapabilityId::SERVICE_LOGS)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::PermissionRequired)
    );
    assert_eq!(
        capabilities
            .get(&CapabilityId::SERVICE_LOG_STREAM)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::TemporarilyUnavailable)
    );
}
