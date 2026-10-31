use super::StorageTelemetryObservation;
use crate::core::metrics::DiskMetrics;
use crate::core::metrics::system::ProviderRuntimeState;
use crate::core::metrics::system::domain::SystemObservationState;
use crate::core::{FailureKind, ProviderId, SourceOutcome, SourceStatus};

fn sources() -> Vec<SourceStatus> {
    vec![SourceStatus {
        provider: ProviderId::borrowed("linux.storage.sysfs"),
        outcome: SourceOutcome::Available,
        item_count: 2,
    }]
}

fn provider_state() -> Vec<ProviderRuntimeState> {
    vec![ProviderRuntimeState {
        provider: ProviderId::borrowed("linux.storage.sysfs"),
        status: crate::core::DeviceStatus::Healthy,
        last_success_ms: Some(10),
    }]
}

fn disks() -> Vec<DiskMetrics> {
    vec![DiskMetrics::default()]
}

#[test]
fn current_observation_exposes_value_state_and_sources() {
    let observation = StorageTelemetryObservation::current(
        disks(),
        10,
        sources(),
        provider_state(),
        Default::default(),
    );
    assert!(matches!(
        observation.state(),
        SystemObservationState::Current { .. }
    ));
    assert_eq!(observation.current_value().map(|d| d.len()), Some(1));
    assert_eq!(observation.last_known_value().map(|d| d.len()), Some(1));
    assert_eq!(observation.sources().len(), 1);
}

#[test]
fn partial_observation_is_current_with_the_failure_recorded() {
    let observation = StorageTelemetryObservation::partial(
        disks(),
        10,
        FailureKind::TemporarilyUnavailable,
        sources(),
        provider_state(),
        Default::default(),
    );
    assert!(matches!(
        observation.state(),
        SystemObservationState::Partial { .. }
    ));
    assert_eq!(observation.current_value().map(|d| d.len()), Some(1));
    assert_eq!(observation.sources().len(), 1);
}

#[test]
fn stale_observation_keeps_only_last_known_and_never_a_current_value() {
    let observation = StorageTelemetryObservation::stale(
        disks(),
        10,
        FailureKind::TimedOut,
        sources(),
        provider_state(),
        Default::default(),
    );
    assert!(matches!(
        observation.state(),
        SystemObservationState::Stale { .. }
    ));
    assert!(
        observation.current_value().is_none(),
        "stale storage must not be presented as current"
    );
    assert_eq!(observation.last_known_value().map(|d| d.len()), Some(1));
}

#[test]
fn unavailable_observation_exposes_sources_but_no_values() {
    let observation = StorageTelemetryObservation::unavailable(
        FailureKind::PermissionDenied,
        sources(),
        provider_state(),
        Default::default(),
    );
    assert!(matches!(
        observation.state(),
        SystemObservationState::Unavailable { .. }
    ));
    assert!(observation.current_value().is_none());
    assert!(observation.last_known_value().is_none());
    assert_eq!(
        observation.sources().len(),
        1,
        "the failure receipt's sources must survive"
    );
}
