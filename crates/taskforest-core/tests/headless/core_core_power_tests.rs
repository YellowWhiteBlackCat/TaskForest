use super::*;
use crate::core::FailureKind;
use crate::core::device_state::{DevicePresence, DeviceStatus};

fn snapshot(timestamp_ms: u64, batteries: Vec<BatteryInfo>) -> PowerSupplySnapshot {
    PowerSupplySnapshot {
        state: DeviceState::healthy(timestamp_ms),
        timestamp_ms,
        batteries,
        device_lifecycles: HashMap::new(),
    }
}

fn battery_with_capacity(id: &str, observed_at_ms: u64, capacity_pct: u8) -> BatteryInfo {
    let mut battery = BatteryInfo {
        id: id.into(),
        device_state: DeviceState::healthy(observed_at_ms),
        ..Default::default()
    };
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(capacity_pct, observed_at_ms),
        ..Default::default()
    });
    battery
}

fn legacy_battery_wire(id: &str, device_state: DeviceState) -> serde_json::Value {
    let mut wire =
        serde_json::to_value(BatteryInfo::default()).expect("serialize battery wire shape");
    let object = wire.as_object_mut().expect("battery JSON object");
    object.insert("id".into(), serde_json::json!(id));
    object.insert(
        "device_state".into(),
        serde_json::to_value(device_state).expect("serialize device state"),
    );
    object.insert("capacity_pct".into(), serde_json::json!(0));
    object.insert("voltage_uv".into(), serde_json::json!(0));
    object.insert("power_w".into(), serde_json::json!(0.0));
    object.insert("cycle_count".into(), serde_json::json!(0));
    object.remove("scalar_observations");
    wire
}

#[test]
fn schema_v1_battery_defaults_kind_while_ups_round_trips_explicitly() {
    let mut legacy =
        serde_json::to_value(BatteryInfo::default()).expect("serialize battery fixture");
    legacy
        .as_object_mut()
        .expect("battery fixture is an object")
        .remove("kind");
    let decoded: BatteryInfo =
        serde_json::from_value(legacy).expect("schema-v1 battery remains readable");
    assert_eq!(decoded.kind, PowerSupplyKind::Battery);

    let ups = BatteryInfo {
        kind: PowerSupplyKind::UninterruptiblePowerSupply,
        ..Default::default()
    };
    let round_trip: BatteryInfo =
        serde_json::from_value(serde_json::to_value(&ups).expect("serialize UPS fixture"))
            .expect("deserialize UPS fixture");
    assert_eq!(round_trip.kind, PowerSupplyKind::UninterruptiblePowerSupply);
}

#[test]
fn battery_reappearance_advances_generation_without_replaying_absent_value() {
    let mut tracker = PowerSupplyLifecycleTracker::new(100);
    let battery = battery_with_capacity("power-supply:BAT0", 10, 80);
    let mut first = snapshot(10, vec![battery.clone()]);
    tracker.reconcile(&mut first, DeviceRefreshOutcome::Complete);
    assert_eq!(first.batteries[0].device_generation.get(), 1);
    assert_eq!(
        first
            .device_lifecycles
            .get("power-supply:BAT0")
            .map(|lifecycle| lifecycle.generation),
        Some(DeviceGeneration::INITIAL)
    );

    let mut absent = snapshot(20, Vec::new());
    tracker.reconcile(&mut absent, DeviceRefreshOutcome::Complete);
    assert!(absent.batteries.is_empty());
    assert_eq!(
        absent
            .device_lifecycles
            .get("power-supply:BAT0")
            .map(|entry| entry.presence),
        Some(DevicePresence::Absent)
    );

    let mut returned = snapshot(30, vec![battery]);
    tracker.reconcile(&mut returned, DeviceRefreshOutcome::Complete);
    assert_eq!(returned.batteries[0].device_generation.get(), 2);
    assert_eq!(
        returned.batteries[0].device_state.status,
        DeviceStatus::Healthy
    );

    let lifecycle = tracker.lifecycle("power-supply:BAT0");
    assert_eq!(
        lifecycle.map(|entry| entry.presence),
        Some(DevicePresence::Present)
    );
}

