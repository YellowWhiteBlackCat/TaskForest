use super::{ProcessItem, ProcessScalarObservations};
use crate::core::FailureKind;
use crate::core::metrics::{ScalarAvailability, ScalarObservation};

fn observations_with(overrides: ProcessScalarObservations) -> ProcessItem {
    ProcessItem::new(7, "worker").with_scalar_observations(overrides)
}

#[test]
fn trustworthy_start_token_hydrates_identity_bound_legacy_values() {
    // Unknown availability + trustworthy identity (pid + observed start
    // token) permits compatibility hydration at the private wire boundary.
    let typed = ProcessItem::new(7, "worker").with_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(7_500, 10),
        ..ProcessScalarObservations::default()
    });
    let mut wire = serde_json::to_value(typed).expect("serialize typed identity");
    wire["cpu_time_secs"] = serde_json::json!(42);
    wire["fds"] = serde_json::json!(12);
    wire["nice"] = serde_json::json!(-5);
    let item: ProcessItem = serde_json::from_value(wire).expect("legacy scalar wire");

    assert_eq!(item.current_cpu_time_secs(), Some(42));
    assert_eq!(item.current_fds(), Some(12));
    assert_eq!(item.current_nice(), Some(-5));
}

#[test]
fn untrustworthy_start_token_never_hydrates_identity_bound_legacy_values() {
    // The same legacy numbers with an unavailable start token stay unknown.
    let typed = ProcessItem::new(7, "worker").with_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        ..ProcessScalarObservations::default()
    });
    let mut wire = serde_json::to_value(typed).expect("serialize failed identity");
    wire["cpu_time_secs"] = serde_json::json!(42);
    wire["fds"] = serde_json::json!(12);
    wire["nice"] = serde_json::json!(-5);
    let item: ProcessItem = serde_json::from_value(wire).expect("legacy scalar wire");

    assert_eq!(item.current_cpu_time_secs(), None);
    assert_eq!(item.current_fds(), None);
    assert_eq!(item.current_nice(), None);
}

#[test]
fn scalar_observations_retain_previous_bridges_only_unavailable_gaps() {
    let previous = ProcessScalarObservations {
        cpu_percentage: ScalarObservation::available(25.0, 10),
        ..ProcessScalarObservations::default()
    };
    let current = ProcessScalarObservations {
        cpu_percentage: ScalarObservation::unavailable(FailureKind::TimedOut),
        ..ProcessScalarObservations::default()
    };
    let bridged = current.retain_previous(previous);
    assert!(matches!(
        bridged.cpu_percentage.availability(),
        ScalarAvailability::Stale(FailureKind::TimedOut)
    ));
    assert_eq!(bridged.cpu_percentage.last_known_value(), Some(&25.0));
    assert_eq!(bridged.cpu_percentage.current_value(), None);
}

#[test]
fn available_disk_write_bytes_are_published() {
    let item = observations_with(ProcessScalarObservations {
        disk_write_bytes_total: ScalarObservation::available(2048, 10),
        ..ProcessScalarObservations::default()
    });
    assert_eq!(item.current_disk_write_bytes_total(), Some(2048));
}
