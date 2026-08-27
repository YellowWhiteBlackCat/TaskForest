use super::battery_stats;
use crate::core::{BatteryInfo, BatteryScalarObservations, DeviceState, ScalarObservation};

fn battery_with_scalars(scalars: BatteryScalarObservations) -> BatteryInfo {
    let mut battery = BatteryInfo::new("power-supply:BAT0", DeviceState::healthy(10));
    battery.apply_scalar_observations(scalars);
    battery
}

fn row_value(rows: &[taskmanager_shell::viewmodel::StatRow], key: &'static str) -> Option<String> {
    rows.iter()
        .find(|row| row.label() == crate::i18n::t(key))
        .and_then(|row| row.value().map(str::to_owned))
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