#[test]
fn provider_unavailable_never_falsely_marks_battery_absent_or_zero() {
    let mut tracker = PowerSupplyLifecycleTracker::new(100);
    let mut first = snapshot(
        10,
        vec![BatteryInfo {
            id: "power-supply:BAT0".into(),
            device_state: DeviceState::healthy(10),
            ..Default::default()
        }],
    );
    tracker.reconcile(&mut first, DeviceRefreshOutcome::Complete);

    let mut unavailable = snapshot(20, Vec::new());
    unavailable.state = DeviceState {
        status: DeviceStatus::PermissionDenied,
        last_success_ms: None,
    };
    let delta = tracker.reconcile(
        &mut unavailable,
        DeviceRefreshOutcome::Unavailable(DeviceStatus::PermissionDenied),
    );

    assert!(delta.newly_absent.is_empty());
    assert!(unavailable.batteries.is_empty());
    let lifecycle = unavailable
        .device_lifecycles
        .get("power-supply:BAT0")
        .expect("provider outage retains device identity");
    assert_eq!(lifecycle.presence, DevicePresence::Unavailable);
    assert_eq!(lifecycle.state.last_success_ms, Some(10));
}

#[test]
fn retained_battery_row_cannot_override_explicit_discovery_absence() {
    let mut tracker = PowerSupplyLifecycleTracker::new(100);
    let device_id = crate::core::DeviceId::new("power-supply:BAT0");
    let mut first = snapshot(10, vec![battery_with_capacity(device_id.as_str(), 10, 80)]);
    tracker.reconcile_discovered(
        &mut first,
        std::slice::from_ref(&device_id),
        DeviceRefreshOutcome::Complete,
    );

    let mut retained = first;
    retained.timestamp_ms = 20;
    let delta = tracker.reconcile_discovered(&mut retained, &[], DeviceRefreshOutcome::Complete);

    assert_eq!(delta.newly_absent, [device_id]);
    assert_eq!(
        retained.batteries[0].device_state.status,
        DeviceStatus::Stale
    );
    assert_eq!(
        retained
            .device_lifecycles
            .get("power-supply:BAT0")
            .map(|lifecycle| lifecycle.presence),
        Some(DevicePresence::Absent)
    );
}

#[test]
fn legacy_options_require_current_device_identity_and_success_time() {
    let stale_state = DeviceState {
        status: DeviceStatus::Stale,
        last_success_ms: Some(10),
    };
    let healthy_without_success = DeviceState {
        status: DeviceStatus::Healthy,
        last_success_ms: None,
    };
    for untrusted_wire in [
        legacy_battery_wire("", DeviceState::healthy(10)),
        legacy_battery_wire("power-supply:BAT0", stale_state),
        legacy_battery_wire("power-supply:BAT0", healthy_without_success),
    ] {
        let legacy_values: BatteryInfo =
            serde_json::from_value(untrusted_wire).expect("decode schema-v1 battery");
        assert_eq!(legacy_values.current_capacity_pct(), None);
        assert_eq!(legacy_values.current_voltage_uv(), None);
        assert_eq!(legacy_values.current_power_w(), None);
        assert_eq!(legacy_values.current_cycle_count(), None);
    }

    let mut battery: BatteryInfo = serde_json::from_value(legacy_battery_wire(
        "power-supply:BAT0",
        DeviceState::healthy(10),
    ))
    .expect("decode trusted schema-v1 battery");

    assert_eq!(battery.current_capacity_pct(), Some(0));
    assert_eq!(battery.current_voltage_uv(), Some(0));
    assert_eq!(battery.current_power_w(), Some(0.0));
    assert_eq!(battery.current_cycle_count(), Some(0));

    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::unavailable(FailureKind::Unsupported),
        voltage_uv: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        power_w: ScalarObservation::available(12.5, 10)
            .transition_failure(FailureKind::TemporarilyUnavailable),
        cycle_count: ScalarObservation::unavailable(FailureKind::ProviderFault),
        ..Default::default()
    });

    assert_eq!(battery.current_capacity_pct(), None);
    assert_eq!(battery.current_voltage_uv(), None);
    assert_eq!(battery.current_power_w(), None);
    assert_eq!(battery.current_cycle_count(), None);
    assert_eq!(
        battery.scalar_observations().power_w.last_known_value(),
        Some(&12.5)
    );
}

#[test]
fn typed_truth_wins_over_conflicting_legacy_wire_values() {
    let mut battery = battery_with_capacity("power-supply:BAT0", 10, 64);
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(64, 10),
        voltage_uv: ScalarObservation::partial(12_000_000, 10, FailureKind::ProviderFault),
        power_w: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        cycle_count: ScalarObservation::available(8, 10)
            .transition_failure(FailureKind::TemporarilyUnavailable),
        ..Default::default()
    });
    let mut wire = serde_json::to_value(&battery).expect("serialize typed battery");
    let object = wire.as_object_mut().expect("battery JSON object");
    object.insert("capacity_pct".into(), serde_json::json!(99));
    object.insert("voltage_uv".into(), serde_json::json!(99));
    object.insert("power_w".into(), serde_json::json!(99.0));
    object.insert("cycle_count".into(), serde_json::json!(99));

    let decoded: BatteryInfo = serde_json::from_value(wire).expect("read mixed battery wire");
    assert_eq!(decoded.current_capacity_pct(), Some(64));
    assert_eq!(decoded.current_voltage_uv(), Some(12_000_000));
    assert_eq!(decoded.current_power_w(), None);
    assert_eq!(decoded.current_cycle_count(), None);

    let encoded = serde_json::to_value(decoded).expect("serialize canonical battery");
    assert_eq!(encoded["capacity_pct"], 64);
    assert_eq!(encoded["voltage_uv"], serde_json::Value::Null);
    assert_eq!(encoded["power_w"], serde_json::Value::Null);
    assert_eq!(encoded["cycle_count"], serde_json::Value::Null);
}

