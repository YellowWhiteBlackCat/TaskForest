use super::battery_stats;
use taskmanager_core::core::{
    BatteryInfo, BatteryScalarObservations, DeviceState, ScalarObservation,
};

fn battery_with_scalars(scalars: BatteryScalarObservations) -> BatteryInfo {
    let mut battery = BatteryInfo::new("power-supply:BAT0", DeviceState::healthy(10));
    battery.apply_scalar_observations(scalars);
    battery
}

fn row_value(rows: &[taskmanager_shell::viewmodel::StatRow], key: &'static str) -> Option<String> {
    rows.iter()
        .find(|row| row.label() == taskmanager_application::i18n::t(key))
        .and_then(|row| row.value().map(str::to_owned))
}

/// Whether the row exists at all: `StatRow::value() == None` means the fact is
/// applicable but uncollected, which is distinct from the producer omitting a
/// fact that does not exist on this host. (The shared `stats_panel` filters
/// valueless rows before painting, so neither shape can become a fabricated
/// numeric zero on the delivered surface.)
fn row_present(rows: &[taskmanager_shell::viewmodel::StatRow], key: &'static str) -> bool {
    rows.iter()
        .any(|row| row.label() == taskmanager_application::i18n::t(key))
}

/// Health and the native runtime estimates render only as typed facts: a
/// present pair derives 87.5% through the core rule and the estimate
/// formats through the shared duration formatter, while every unavailable
/// fact leaves its row entirely absent — never "0%" or "00h 00m".
#[test]
fn battery_stats_render_health_and_estimates_only_when_current() {
    taskmanager_test_support::pin_english();
    let full = battery_stats(&battery_with_scalars(BatteryScalarObservations {
        energy_full_uwh: ScalarObservation::available(49_000_000.0, 10),
        energy_full_design_uwh: ScalarObservation::available(56_000_000.0, 10),
        time_to_empty_secs: ScalarObservation::available(3_780.0, 10),
        ..Default::default()
    }));
    assert_eq!(row_value(&full, "battery.health").as_deref(), Some("87.5%"));
    assert_eq!(
        row_value(&full, "battery.time_to_empty").as_deref(),
        Some("01h 03m")
    );
    // Status-gated twin: no row at all, not a fake zero duration.
    assert_eq!(row_value(&full, "battery.time_to_full"), None);

    let sparse = battery_stats(&battery_with_scalars(BatteryScalarObservations::default()));
    assert_eq!(row_value(&sparse, "battery.health"), None);
    assert_eq!(row_value(&sparse, "battery.time_to_full"), None);
    assert_eq!(row_value(&sparse, "battery.time_to_empty"), None);
}

/// The feature's `delivery_definition` clause by clause, on the row set the
/// Performance battery panel paints (`dynamic::render_battery` feeds this
/// exact vector into `stats_panel`): charge, charge/discharge power, voltage,
/// health (the `energy_full / energy_full_design × 100` core rule), and cycle
/// count all carry real projected values. The honesty half is asserted too -
/// unobserved power/voltage/cycles/health omit their rows, and an uncollected
/// charge stays a valueless row (which the panel filters before painting), so
/// no clause can ever surface as a fabricated "0%"/"0 W"/"0.00 V"/"0" value.
#[test]
fn battery_stats_render_charge_power_voltage_health_and_cycles_with_honest_absence() {
    taskmanager_test_support::pin_english();
    let battery = battery_with_scalars(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(82, 10),
        voltage_uv: ScalarObservation::available(12_400_000, 10),
        power_w: ScalarObservation::available(8.4, 10),
        cycle_count: ScalarObservation::available(312, 10),
        energy_full_uwh: ScalarObservation::available(49_000_000.0, 10),
        energy_full_design_uwh: ScalarObservation::available(56_000_000.0, 10),
        ..Default::default()
    });
    let rows = battery_stats(&battery);
    assert_eq!(row_value(&rows, "battery.capacity").as_deref(), Some("82%"));
    assert_eq!(row_value(&rows, "battery.power").as_deref(), Some("8.4 W"));
    assert_eq!(
        row_value(&rows, "battery.voltage").as_deref(),
        Some("12.40 V")
    );
    assert_eq!(row_value(&rows, "battery.health").as_deref(), Some("87.5%"));
    assert_eq!(row_value(&rows, "battery.cycles").as_deref(), Some("312"));

    let sparse = battery_stats(&battery_with_scalars(BatteryScalarObservations::default()));
    // The applicable-but-uncollected charge keeps a valueless row ("not a
    // fabricated 0%"); the panel filters it before painting.
    assert!(row_present(&sparse, "battery.capacity"));
    assert_eq!(row_value(&sparse, "battery.capacity"), None);
    // Facts that do not exist on this host omit their rows; no fabricated
    // "0 W" / "0.00 V" / "0" / "0.0%".
    for key in [
        "battery.power",
        "battery.voltage",
        "battery.cycles",
        "battery.health",
    ] {
        assert!(
            !row_present(&sparse, key),
            "{key}: an unobserved field must not fabricate a row"
        );
    }
}
