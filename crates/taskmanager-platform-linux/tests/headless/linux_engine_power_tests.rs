use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

struct FixtureDir(std::path::PathBuf);

impl FixtureDir {
    fn new() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = crate::test_support::repo_temp_dir().join(format!(
            "taskmanager-power-fixture-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("fixture directory");
        Self(path)
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn discovers_multiple_batteries_and_keeps_unknown_capacity_as_none() {
    let fixture = FixtureDir::new();
    for (name, serial, capacity) in [
        ("BAT0", "serial-a", Some("75")),
        ("BAT1", "serial-b", None),
        ("BAT2", "serial-c", Some("101")),
    ] {
        let dir = fixture.0.join(name);
        fs::create_dir_all(&dir).expect("battery directory");
        fs::write(dir.join("type"), "Battery\n").expect("battery type");
        fs::write(dir.join("serial_number"), serial).expect("battery serial");
        if let Some(capacity) = capacity {
            fs::write(dir.join("capacity"), capacity).expect("battery capacity");
        }
    }

    let snapshot = collect_power_supplies_from(&fixture.0, 10);

    assert_eq!(snapshot.value.batteries.len(), 3);
    assert_eq!(snapshot.value.batteries[0].current_capacity_pct(), Some(75));
    assert_eq!(snapshot.value.batteries[1].current_capacity_pct(), None);
    assert_eq!(snapshot.value.batteries[2].current_capacity_pct(), None);
    assert_eq!(
        snapshot.value.batteries[0]
            .scalar_observations()
            .capacity_pct,
        ScalarObservation::available(75, 10)
    );
    assert_eq!(
        snapshot.value.batteries[1]
            .scalar_observations()
            .capacity_pct
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(
        snapshot.value.batteries[2]
            .scalar_observations()
            .capacity_pct
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(snapshot.value.batteries[0].display_name, "BAT0");
    assert_eq!(snapshot.discovery().outcome, SourceOutcome::Available);
    assert_eq!(snapshot.discovered_devices().len(), 3);
    assert_eq!(snapshot.enrichments.len(), 1);
    assert_eq!(
        snapshot.enrichments[0].provider.as_str(),
        "linux.power-supply.scalars"
    );
    assert_eq!(
        snapshot.enrichments[0].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(snapshot.enrichments[0].item_count, 1);
}

#[test]
fn discovers_ups_without_treating_mains_or_usb_sources_as_batteries() {
    let fixture = FixtureDir::new();
    for (name, supply_type) in [("UPS0", "UPS"), ("AC0", "Mains"), ("USB0", "USB")] {
        let dir = fixture.0.join(name);
        fs::create_dir_all(&dir).expect("power-supply directory");
        fs::write(dir.join("type"), supply_type).expect("power-supply type");
        fs::write(dir.join("capacity"), "0\n").expect("power-supply capacity");
    }

    let snapshot = collect_power_supplies_from(&fixture.0, 42);

    assert_eq!(snapshot.value.batteries.len(), 1);
    assert_eq!(snapshot.value.batteries[0].display_name, "UPS0");
    assert_eq!(
        snapshot.value.batteries[0].kind,
        PowerSupplyKind::UninterruptiblePowerSupply
    );
    assert_eq!(snapshot.value.batteries[0].current_capacity_pct(), Some(0));
    assert_eq!(snapshot.discovery().item_count, 1);
}

#[test]
fn duplicate_serials_are_not_disambiguated_by_enumeration_names() {
    let fixture = FixtureDir::new();
    for name in ["BAT0", "BAT1"] {
        let dir = fixture.0.join(name);
        fs::create_dir_all(&dir).expect("battery directory");
        fs::write(dir.join("type"), "Battery\n").expect("battery type");
        fs::write(dir.join("serial_number"), "same-serial").expect("battery serial");
        fs::write(dir.join("manufacturer"), "ACME").expect("manufacturer");
        fs::write(dir.join("model_name"), "Pack").expect("model");
    }

    let snapshot = collect_power_supplies_from(&fixture.0, 10);
    assert_eq!(snapshot.value.batteries.len(), 1);
    assert_eq!(
        snapshot.discovery().outcome,
        SourceOutcome::Partial(FailureKind::Unsupported)
    );
}

#[test]
fn missing_provider_root_is_typed_unsupported_not_empty() {
    let fixture = FixtureDir::new();
    let missing = fixture.0.join("missing");
    let snapshot = collect_power_supplies_from(&missing, 10);

    assert_eq!(
        snapshot.discovery().outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(snapshot.value.state.status, DeviceStatus::Unsupported);
}

#[test]
fn measured_zero_is_available_for_every_numeric_field() {
    let fixture = FixtureDir::new();
    let dir = fixture.0.join("BAT0");
    fs::create_dir_all(&dir).expect("battery directory");
    for (name, value) in [
        ("type", "Battery\n"),
        ("status", "Discharging\n"),
        ("capacity", "0\n"),
        ("voltage_now", "0\n"),
        ("power_now", "0\n"),
        ("cycle_count", "0\n"),
        ("energy_full", "0\n"),
        ("energy_full_design", "0\n"),
        ("time_to_empty_now", "0\n"),
    ] {
        fs::write(dir.join(name), value).expect("battery field");
    }

    let snapshot = collect_power_supplies_from(&fixture.0, 42);
    let battery = &snapshot.value.batteries[0];
    assert_eq!(battery.current_capacity_pct(), Some(0));
    assert_eq!(battery.current_voltage_uv(), Some(0));
    assert_eq!(battery.current_power_w(), Some(0.0));
    assert_eq!(battery.current_cycle_count(), Some(0));
    assert_eq!(battery.current_energy_full_uwh(), Some(0.0));
    assert_eq!(battery.current_energy_full_design_uwh(), Some(0.0));
    // A kernel-reported zero-minute estimate is a measurement, not a
    // fabricated one; the status-gated twin stays unavailable.
    assert_eq!(battery.current_time_to_empty_secs(), Some(0.0));
    assert_eq!(battery.current_time_to_full_secs(), None);
    assert_eq!(
        battery.scalar_observations().capacity_pct.last_success_ms(),
        Some(42)
    );
    assert_eq!(
        battery.scalar_observations().voltage_uv.last_success_ms(),
        Some(42)
    );
    assert_eq!(
        battery.scalar_observations().power_w.last_success_ms(),
        Some(42)
    );
    assert_eq!(
        battery.scalar_observations().cycle_count.last_success_ms(),
        Some(42)
    );
    assert_eq!(
        battery
            .scalar_observations()
            .energy_full_uwh
            .last_success_ms(),
        Some(42)
    );
    assert_eq!(
        battery
            .scalar_observations()
            .time_to_empty_secs
            .last_success_ms(),
        Some(42)
    );
    // Zero design capacity keeps the derived health honestly unavailable.
    assert_eq!(battery.current_health_pct(), None);
    // Six always-on slots plus the status-applicable empty estimate.
    assert_eq!(snapshot.enrichments[0].outcome, SourceOutcome::Available);
    assert_eq!(snapshot.enrichments[0].item_count, 7);
    assert_eq!(snapshot.value.state, DeviceState::healthy(42));
}

#[test]
fn malformed_numeric_nodes_are_provider_faults_not_unknown_values() {
    let fixture = FixtureDir::new();
    let dir = fixture.0.join("BAT0");
    fs::create_dir_all(&dir).expect("battery directory");
    fs::write(dir.join("type"), "Battery\n").expect("battery type");
    for name in [
        "capacity",
        "voltage_now",
        "power_now",
        "cycle_count",
        "energy_full",
        "energy_full_design",
    ] {
        fs::write(dir.join(name), "not-a-number\n").expect("malformed battery field");
    }

    let snapshot = collect_power_supplies_from(&fixture.0, 42);
    let observations = *snapshot.value.batteries[0].scalar_observations();
    for availability in [
        observations.capacity_pct.availability(),
        observations.voltage_uv.availability(),
        observations.power_w.availability(),
        observations.cycle_count.availability(),
        observations.energy_full_uwh.availability(),
        observations.energy_full_design_uwh.availability(),
    ] {
        assert_eq!(
            availability,
            ScalarAvailability::Unavailable(FailureKind::ProviderFault)
        );
    }
    assert_eq!(
        snapshot.enrichments[0].outcome,
        SourceOutcome::Unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(snapshot.enrichments[0].item_count, 0);
    assert_eq!(snapshot.value.state.status, DeviceStatus::Stale);
}

#[test]
fn current_voltage_fallback_reports_partial_when_direct_power_faults() {
    let fixture = FixtureDir::new();
    let dir = fixture.0.join("BAT0");
    fs::create_dir_all(&dir).expect("battery directory");
    for (name, value) in [
        ("type", "Battery\n"),
        ("voltage_now", "12000000\n"),
        ("current_now", "2000000\n"),
        ("power_now", "malformed\n"),
    ] {
        fs::write(dir.join(name), value).expect("battery field");
    }

    let snapshot = collect_power_supplies_from(&fixture.0, 42);
    let power = snapshot.value.batteries[0].scalar_observations().power_w;
    assert_eq!(power.last_known_value(), Some(&24.0));
    assert_eq!(
        power.availability(),
        ScalarAvailability::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(power.last_success_ms(), Some(42));
    assert_eq!(snapshot.value.batteries[0].current_power_w(), Some(24.0));
    assert_eq!(
        snapshot.enrichments[0].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(snapshot.enrichments[0].item_count, 2);
}

#[test]
fn unsupported_direct_power_uses_current_voltage_as_available_fallback() {
    let fixture = FixtureDir::new();
    let dir = fixture.0.join("BAT0");
    fs::create_dir_all(&dir).expect("battery directory");
    for (name, value) in [
        ("type", "Battery\n"),
        ("voltage_now", "12000000\n"),
        ("current_now", "2000000\n"),
    ] {
        fs::write(dir.join(name), value).expect("battery field");
    }

    let snapshot = collect_power_supplies_from(&fixture.0, 42);
    assert_eq!(
        snapshot.value.batteries[0].scalar_observations().power_w,
        ScalarObservation::available(24.0, 42)
    );
}

#[test]
fn absurd_power_value_is_rejected_instead_of_lossily_cast() {
    let fixture = FixtureDir::new();
    let dir = fixture.0.join("BAT0");
    fs::create_dir_all(&dir).expect("battery directory");
    fs::write(dir.join("type"), "Battery\n").expect("battery type");
    fs::write(dir.join("power_now"), u64::MAX.to_string()).expect("absurd power");

    let snapshot = collect_power_supplies_from(&fixture.0, 42);
    let battery = &snapshot.value.batteries[0];
    assert_eq!(battery.current_power_w(), None);
    assert_eq!(
        battery.scalar_observations().power_w.availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(
        battery.scalar_observations().power_w.last_success_ms(),
        None
    );
}

#[test]
fn discovered_battery_with_no_numeric_nodes_degrades_scalar_health_only() {
    let fixture = FixtureDir::new();
    let dir = fixture.0.join("BAT0");
    fs::create_dir_all(&dir).expect("battery directory");
    fs::write(dir.join("type"), "Battery\n").expect("battery type");

    let snapshot = collect_power_supplies_from(&fixture.0, 42);
    assert_eq!(
        snapshot.discovery().outcome,
        SourceOutcome::Partial(FailureKind::Unsupported)
    );
    assert_eq!(snapshot.discovered_devices().len(), 1);
    assert!(!snapshot.discovery_is_authoritative());
    assert_eq!(
        snapshot.enrichments[0].outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(snapshot.enrichments[0].item_count, 0);
    assert_eq!(snapshot.value.state.status, DeviceStatus::Unsupported);
}

#[test]
fn scalar_io_error_kinds_map_to_provider_neutral_failures() {
    for (kind, expected) in [
        (ErrorKind::NotFound, FailureKind::Unsupported),
        (ErrorKind::Unsupported, FailureKind::Unsupported),
        (ErrorKind::PermissionDenied, FailureKind::PermissionDenied),
        (ErrorKind::Interrupted, FailureKind::TemporarilyUnavailable),
        (ErrorKind::WouldBlock, FailureKind::TemporarilyUnavailable),
        (ErrorKind::TimedOut, FailureKind::TemporarilyUnavailable),
        (ErrorKind::InvalidData, FailureKind::ProviderFault),
    ] {
        assert_eq!(classify_scalar_io_error(kind), expected);
    }
}

#[test]
fn scalar_source_uses_success_count_and_strongest_failure() {
    let mut battery = BatteryInfo::default();
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(50, 42),
        voltage_uv: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        power_w: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        cycle_count: ScalarObservation::unavailable(FailureKind::Unsupported),
        ..Default::default()
    });

    let source = power_scalar_source_status(&[battery]);
    assert_eq!(source.item_count, 1);
    assert_eq!(
        source.outcome,
        SourceOutcome::Partial(FailureKind::PermissionDenied)
    );

    let mut denied = BatteryInfo::default();
    denied.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        voltage_uv: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        power_w: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        cycle_count: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        ..Default::default()
    });
    let source = power_scalar_source_status(&[denied]);
    assert_eq!(source.item_count, 0);
    assert_eq!(
        source.outcome,
        SourceOutcome::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(
        power_snapshot_state(source.outcome, 42).status,
        DeviceStatus::PermissionDenied
    );
}

/// Charge-reporting drivers expose µAh pairs instead of µWh nodes; the
/// conversion with the live voltage yields the same degradation facts (and
/// the core health rule derives from them, never a provider health).
#[test]
fn charge_full_pair_converts_to_micro_watt_hours_with_the_live_voltage() {
    let fixture = FixtureDir::new();
    let dir = fixture.0.join("BAT0");
    fs::create_dir_all(&dir).expect("battery directory");
    for (name, value) in [
        ("type", "Battery\n"),
        ("voltage_now", "14000000\n"),
        ("charge_full", "3500000\n"),
        ("charge_full_design", "4000000\n"),
    ] {
        fs::write(dir.join(name), value).expect("battery field");
    }

    let snapshot = collect_power_supplies_from(&fixture.0, 42);
    let battery = &snapshot.value.batteries[0];
    assert_eq!(battery.current_energy_full_uwh(), Some(49_000_000.0));
    assert_eq!(battery.current_energy_full_design_uwh(), Some(56_000_000.0));
    assert_eq!(
        battery.scalar_observations().energy_full_uwh,
        ScalarObservation::available(49_000_000.0, 42)
    );
    assert_eq!(battery.current_health_pct(), Some(87.5));
}

/// Drivers without an instant voltage reading still convert through the
/// documented `constant_charge_voltage_now` node.
#[test]
fn charge_pair_without_voltage_now_uses_constant_charge_voltage() {
    let fixture = FixtureDir::new();
    let dir = fixture.0.join("BAT0");
    fs::create_dir_all(&dir).expect("battery directory");
    for (name, value) in [
        ("type", "Battery\n"),
        ("constant_charge_voltage_now", "12000000\n"),
        ("charge_full", "2500000\n"),
    ] {
        fs::write(dir.join(name), value).expect("battery field");
    }

    let snapshot = collect_power_supplies_from(&fixture.0, 42);
    let battery = &snapshot.value.batteries[0];
    assert_eq!(battery.current_energy_full_uwh(), Some(30_000_000.0));
    // No design fact → the derived health stays honestly absent.
    assert_eq!(battery.current_health_pct(), None);
}

/// Batteries without any energy/charge nodes report typed unavailability,
/// not a collection failure — the source stays Partial, never Stale.
#[test]
fn missing_energy_nodes_are_typed_unavailable_not_collection_failures() {
    let fixture = FixtureDir::new();
    let dir = fixture.0.join("BAT0");
    fs::create_dir_all(&dir).expect("battery directory");
    fs::write(dir.join("type"), "Battery\n").expect("battery type");
    fs::write(dir.join("capacity"), "80\n").expect("battery capacity");

    let snapshot = collect_power_supplies_from(&fixture.0, 42);
    let battery = &snapshot.value.batteries[0];
    for availability in [
        battery.scalar_observations().energy_full_uwh.availability(),
        battery
            .scalar_observations()
            .energy_full_design_uwh
            .availability(),
        battery
            .scalar_observations()
            .time_to_empty_secs
            .availability(),
        battery
            .scalar_observations()
            .time_to_full_secs
            .availability(),
    ] {
        assert_eq!(
            availability,
            ScalarAvailability::Unavailable(FailureKind::Unsupported)
        );
    }
    assert_eq!(battery.current_health_pct(), None);
    assert_eq!(battery.current_time_to_empty_secs(), None);
    assert_eq!(battery.current_time_to_full_secs(), None);
    assert_eq!(
        snapshot.enrichments[0].outcome,
        SourceOutcome::Partial(FailureKind::Unsupported)
    );
    assert_eq!(snapshot.value.state.status, DeviceStatus::Healthy);
}

/// Runtime estimates are status-gated in both directions: charging reports
/// only time-to-full, discharging only time-to-empty, and any other status
/// (Full/Not charging/Unknown) leaves both unavailable even when the kernel
/// exposes the nodes — never a fabricated zero.
#[test]
fn time_estimates_follow_the_status_gate() {
    for (status, expected_empty, expected_full) in [
        ("Charging", None, Some(5_400.0)),
        ("Discharging", Some(3_780.0), None),
        ("Full", None, None),
        ("Not charging", None, None),
        ("Unknown", None, None),
    ] {
        let fixture = FixtureDir::new();
        let dir = fixture.0.join("BAT0");
        fs::create_dir_all(&dir).expect("battery directory");
        fs::write(dir.join("type"), "Battery\n").expect("battery type");
        fs::write(dir.join("status"), format!("{status}\n")).expect("battery status");
        fs::write(dir.join("time_to_empty_now"), "63\n").expect("time_to_empty node");
        fs::write(dir.join("time_to_full_now"), "90\n").expect("time_to_full node");

        let snapshot = collect_power_supplies_from(&fixture.0, 42);
        let battery = &snapshot.value.batteries[0];
        assert_eq!(battery.status, status);
        assert_eq!(
            battery.current_time_to_empty_secs(),
            expected_empty,
            "time_to_empty under status {status}"
        );
        assert_eq!(
            battery.current_time_to_full_secs(),
            expected_full,
            "time_to_full under status {status}"
        );
        // Each inapplicable side is typed-unavailable (not merely absent),
        // so a consumer can distinguish "no estimate under this status"
        // from "never observed".
        if expected_empty.is_none() {
            assert_eq!(
                battery
                    .scalar_observations()
                    .time_to_empty_secs
                    .availability(),
                ScalarAvailability::Unavailable(FailureKind::Unsupported),
                "time_to_empty gating under status {status}"
            );
        }
        if expected_full.is_none() {
            assert_eq!(
                battery
                    .scalar_observations()
                    .time_to_full_secs
                    .availability(),
                ScalarAvailability::Unavailable(FailureKind::Unsupported),
                "time_to_full gating under status {status}"
            );
        }
    }
}

/// An absurd estimate node (minutes beyond the plausible ceiling) is a
/// provider fault, never a saturated or wrapped seconds value.
#[test]
fn absurd_time_estimate_is_a_provider_fault() {
    let fixture = FixtureDir::new();
    let dir = fixture.0.join("BAT0");
    fs::create_dir_all(&dir).expect("battery directory");
    fs::write(dir.join("type"), "Battery\n").expect("battery type");
    fs::write(dir.join("status"), "Discharging\n").expect("battery status");
    fs::write(dir.join("time_to_empty_now"), u64::MAX.to_string()).expect("absurd estimate");

    let snapshot = collect_power_supplies_from(&fixture.0, 42);
    let battery = &snapshot.value.batteries[0];
    assert_eq!(battery.current_time_to_empty_secs(), None);
    assert_eq!(
        battery
            .scalar_observations()
            .time_to_empty_secs
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
}
