use super::*;

#[test]
fn source_status_distinguishes_empty_partial_and_unavailable() {
    assert_eq!(
        source_status_from_device_state("fixture.empty", DeviceStatus::Unsupported, 0, 0).outcome,
        SourceOutcome::Empty
    );
    assert_eq!(
        source_status_from_device_state("fixture.partial", DeviceStatus::Healthy, 2, 3).outcome,
        SourceOutcome::Partial(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        source_status_from_device_state("fixture.enumeration-partial", DeviceStatus::Stale, 2, 2,)
            .outcome,
        SourceOutcome::Partial(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        source_status_from_device_state("fixture.denied", DeviceStatus::PermissionDenied, 0, 2,)
            .outcome,
        SourceOutcome::Unavailable(FailureKind::PermissionDenied)
    );
}
