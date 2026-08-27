use super::*;
use crate::core::{
    DeviceRefreshOutcome, DeviceState, SensorCenterSnapshot, SensorLifecycleTracker,
};

fn zone(device_id: DeviceId) -> ThermalZoneStatus {
    ThermalZoneStatus {
        id: format!("{}:zone:fixture", device_id.as_str()),
        device_id,
        device_generation: Default::default(),
        label: ScalarObservation::available("fixture-zone".to_owned(), 10),
        mode: ScalarObservation::available(ThermalZoneMode::Enabled, 10),
        policy: ScalarObservation::available(ThermalPolicy::StepWise, 10),
        trip_points: ThermalTripPointSet::available(Vec::new(), 10),
    }
}

fn cooling(device_id: DeviceId) -> ThermalCoolingDeviceStatus {
    ThermalCoolingDeviceStatus {
        id: format!("{}:channel:fixture", device_id.as_str()),
        device_id,
        device_generation: Default::default(),
        kind: ScalarObservation::available(ThermalCoolingKind::Fan, 10),
        current_state: ScalarObservation::available(0, 10),
        maximum_state: ScalarObservation::available(1, 10),
        activity: ScalarObservation::available(ThermalCoolingActivity::Inactive, 10),
    }
}

fn snapshot(
    now_ms: u64,
    zone: ThermalZoneStatus,
    cooling: ThermalCoolingDeviceStatus,
) -> SensorCenterSnapshot {
    SensorCenterSnapshot {
        state: DeviceState::healthy(now_ms),
        timestamp_ms: now_ms,
        readings: Vec::new(),
        thermal_control: ThermalControlSnapshot {
            zones: vec![zone],
            cooling_devices: vec![cooling],
            throttle: Default::default(),
        },
        device_lifecycles: Default::default(),
    }
}

#[test]
fn control_sidecars_share_physical_lifecycle_without_becoming_readings() {
    let zone_id = DeviceId::new("thermal:type:fixture-zone");
    let cooling_id = DeviceId::new("cooling:device:fixture-fan");
    let mut tracker = SensorLifecycleTracker::new(100);
    let mut first = snapshot(10, zone(zone_id.clone()), cooling(cooling_id.clone()));

    tracker.reconcile_discovered(
        &mut first,
        &[zone_id.clone(), cooling_id.clone()],
        DeviceRefreshOutcome::Complete,
    );

    assert!(first.readings.is_empty());
    assert_eq!(
        first.thermal_control.zones[0].device_generation,
        DeviceGeneration::INITIAL
    );
    assert_eq!(
        first.thermal_control.cooling_devices[0].device_generation,
        DeviceGeneration::INITIAL
    );

    let mut absent = SensorCenterSnapshot {
        state: DeviceState::healthy(20),
        timestamp_ms: 20,
        ..Default::default()
    };
    tracker.reconcile_discovered(&mut absent, &[], DeviceRefreshOutcome::Complete);

    let mut readded = snapshot(30, zone(zone_id.clone()), cooling(cooling_id.clone()));
    tracker.reconcile_discovered(
        &mut readded,
        &[zone_id, cooling_id],
        DeviceRefreshOutcome::Complete,
    );
    assert_eq!(readded.thermal_control.zones[0].device_generation.get(), 2);
    assert_eq!(
        readded.thermal_control.cooling_devices[0]
            .device_generation
            .get(),
        2
    );
}
