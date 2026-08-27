use super::*;
use std::collections::HashSet;

fn device(id: &str, driver: &str) -> NpuDevice {
    NpuDevice {
        device_id: DeviceId::new(id),
        driver: Some(driver.to_owned()),
        ..NpuDevice::default()
    }
}

#[test]
fn discovered_sorts_devices_deterministically() {
    let snapshot = NpuInventorySnapshot::discovered(
        vec![device("accel1", "intel_vpu"), device("accel0", "intel_vpu")],
        42,
    );
    let ids: Vec<&str> = snapshot
        .devices
        .iter()
        .map(|device| device.device_id.as_str())
        .collect();
    assert_eq!(ids, ["accel0", "accel1"]);
    assert!(snapshot.is_success());
    assert_eq!(snapshot.observed_at_ms, 42);
}

#[test]
fn empty_discovery_is_an_honest_success_not_a_failure() {
    let snapshot = NpuInventorySnapshot::discovered(Vec::new(), 7);
    assert!(snapshot.is_success());
    assert!(snapshot.devices.is_empty());
    assert_eq!(snapshot.failure, None);
}

#[test]
fn failed_never_carries_a_fabricated_device() {
    let snapshot = NpuInventorySnapshot::failed(FailureKind::Unsupported, "no accel subsystem", 7);
    assert!(!snapshot.is_success());
    assert!(snapshot.devices.is_empty());
    let failure = snapshot.failure.as_ref().expect("failure must be tagged");
    assert_eq!(failure.kind, FailureKind::Unsupported);
}

#[test]
fn snapshots_and_devices_round_trip_through_serde() {
    let snapshot = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("accel0"),
            brand: Some("Intel AI Boost".into()),
            driver: Some("intel_vpu".into()),
            utilization_pct: ScalarObservation::unavailable(FailureKind::Unsupported),
            engines: vec![NpuEngineUsage {
                kind: NpuEngineKind::Matrix,
                utilization_pct: ScalarObservation::unavailable(FailureKind::Unsupported),
            }],
            memory: NpuMemoryReport {
                dedicated_total_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
                shared_total_bytes: ScalarObservation::available(2_147_483_648, 7),
            },
            ..NpuDevice::default()
        }],
        7,
    );
    let json = serde_json::to_string(&snapshot).expect("snapshot serializes");
    let decoded: NpuInventorySnapshot = serde_json::from_str(&json).expect("snapshot round-trips");
    assert_eq!(decoded, snapshot);
}

#[test]
fn engine_kind_all_has_unique_variants() {
    let mut seen = HashSet::new();
    for kind in NpuEngineKind::ALL {
        let json = serde_json::to_string(kind).expect("kind serializes");
        assert!(seen.insert(json), "duplicate engine kind wire form");
    }
}
