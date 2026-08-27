use super::*;
use crate::core::ScalarAvailability;

fn process_wire(pid: u32, observations: ProcessScalarObservations) -> serde_json::Value {
    serde_json::to_value(ProcessItem::new(pid, "worker").with_scalar_observations(observations))
        .expect("serialize process wire fixture")
}

#[test]
fn group_failure_retains_values_only_as_stale() {
    let observations = ProcessScalarObservations {
        memory_pss_bytes: ScalarObservation::available(4096, 42),
        swap_bytes: ScalarObservation::available(1024, 42),
        threads: ScalarObservation::available(0, 42),
        start_time_secs: ScalarObservation::available(1_720_000_000, 42),
        cpu_time_secs: ScalarObservation::available(0, 42),
        fds: ScalarObservation::available(0, 42),
        nice: ScalarObservation::available(0, 42),
        ..ProcessScalarObservations::default()
    }
    .transition_failure(FailureKind::PermissionDenied);

    assert_eq!(
        observations.cpu_time_secs.availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(observations.cpu_time_secs.current_value(), None);
    assert_eq!(observations.cpu_time_secs.last_known_value(), Some(&0));
    assert_eq!(observations.cpu_time_secs.last_success_ms(), Some(42));
    assert_eq!(observations.memory_pss_bytes.current_value(), None);
    assert_eq!(
        observations.memory_pss_bytes.last_known_value(),
        Some(&4096)
    );
    assert_eq!(observations.swap_bytes.current_value(), None);
    assert_eq!(observations.swap_bytes.last_known_value(), Some(&1024));
}

#[test]
fn legacy_wire_payload_uses_unknown_only_compatibility_fallbacks() {
    let mut json = process_wire(7, ProcessScalarObservations::default());
    json["cpu_usage"] = serde_json::json!(0.0);
    json["memory_bytes"] = serde_json::json!(0);
    json["disk_read_bytes"] = serde_json::json!(0);
    json["disk_write_bytes"] = serde_json::json!(0);
    json["threads"] = serde_json::json!(8);
    json["start_time_secs"] = serde_json::json!(1_720_000_000_u64);

    let decoded: ProcessItem =
        serde_json::from_value(json).expect("deserialize legacy process row");

    assert_eq!(
        decoded.scalar_observations().cpu_percentage.availability(),
        ScalarAvailability::Available
    );
    assert_eq!(decoded.current_threads(), Some(8));
    assert_eq!(decoded.current_cpu_percentage(), Some(0.0));
    assert_eq!(decoded.current_memory_bytes(), Some(0));
    assert_eq!(decoded.current_disk_read_bytes_per_sec(), Some(0));
    assert_eq!(decoded.current_disk_write_bytes_per_sec(), Some(0));
    assert_eq!(decoded.current_disk_read_bytes_total(), None);
    assert_eq!(decoded.current_start_time_secs(), Some(1_720_000_000));
    assert_eq!(decoded.current_cpu_time_secs(), None);
    assert_eq!(decoded.current_fds(), None);
    assert_eq!(decoded.current_nice(), None);
}

#[test]
fn explicit_unavailability_never_falls_back_to_legacy_numbers() {
    let failure = FailureKind::PermissionDenied;
    let mut wire = process_wire(
        7,
        ProcessScalarObservations {
            start_token: ScalarObservation::unavailable(failure),
            cpu_percentage: ScalarObservation::unavailable(failure),
            memory_bytes: ScalarObservation::unavailable(failure),
            memory_pss_bytes: ScalarObservation::unavailable(failure),
            swap_bytes: ScalarObservation::unavailable(failure),
            disk_read_bytes_total: ScalarObservation::unavailable(failure),
            disk_write_bytes_total: ScalarObservation::unavailable(failure),
            disk_read_bytes_per_sec: ScalarObservation::unavailable(failure),
            disk_write_bytes_per_sec: ScalarObservation::unavailable(failure),
            threads: ScalarObservation::unavailable(failure),
            start_time_secs: ScalarObservation::unavailable(failure),
            cpu_time_secs: ScalarObservation::unavailable(failure),
            fds: ScalarObservation::unavailable(failure),
            nice: ScalarObservation::unavailable(failure),
        },
    );
    wire["cpu_usage"] = serde_json::json!(25.0);
    wire["memory_bytes"] = serde_json::json!(4096);
    wire["disk_read_bytes"] = serde_json::json!(100);
    wire["disk_write_bytes"] = serde_json::json!(200);
    wire["threads"] = serde_json::json!(8);
    wire["start_time_secs"] = serde_json::json!(1_720_000_000_u64);
    wire["cpu_time_secs"] = serde_json::json!(7);
    wire["fds"] = serde_json::json!(12);
    wire["nice"] = serde_json::json!(-5);
    let item: ProcessItem = serde_json::from_value(wire).expect("conflicting scalar wire");

    assert_eq!(item.current_start_token(), None);
    assert_eq!(item.current_cpu_percentage(), None);
    assert_eq!(item.current_memory_bytes(), None);
    assert_eq!(item.current_disk_read_bytes_total(), None);
    assert_eq!(item.current_disk_write_bytes_total(), None);
    assert_eq!(item.current_disk_read_bytes_per_sec(), None);
    assert_eq!(item.current_disk_write_bytes_per_sec(), None);
    assert_eq!(item.current_threads(), None);
    assert_eq!(item.current_start_time_secs(), None);
    assert_eq!(item.current_cpu_time_secs(), None);
    assert_eq!(item.current_fds(), None);
    assert_eq!(item.current_nice(), None);
}

#[test]
fn typed_only_payload_survives_without_any_legacy_scalar_keys() {
    let observations = ProcessScalarObservations {
        start_token: ScalarObservation::available(600, 42),
        cpu_percentage: ScalarObservation::available(12.5, 42),
        memory_bytes: ScalarObservation::available(4096, 42),
        disk_read_bytes_per_sec: ScalarObservation::available(100, 42),
        disk_write_bytes_per_sec: ScalarObservation::available(200, 42),
        threads: ScalarObservation::available(8, 42),
        start_time_secs: ScalarObservation::available(1_720_000_000, 42),
        cpu_time_secs: ScalarObservation::available(7, 42),
        fds: ScalarObservation::available(12, 42),
        nice: ScalarObservation::available(-5, 42),
        ..ProcessScalarObservations::default()
    };
    let mut wire = process_wire(7, observations);
    let object = wire.as_object_mut().expect("process row object");
    for key in [
        "cpu_usage",
        "memory_bytes",
        "disk_read_bytes",
        "disk_write_bytes",
        "threads",
        "start_time_secs",
        "cpu_time_secs",
        "fds",
        "nice",
    ] {
        object.remove(key);
    }

    let decoded: ProcessItem = serde_json::from_value(wire).expect("typed-only scalar payload");
    assert_eq!(decoded.scalar_observations(), &observations);
    assert_eq!(decoded.current_cpu_percentage(), Some(12.5));
    assert_eq!(decoded.current_nice(), Some(-5));
}

#[test]
fn typed_failure_omits_legacy_success_keys_and_stale_never_rehydrates() {
    let failure = FailureKind::PermissionDenied;
    let failed = ProcessScalarObservations {
        start_token: ScalarObservation::available(600, 42).transition_failure(failure),
        cpu_percentage: ScalarObservation::available(12.5, 42).transition_failure(failure),
        memory_bytes: ScalarObservation::available(4096, 42).transition_failure(failure),
        threads: ScalarObservation::available(8, 42).transition_failure(failure),
        ..ProcessScalarObservations::default()
    };
    let mut wire = process_wire(7, failed);
    for key in ["cpu_usage", "memory_bytes", "threads"] {
        assert!(
            wire.get(key).is_none(),
            "failure must omit legacy key {key}"
        );
    }
    wire["cpu_usage"] = serde_json::json!(99.0);
    wire["memory_bytes"] = serde_json::json!(99);
    wire["threads"] = serde_json::json!(99);

    let decoded: ProcessItem = serde_json::from_value(wire).expect("conflicting stale payload");
    assert_eq!(decoded.current_cpu_percentage(), None);
    assert_eq!(decoded.current_memory_bytes(), None);
    assert_eq!(decoded.current_threads(), None);
}

#[test]
fn legacy_scalars_reject_pid_zero_and_zero_presence_sentinels() {
    let mut pid_zero = process_wire(0, ProcessScalarObservations::default());
    pid_zero["cpu_usage"] = serde_json::json!(0.0);
    pid_zero["memory_bytes"] = serde_json::json!(0);
    pid_zero["disk_read_bytes"] = serde_json::json!(0);
    let decoded: ProcessItem = serde_json::from_value(pid_zero).expect("PID zero legacy row");
    assert_eq!(decoded.current_cpu_percentage(), None);
    assert_eq!(decoded.current_memory_bytes(), None);
    assert_eq!(decoded.current_disk_read_bytes_per_sec(), None);

    let mut zero_sentinels = process_wire(7, ProcessScalarObservations::default());
    zero_sentinels["threads"] = serde_json::json!(0);
    zero_sentinels["start_time_secs"] = serde_json::json!(0);
    let decoded: ProcessItem =
        serde_json::from_value(zero_sentinels).expect("zero presence sentinel row");
    assert_eq!(decoded.current_threads(), None);
    assert_eq!(decoded.current_start_time_secs(), None);
}

#[test]
fn default_empty_row_does_not_turn_legacy_zero_sentinels_into_measurements() {
    let item = ProcessItem::default();

    assert_eq!(item.current_cpu_percentage(), None);
    assert_eq!(item.current_memory_bytes(), None);
    assert_eq!(item.current_disk_read_bytes_per_sec(), None);
    assert_eq!(item.current_disk_write_bytes_per_sec(), None);
    assert_eq!(item.current_cpu_time_secs(), None);
    assert_eq!(item.current_fds(), None);
    assert_eq!(item.current_nice(), None);
    assert_eq!(item.current_memory_pss_bytes(), None);
    assert_eq!(item.current_swap_bytes(), None);
}

#[test]
fn legacy_zero_sentinels_require_exact_current_identity_success() {
    let mut wire = process_wire(7, ProcessScalarObservations::default());
    wire["cpu_time_secs"] = serde_json::json!(0);
    wire["fds"] = serde_json::json!(0);
    wire["nice"] = serde_json::json!(0);
    let item: ProcessItem = serde_json::from_value(wire.clone()).expect("legacy scalar row");

    assert_eq!(item.current_cpu_time_secs(), None);
    assert_eq!(item.current_fds(), None);
    assert_eq!(item.current_nice(), None);

    wire["scalar_observations"]["start_token"] =
        serde_json::to_value(ScalarObservation::available(600_u64, 42))
            .expect("serialize start token");
    let trusted: ProcessItem = serde_json::from_value(wire).expect("identity-bound scalar row");
    assert_eq!(trusted.current_cpu_time_secs(), Some(0));
    assert_eq!(trusted.current_fds(), Some(0));
    assert_eq!(trusted.current_nice(), Some(0));
}

#[test]
fn measured_zero_live_scalars_remain_current_and_project_to_legacy_fields() {
    let mut item = ProcessItem::new(7, "worker");
    item.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(600, 42),
        cpu_percentage: ScalarObservation::available(0.0, 42),
        memory_bytes: ScalarObservation::available(0, 42),
        disk_read_bytes_total: ScalarObservation::available(0, 42),
        disk_write_bytes_total: ScalarObservation::available(0, 42),
        disk_read_bytes_per_sec: ScalarObservation::available(0, 42),
        disk_write_bytes_per_sec: ScalarObservation::available(0, 42),
        cpu_time_secs: ScalarObservation::available(0, 42),
        fds: ScalarObservation::available(0, 42),
        nice: ScalarObservation::available(0, 42),
        ..ProcessScalarObservations::default()
    });

    assert_eq!(item.current_start_token(), Some(600));
    assert_eq!(item.current_cpu_percentage(), Some(0.0));
    assert_eq!(item.current_memory_bytes(), Some(0));
    assert_eq!(item.current_disk_read_bytes_total(), Some(0));
    assert_eq!(item.current_disk_write_bytes_per_sec(), Some(0));
    assert_eq!(item.current_cpu_time_secs(), Some(0));
    assert_eq!(item.current_fds(), Some(0));
    assert_eq!(item.current_nice(), Some(0));
    let wire = serde_json::to_value(item).expect("serialize measured zero row");
    assert_eq!(wire["cpu_usage"], serde_json::json!(0.0));
    assert_eq!(wire["memory_bytes"], serde_json::json!(0));
    assert_eq!(wire["disk_read_bytes"], serde_json::json!(0));
    assert_eq!(wire["cpu_time_secs"], serde_json::json!(0));
}

