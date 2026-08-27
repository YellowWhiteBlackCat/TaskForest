use taskmanager_application::{DeviceDiscovery, DeviceId, ProviderId};
use taskmanager_core::ProcessResourceObservations;

use super::*;

fn source(outcome: SourceOutcome) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed("fixture.source"),
        outcome,
        item_count: 0,
    }
}

#[test]
fn process_resource_health_uses_published_sources_not_coarse_state() {
    let snapshot = ProcessInsightSnapshot {
        identity: taskmanager_core::ProcessIdentity {
            pid: 42,
            start_token: 900,
        },
        value: ProcessResourceSnapshot::from_observations(
            DeviceState::healthy(10),
            ProcessResourceObservations::default(),
            vec![source(SourceOutcome::Unavailable(
                FailureKind::PermissionDenied,
            ))],
        ),
    };

    assert_eq!(
        snapshot.observation_health(),
        CapabilityHealth::Unavailable(ProviderFailure::PermissionDenied)
    );
}

#[test]
fn partial_system_value_remains_degraded_when_a_contributing_source_failed() {
    let observation = CpuTelemetryObservation::partial(
        taskmanager_core::CpuMetrics::default(),
        10,
        FailureKind::PermissionDenied,
        vec![source(SourceOutcome::Unavailable(
            FailureKind::PermissionDenied,
        ))],
    );

    assert_eq!(
        observation.observation_health(),
        CapabilityHealth::Degraded(FailureKind::PermissionDenied)
    );
}

#[test]
fn executed_provider_does_not_make_unavailable_sources_available() {
    assert_eq!(
        source_health(&[source(SourceOutcome::Unavailable(
            FailureKind::PermissionDenied,
        ))]),
        CapabilityHealth::Unavailable(ProviderFailure::PermissionDenied)
    );
    assert_eq!(
        source_health(&[
            source(SourceOutcome::Available),
            source(SourceOutcome::Unavailable(FailureKind::TimedOut)),
        ]),
        CapabilityHealth::Degraded(FailureKind::TimedOut)
    );
    assert_eq!(
        source_health(&[source(SourceOutcome::Empty)]),
        CapabilityHealth::Available
    );
    assert_eq!(
        source_health(&[]),
        CapabilityHealth::Unavailable(ProviderFailure::ProviderFault)
    );
}

#[test]
fn item_count_never_overrides_the_typed_source_outcome() {
    let mut partial_without_items = source(SourceOutcome::Partial(FailureKind::TimedOut));
    partial_without_items.item_count = 0;
    assert_eq!(
        source_health(&[partial_without_items]),
        CapabilityHealth::Degraded(FailureKind::TimedOut)
    );

    let mut unavailable_with_retained_items =
        source(SourceOutcome::Unavailable(FailureKind::PermissionDenied));
    unavailable_with_retained_items.item_count = 12;
    assert_eq!(
        source_health(&[unavailable_with_retained_items]),
        CapabilityHealth::Unavailable(ProviderFailure::PermissionDenied)
    );
}

#[test]
fn enrichment_failure_never_revokes_authoritative_device_discovery() {
    let snapshot = DeviceSourceSnapshot::from_discovery(
        (),
        ProviderId::borrowed("fixture.source"),
        DeviceDiscovery::Available(vec![DeviceId::new("device:1")]),
        vec![source(SourceOutcome::Unavailable(
            FailureKind::PermissionDenied,
        ))],
    );

    assert_eq!(
        device_source_health(&snapshot),
        CapabilityHealth::Degraded(FailureKind::PermissionDenied)
    );
}

#[test]
fn unavailable_device_discovery_dominates_successful_enrichment() {
    let snapshot = DeviceSourceSnapshot::from_discovery(
        (),
        ProviderId::borrowed("fixture.source"),
        DeviceDiscovery::Unavailable(FailureKind::TimedOut),
        vec![source(SourceOutcome::Available)],
    );

    assert_eq!(
        device_source_health(&snapshot),
        CapabilityHealth::Unavailable(ProviderFailure::TimedOut)
    );
}

#[test]
fn partial_device_discovery_without_enrichments_keeps_its_failure() {
    let snapshot = DeviceSourceSnapshot::from_discovery(
        (),
        ProviderId::borrowed("fixture.source"),
        DeviceDiscovery::Partial {
            discovered_devices: vec![DeviceId::new("device:partial")],
            failure: FailureKind::Unsupported,
        },
        Vec::new(),
    );

    assert_eq!(
        device_source_health(&snapshot),
        CapabilityHealth::Degraded(FailureKind::Unsupported)
    );
}

#[test]
fn device_state_health_distinguishes_partial_and_total_failure() {
    assert_eq!(
        device_state_health([
            DeviceState::healthy(10),
            DeviceState {
                status: DeviceStatus::PermissionDenied,
                last_success_ms: None,
            },
        ]),
        CapabilityHealth::Degraded(FailureKind::PermissionDenied)
    );
    assert_eq!(
        device_state_health([DeviceState {
            status: DeviceStatus::MissingTool,
            last_success_ms: Some(5),
        }]),
        CapabilityHealth::Unavailable(ProviderFailure::MissingDependency)
    );
    assert_eq!(
        device_state_health([DeviceState {
            status: DeviceStatus::Healthy,
            last_success_ms: None,
        }]),
        CapabilityHealth::Unavailable(ProviderFailure::ProviderFault)
    );
}

#[test]
fn degraded_health_is_order_independent_and_empty_means_no_partial_failure() {
    assert_eq!(degraded_health([]), CapabilityHealth::Available);
    let left = degraded_health([
        FailureKind::Unsupported,
        FailureKind::PermissionDenied,
        FailureKind::TimedOut,
    ]);
    let right = degraded_health([
        FailureKind::TimedOut,
        FailureKind::PermissionDenied,
        FailureKind::Unsupported,
    ]);
    assert_eq!(
        left,
        CapabilityHealth::Degraded(FailureKind::PermissionDenied)
    );
    assert_eq!(right, left);
}
