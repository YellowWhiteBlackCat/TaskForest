use super::*;
use crate::core::{DeviceStatus, FailureKind};

#[test]
fn empty_healthy_is_a_real_empty_not_unknown() {
    let breakdown = ProcessGpuEngines::empty_healthy(1_000);
    assert_eq!(breakdown.state, DeviceState::healthy(1_000));
    assert!(breakdown.engines.is_empty());
}

#[test]
fn unavailable_never_carries_engines() {
    let denied = DeviceState::healthy(10).transition(DeviceStatus::PermissionDenied, 20);
    let breakdown = ProcessGpuEngines::unavailable(denied);
    assert_eq!(breakdown.state.status, DeviceStatus::PermissionDenied);
    assert!(breakdown.engines.is_empty());
}

#[test]
fn cold_start_usage_is_unavailable_but_cumulative_is_observed() {
    let now_ms = 5_000;
    let engine = ProcessGpuEngineUsage {
        name: "render".into(),
        usage_pct: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        engine_time_ns: ScalarObservation::available(1_000_000, now_ms),
        engine_cycles: ScalarObservation::default(),
    };
    // The cumulative counter is honest from the first read ...
    assert_eq!(engine.engine_time_ns.current_value(), Some(&1_000_000));
    // ... but the rate is a typed gap until a second sample arrives.
    assert!(engine.usage_pct.current_value().is_none());
    // A cycles-only xe source keeps the ns counter unknown.
    assert!(engine.engine_cycles.current_value().is_none());
}