#[test]
fn pss_and_swap_preserve_current_zero_without_mixing_measurement_kinds() {
    let mut item = ProcessItem::new(7, "worker");
    item.apply_scalar_observations(ProcessScalarObservations {
        memory_bytes: ScalarObservation::available(99 * 1024, 42),
        memory_pss_bytes: ScalarObservation::available(0, 42),
        swap_bytes: ScalarObservation::available(0, 42),
        ..ProcessScalarObservations::default()
    });

    assert_eq!(item.current_memory_bytes(), Some(99 * 1024));
    assert_eq!(item.current_memory_pss_bytes(), Some(0));
    assert_eq!(item.current_swap_bytes(), Some(0));

    let failed = item
        .scalar_observations()
        .transition_failure(FailureKind::PermissionDenied);
    assert_eq!(failed.memory_pss_bytes.current_value(), None);
    assert_eq!(failed.memory_pss_bytes.last_known_value(), Some(&0));
    assert_eq!(failed.swap_bytes.current_value(), None);
    assert_eq!(failed.swap_bytes.last_known_value(), Some(&0));
}

#[test]
fn older_scalar_payload_defaults_new_memory_facets_to_unknown() {
    let mut json = serde_json::to_value(ProcessScalarObservations {
        start_token: ScalarObservation::available(7, 1),
        cpu_percentage: ScalarObservation::available(0.0, 1),
        memory_bytes: ScalarObservation::available(4096, 1),
        ..ProcessScalarObservations::default()
    })
    .expect("serialize legacy scalar payload");
    let object = json.as_object_mut().expect("scalar payload is an object");
    object.remove("memory_pss_bytes");
    object.remove("swap_bytes");
    let decoded: ProcessScalarObservations =
        serde_json::from_value(json).expect("legacy scalar payload remains readable");

    assert_eq!(
        decoded.memory_pss_bytes.availability(),
        ScalarAvailability::Unknown
    );
    assert_eq!(
        decoded.swap_bytes.availability(),
        ScalarAvailability::Unknown
    );
    assert_eq!(decoded.memory_pss_bytes.current_value(), None);
    assert_eq!(decoded.swap_bytes.current_value(), None);
}
