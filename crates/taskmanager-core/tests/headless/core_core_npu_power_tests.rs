//! Behavior tests for the NPU power/energy contract (CORE-49).
//!
//! The contract publishes an accelerator's power and energy only where the
//! platform reports them. An inventory without a power observation carries no
//! energy series at all — the absence is never replaced by a value derived
//! from utilization.

use super::*;
use crate::core::metrics::ScalarAvailability;

fn device(id: &str) -> NpuDevice {
    NpuDevice {
        device_id: DeviceId::new(id),
        ..NpuDevice::default()
    }
}

#[test]
fn an_inventory_without_power_observations_carries_no_energy_series() {
    let accelerator = device("npu:0");
    let snapshot = NpuInventorySnapshot::discovered(vec![accelerator.clone()], 42);
    assert!(
        snapshot.power.is_empty(),
        "an inventory without a capability report carries no energy series"
    );
    assert!(
        snapshot.power_for(&accelerator.device_id).is_none(),
        "the energy series is absent, never derived from utilization"
    );
}

#[test]
fn a_power_observation_is_filed_under_its_accelerator_identity() {
    let accelerator = device("npu:0");
    let other = device("npu:1");
    let report = NpuPowerReport {
        power_w: ScalarObservation::available(2.5, 42),
        energy_j: ScalarObservation::available(3_000, 42),
    };
    let snapshot =
        NpuInventorySnapshot::discovered(vec![accelerator.clone()], 42).with_power(vec![
            NpuPowerObservation {
                device_id: accelerator.device_id.clone(),
                report,
            },
        ]);

    let observed = snapshot
        .power_for(&accelerator.device_id)
        .expect("the measured accelerator carries its power observation");
    assert_eq!(observed.power_w.current_value().copied(), Some(2.5));
    assert_eq!(observed.energy_j.current_value().copied(), Some(3_000));
    assert!(
        snapshot.power_for(&other.device_id).is_none(),
        "another accelerator must not inherit the observation"
    );
}

#[test]
fn an_unsupported_report_keeps_both_observations_unavailable() {
    let report = NpuPowerReport::unsupported();
    assert_eq!(
        report.power_w.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(
        report.energy_j.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert!(report.power_w.current_value().is_none());
    assert!(report.energy_j.current_value().is_none());
}

#[test]
fn the_power_observations_survive_the_snapshot_wire_boundary() {
    let accelerator = device("npu:0");
    let snapshot =
        NpuInventorySnapshot::discovered(vec![accelerator.clone()], 42).with_power(vec![
            NpuPowerObservation {
                device_id: accelerator.device_id.clone(),
                report: NpuPowerReport {
                    power_w: ScalarObservation::available(2.5, 42),
                    energy_j: ScalarObservation::available(3_000, 42),
                },
            },
        ]);
    let serialized = serde_json::to_string(&snapshot).expect("serialize the inventory");
    let restored: NpuInventorySnapshot =
        serde_json::from_str(&serialized).expect("deserialize the inventory");
    let report = restored
        .power_for(&accelerator.device_id)
        .expect("the published inventory keeps its power observation");
    assert_eq!(report.power_w.current_value().copied(), Some(2.5));
    assert_eq!(report.energy_j.current_value().copied(), Some(3_000));
}

#[test]
fn a_failed_inventory_carries_no_power_observation() {
    let snapshot = NpuInventorySnapshot::failed(FailureKind::TemporarilyUnavailable, "boom", 42);
    assert!(!snapshot.is_success());
    assert!(
        snapshot.power.is_empty(),
        "a failed enumeration carries no power observation"
    );
}
