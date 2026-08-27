use super::*;
use crate::core::{DeviceStatus, FailureKind};

#[test]
fn empty_healthy_rollup_is_a_real_empty_not_unknown() {
    let rollup = ContainerRollup::empty_healthy(1_000);
    assert_eq!(rollup.state.status, DeviceStatus::Healthy);
    assert!(rollup.containers.is_empty());
    assert!(!rollup.has_current_reading());
}

#[test]
fn unavailable_rollup_never_carries_rows() {
    let denied = DeviceState::healthy(10).transition(DeviceStatus::PermissionDenied, 20);
    let rollup = ContainerRollup::unavailable(denied);
    assert_eq!(rollup.state.status, DeviceStatus::PermissionDenied);
    assert!(rollup.containers.is_empty());
}

#[test]
fn has_current_reading_distinguishes_present_from_unavailable_fields() {
    let mut rollup = ContainerRollup::empty_healthy(1_000);
    rollup.containers.push(ContainerSummary {
        id: "/docker/abc".into(),
        name: "abc".into(),
        runtime: Some(IsolationKind::Docker),
        cgroup_path: "/docker/abc".into(),
        cpu_percentage: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        memory_bytes: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        member_pids: Vec::new(),
    });
    assert!(!rollup.has_current_reading());
    rollup.containers[0].memory_bytes = ScalarObservation::available(1_048_576, 1_000);
    assert!(rollup.has_current_reading());
}
