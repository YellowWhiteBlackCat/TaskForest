//! Battery device projection and formatting for the Performance page.

use taskmanager_application::i18n::t;
use taskmanager_core::core::power::BatteryInfo;
use taskmanager_shell::presentation::duration;

use super::missing_value;

/// Joined fact line for one battery block: charge, power, voltage, cycles,
/// degradation health, and runtime estimates (full / empty).
pub(crate) fn battery_fact_line(battery: &BatteryInfo) -> String {
    let charge = battery
        .current_capacity_pct()
        .map_or_else(missing_value, |pct| format!("{pct}%"));
    let watts = battery
        .current_power_w()
        .filter(|w| w.is_finite())
        .map_or_else(missing_value, |w| format!("{w:.1} W"));
    let mut parts = vec![
        format!("{}: {charge}", t("battery.capacity")),
        format!("{}: {watts}", t("battery.power")),
    ];
    if let Some(voltage_uv) = battery.current_voltage_uv() {
        parts.push(format!(
            "{}: {:.2} V",
            t("battery.voltage"),
            voltage_uv as f64 / 1_000_000.0
        ));
    }
    if let Some(cycles) = battery.current_cycle_count() {
        parts.push(format!("{}: {cycles}", t("battery.cycles")));
    }
    if let Some(health) = battery.current_health_pct() {
        parts.push(format!("{}: {health:.1}%", t("battery.health")));
    }
    if let Some(secs) = battery.current_time_to_full_secs() {
        parts.push(format!(
            "{}: {}",
            t("battery.time_to_full"),
            duration(secs as u64)
        ));
    }
    if let Some(secs) = battery.current_time_to_empty_secs() {
        parts.push(format!(
            "{}: {}",
            t("battery.time_to_empty"),
            duration(secs as u64)
        ));
    }
    parts.join(" · ")
}
