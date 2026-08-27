use super::{
    BatteryInfo, DeviceRefreshOutcome, DeviceState, DeviceStatus, PowerSupplyLifecycleTracker,
    PowerSupplySnapshot,
};
use crate::core::device_state::DevicePresence;

fn snapshot(timestamp_ms: u64, batteries: Vec<BatteryInfo>) -> PowerSupplySnapshot {
    PowerSupplySnapshot {
        state: DeviceState::healthy(timestamp_ms),
        timestamp_ms,
        batteries,
        ..Default::default()
    }
}

#[test]
fn legacy_wire_value_bridges_only_with_a_healthy_identity() {
    let mut wire = serde_json::to_value(BatteryInfo::default()).expect("serialize battery shape");
    let object = wire.as_object_mut().expect("battery JSON object");
    object.insert("id".into(), serde_json::json!("power-supply:BAT0"));
    object.insert(
        "device_state".into(),
        serde_json::to_value(DeviceState::healthy(1_720_000_000)).expect("serialize healthy state"),
    );
    object.insert("capacity_pct".into(), serde_json::json!(80));
    object.remove("scalar_observations");

    let bridged: BatteryInfo = serde_json::from_value(wire).expect("decode legacy battery");
    assert_eq!(bridged.current_capacity_pct(), Some(80));
}

#[test]
fn observed_battery_keeps_its_real_state_over_the_discovered_default() {
    // A battery that IS observed must not be overwritten by the
    // discovered-devices default pass (a `delete !` mutation of the
    // guard would clobber its real state).
    let mut tracker = PowerSupplyLifecycleTracker::new(100);
    let battery = BatteryInfo {
        id: "power-supply:BAT0".into(),
        device_state: DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: None,
        },
        ..Default::default()
    };
    let mut first = snapshot(10, vec![battery.clone()]);
    tracker.reconcile(&mut first, DeviceRefreshOutcome::Complete);
    assert_eq!(
        first.batteries[0].device_state.status,
        DeviceStatus::Stale,
        "observed battery keeps its real (stale) state, not the discovered default"
    );
    assert_eq!(
        tracker.lifecycle("power-supply:BAT0").map(|l| l.presence),
        Some(DevicePresence::Present)
    );
}
