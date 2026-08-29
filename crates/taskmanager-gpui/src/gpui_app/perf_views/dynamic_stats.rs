//! Pure stat-row projection for dynamic battery and sensor devices.

use taskmanager_application::i18n;
use taskmanager_core::core::{
    BatteryInfo, SensorCenterSnapshot, SensorMagnitude, SensorQuantity, SensorReading,
};
use taskmanager_shell::viewmodel::StatRow;

pub(super) fn battery_stats(battery: &BatteryInfo) -> Vec<StatRow> {
    let mut stats = vec![
        StatRow::text(
            i18n::t("battery.capacity"),
            battery
                .current_capacity_pct()
                .map(|value| format!("{value}%")),
        ),
        StatRow::text(i18n::t("battery.status"), Some(battery.status.clone())),
    ];
    if let Some(power) = battery.current_power_w() {
        stats.push(StatRow::text(
            i18n::t("battery.power"),
            Some(format!("{power:.1} W")),
        ));
    }
    if let Some(voltage) = battery.current_voltage_uv() {
        stats.push(StatRow::text(
            i18n::t("battery.voltage"),
            Some(format!("{:.2} V", voltage as f64 / 1_000_000.0)),
        ));
    }
    if let Some(cycles) = battery.current_cycle_count() {
        stats.push(StatRow::text(
            i18n::t("battery.cycles"),
            Some(cycles.to_string()),
        ));
    }
    // Degradation health and native runtime estimates render only when the
    // typed facts are current — an unavailable estimate is an absent row,
    // never a believable "0%" / "00h 00m".
    if let Some(health) = battery.current_health_pct() {
        stats.push(StatRow::text(
            i18n::t("battery.health"),
            Some(format!("{health:.1}%")),
        ));
    }
    if let Some(secs) = battery.current_time_to_full_secs() {
        stats.push(StatRow::text(
            i18n::t("battery.time_to_full"),
            Some(taskmanager_shell::presentation::duration(secs as u64)),
        ));
    }
    if let Some(secs) = battery.current_time_to_empty_secs() {
        stats.push(StatRow::text(
            i18n::t("battery.time_to_empty"),
            Some(taskmanager_shell::presentation::duration(secs as u64)),
        ));
    }
    if !battery.technology.is_empty() {
        stats.push(StatRow::text(
            i18n::t("battery.technology"),
            Some(battery.technology.clone()),
        ));
    }
    if !battery.manufacturer.is_empty() {
        stats.push(StatRow::text(
            i18n::t("battery.manufacturer"),
            Some(battery.manufacturer.clone()),
        ));
    }
    stats
}

pub(super) fn fan_stats(sensors: &SensorCenterSnapshot, fan: &SensorReading) -> Vec<StatRow> {
    let mut stats = vec![StatRow::text(
        i18n::t("fan.rpm"),
        fan.current_number().map(|value| format!("{value:.0} RPM")),
    )];
    if let Some(pwm) = sensors
        .readings
        .iter()
        .filter(|reading| reading.device_id() == fan.device_id())
        .find_map(sensor_pwm_percent)
    {
        stats.push(StatRow::text(
            i18n::t("fan.pwm"),
            Some(format!("{pwm:.0}%")),
        ));
    }
    for temperature in sensors.readings.iter().filter(|reading| {
        reading.device_id() == fan.device_id() && reading.quantity() == &SensorQuantity::Temperature
    }) {
        if let Some(value) = temperature.current_number() {
            stats.push(StatRow::text(
                format!("{} {}", i18n::t("common.temperature"), temperature.label()),
                Some(format!("{value:.1} °C")),
            ));
        }
    }
    stats
}

fn sensor_pwm_percent(reading: &SensorReading) -> Option<f32> {
    match reading.measurement_observation().current_value() {
        Some(SensorMagnitude::DutyCycle { value, maximum }) if *maximum > 0 => {
            Some((*value as f32 * 100.0) / *maximum as f32)
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_dynamic_stats_tests.rs"]
mod tests;
