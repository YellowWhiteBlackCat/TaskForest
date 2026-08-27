use super::{
    DeviceState, DeviceStatus, DiskSmart, SmartAvailability, SmartProviderFailureKind,
    refresh_state,
};

#[test]
fn availability_maps_to_state_status_and_failure() {
    for (availability, expected_status, expected_failure) in [
        (SmartAvailability::Available, DeviceStatus::Healthy, None),
        (
            SmartAvailability::Unsupported,
            DeviceStatus::Unsupported,
            Some(SmartProviderFailureKind::UnsupportedProtocol),
        ),
        (
            SmartAvailability::Unavailable,
            DeviceStatus::Stale,
            Some(SmartProviderFailureKind::TemporarilyUnavailable),
        ),
        (
            SmartAvailability::MissingTool,
            DeviceStatus::MissingTool,
            Some(SmartProviderFailureKind::MissingTool),
        ),
        (
            SmartAvailability::PermissionDenied,
            DeviceStatus::PermissionDenied,
            Some(SmartProviderFailureKind::PermissionDenied),
        ),
    ] {
        let smart = DiskSmart::with_availability(availability);
        assert_eq!(smart.availability, availability);
        assert_eq!(smart.state.status, expected_status);
        assert_eq!(smart.failure, expected_failure);
    }
}

#[test]
fn failure_kinds_fold_to_the_expected_availability() {
    for (failure, expected) in [
        (
            SmartProviderFailureKind::UnsupportedProtocol,
            SmartAvailability::Unsupported,
        ),
        (
            SmartProviderFailureKind::BridgeLimitation,
            SmartAvailability::Unsupported,
        ),
        (
            SmartProviderFailureKind::MissingTool,
            SmartAvailability::MissingTool,
        ),
        (
            SmartProviderFailureKind::PermissionDenied,
            SmartAvailability::PermissionDenied,
        ),
        (
            SmartProviderFailureKind::TimedOut,
            SmartAvailability::Unavailable,
        ),
        (
            SmartProviderFailureKind::MalformedResponse,
            SmartAvailability::Unavailable,
        ),
        (
            SmartProviderFailureKind::DeviceUnavailable,
            SmartAvailability::Unavailable,
        ),
        (
            SmartProviderFailureKind::CommandFailed,
            SmartAvailability::Unavailable,
        ),
        (
            SmartProviderFailureKind::TemporarilyUnavailable,
            SmartAvailability::Unavailable,
        ),
    ] {
        let smart = DiskSmart::with_failure(failure);
        assert_eq!(smart.availability, expected, "{failure:?}");
        assert_eq!(smart.failure, Some(failure));
    }
}

#[test]
fn refresh_state_merges_the_previous_history() {
    // transition(Healthy) refreshes the success marker (a
    // `refresh_state → ()` mutation leaves the observed state untouched).
    let previous = DeviceState::healthy(10);
    let mut observed = DiskSmart::with_availability(SmartAvailability::Available);
    observed.state = DeviceState {
        status: DeviceStatus::Healthy,
        last_success_ms: None,
    };
    refresh_state(previous, &mut observed, 20);
    assert_eq!(observed.state.status, DeviceStatus::Healthy);
    assert_eq!(
        observed.state.last_success_ms,
        Some(20),
        "the refreshed marker must carry now_ms"
    );

    // A non-healthy observed status keeps its own status but inherits
    // the previous success marker.
    let mut degraded = DiskSmart::with_availability(SmartAvailability::Unavailable);
    degraded.state = DeviceState {
        status: DeviceStatus::Stale,
        last_success_ms: None,
    };
    refresh_state(previous, &mut degraded, 20);
    assert_eq!(degraded.state.status, DeviceStatus::Stale);
    assert_eq!(degraded.state.last_success_ms, Some(10));
}