#[test]
fn typed_only_battery_round_trip_projects_only_available_legacy_values() {
    let typed_scalars = BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(0, 10),
        ..Default::default()
    };
    let typed_only_wire = serde_json::json!({
        "id": "power-supply:BAT0",
        "kind": "battery",
        "display_name": "BAT0",
        "device_generation": 0,
        "device_state": {"status": "healthy", "last_success_ms": 10},
        "status": "Discharging",
        "technology": "",
        "model_name": "",
        "manufacturer": "",
        "scalar_observations": typed_scalars,
    });

    let decoded: BatteryInfo =
        serde_json::from_value(typed_only_wire).expect("decode typed-only battery");
    assert_eq!(decoded.current_capacity_pct(), Some(0));
    assert_eq!(decoded.current_voltage_uv(), None);

    let projected = serde_json::to_value(decoded).expect("serialize canonical battery");
    assert_eq!(projected["capacity_pct"], 0);
    assert_eq!(projected["voltage_uv"], serde_json::Value::Null);
}

#[test]
fn field_failure_retains_prior_value_only_as_stale() {
    let id = crate::core::DeviceId::new("power-supply:BAT0");
    let mut tracker = PowerSupplyLifecycleTracker::new(100);
    let mut first_battery = BatteryInfo {
        id: id.as_str().to_owned(),
        device_state: DeviceState::healthy(10),
        ..Default::default()
    };
    first_battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(0, 10),
        voltage_uv: ScalarObservation::available(12_000_000, 10),
        power_w: ScalarObservation::available(0.0, 10),
        cycle_count: ScalarObservation::available(0, 10),
        ..Default::default()
    });
    let mut first = snapshot(10, vec![first_battery]);
    tracker.reconcile_discovered(
        &mut first,
        std::slice::from_ref(&id),
        DeviceRefreshOutcome::Complete,
    );

    let mut failed_battery = BatteryInfo {
        id: id.as_str().to_owned(),
        device_state: DeviceState::healthy(20),
        ..Default::default()
    };
    failed_battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        voltage_uv: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        power_w: ScalarObservation::unavailable(FailureKind::ProviderFault),
        cycle_count: ScalarObservation::unavailable(FailureKind::Unsupported),
        ..Default::default()
    });
    let mut failed = snapshot(20, vec![failed_battery]);
    tracker.reconcile_discovered(
        &mut failed,
        std::slice::from_ref(&id),
        DeviceRefreshOutcome::Complete,
    );

    let observations = *failed.batteries[0].scalar_observations();
    assert_eq!(
        observations.capacity_pct.availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(observations.capacity_pct.last_known_value(), Some(&0));
    assert_eq!(observations.capacity_pct.last_success_ms(), Some(10));
    assert_eq!(failed.batteries[0].current_capacity_pct(), None);
    assert_eq!(
        serde_json::to_value(&failed.batteries[0]).expect("serialize retained battery")["capacity_pct"],
        serde_json::Value::Null
    );
    assert_eq!(
        observations.voltage_uv.availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        observations.power_w.availability(),
        ScalarAvailability::Stale(FailureKind::ProviderFault)
    );
    assert_eq!(
        observations.cycle_count.availability(),
        ScalarAvailability::Stale(FailureKind::Unsupported)
    );
}

#[test]
fn confirmed_absence_prevents_scalar_replay_into_new_generation() {
    let id = crate::core::DeviceId::new("power-supply:BAT0");
    let mut tracker = PowerSupplyLifecycleTracker::new(100);
    let mut battery = BatteryInfo {
        id: id.as_str().to_owned(),
        device_state: DeviceState::healthy(10),
        ..Default::default()
    };
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(80, 10),
        ..Default::default()
    });
    let mut first = snapshot(10, vec![battery]);
    tracker.reconcile_discovered(
        &mut first,
        std::slice::from_ref(&id),
        DeviceRefreshOutcome::Complete,
    );
    let mut absent = snapshot(20, Vec::new());
    tracker.reconcile_discovered(&mut absent, &[], DeviceRefreshOutcome::Complete);

    let mut returned_battery = BatteryInfo {
        id: id.as_str().to_owned(),
        device_state: DeviceState::healthy(30),
        ..Default::default()
    };
    returned_battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        ..Default::default()
    });
    let mut returned = snapshot(30, vec![returned_battery]);
    tracker.reconcile_discovered(
        &mut returned,
        std::slice::from_ref(&id),
        DeviceRefreshOutcome::Complete,
    );

    assert_eq!(returned.batteries[0].device_generation.get(), 2);
    assert_eq!(
        returned.batteries[0]
            .scalar_observations()
            .capacity_pct
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        returned.batteries[0]
            .scalar_observations()
            .capacity_pct
            .last_known_value(),
        None
    );
}

/// The degradation-health rule is owned by core and stays honest: a pure
/// full/design ratio when both facts are current, typed unavailability for
/// every missing/zero-denominator case — never a fabricated 0% or 100%.
#[test]
fn health_pct_is_a_pure_ratio_that_stays_unavailable_without_both_facts() {
    let both_current = BatteryScalarObservations {
        energy_full_uwh: ScalarObservation::available(49_000_000.0, 30),
        energy_full_design_uwh: ScalarObservation::available(56_000_000.0, 20),
        ..Default::default()
    };
    // 49/56 Wh × 100 = 87.5; the success time is the OLDER input.
    assert_eq!(
        both_current.health_pct(),
        ScalarObservation::available(87.5, 20)
    );

    // Zero design capacity is a provider fault, not a divide-by-zero 0%.
    let zero_design = BatteryScalarObservations {
        energy_full_uwh: ScalarObservation::available(49_000_000.0, 30),
        energy_full_design_uwh: ScalarObservation::available(0.0, 20),
        ..Default::default()
    };
    assert_eq!(
        zero_design.health_pct().availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );

    // A missing denominator carries its own failure as the reason.
    let missing_design = BatteryScalarObservations {
        energy_full_uwh: ScalarObservation::available(49_000_000.0, 30),
        energy_full_design_uwh: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        ..Default::default()
    };
    assert_eq!(
        missing_design.health_pct().availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );

    // Never-observed inputs (legacy payloads) decode as Unsupported.
    assert_eq!(
        BatteryScalarObservations::default()
            .health_pct()
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );

    // A partial numerator degrades the ratio to partial with its failure
    // while keeping the value current.
    let partial_numerator = BatteryScalarObservations {
        energy_full_uwh: ScalarObservation::partial(49_000_000.0, 30, FailureKind::ProviderFault),
        energy_full_design_uwh: ScalarObservation::available(56_000_000.0, 20),
        ..Default::default()
    };
    assert_eq!(
        partial_numerator.health_pct().availability(),
        ScalarAvailability::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(partial_numerator.health_pct().current_value(), Some(&87.5));
}

/// Payloads written before the health/time facts existed must keep decoding:
/// the absent fields resolve to `Unknown`, which every current-value reader
/// hides — no fabricated estimate survives an upgrade.
#[test]
fn health_and_time_facts_absent_from_legacy_payloads_stay_unknown() {
    let mut battery = battery_with_capacity("power-supply:BAT0", 10, 80);
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(80, 10),
        energy_full_uwh: ScalarObservation::available(49_000_000.0, 10),
        energy_full_design_uwh: ScalarObservation::available(56_000_000.0, 10),
        time_to_empty_secs: ScalarObservation::available(3_780.0, 10),
        time_to_full_secs: ScalarObservation::available(7_200.0, 10),
        ..Default::default()
    });
    let mut wire = serde_json::to_value(&battery).expect("serialize typed battery");
    let scalar = wire
        .get_mut("scalar_observations")
        .expect("scalar group")
        .as_object_mut()
        .expect("scalar group object");
    for key in [
        "energy_full_uwh",
        "energy_full_design_uwh",
        "time_to_empty_secs",
        "time_to_full_secs",
    ] {
        scalar.remove(key);
    }

    let decoded: BatteryInfo =
        serde_json::from_value(wire).expect("pre-health battery payload decodes");
    assert_eq!(decoded.current_capacity_pct(), Some(80));
    assert_eq!(decoded.current_energy_full_uwh(), None);
    assert_eq!(decoded.current_energy_full_design_uwh(), None);
    assert_eq!(decoded.current_health_pct(), None);
    assert_eq!(decoded.current_time_to_empty_secs(), None);
    assert_eq!(decoded.current_time_to_full_secs(), None);
}
