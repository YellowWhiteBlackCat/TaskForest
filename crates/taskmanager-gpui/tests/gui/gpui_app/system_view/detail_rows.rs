use crate::core::{BatteryInfo, SensorCenterSnapshot, ThermalCoolingActivity};
use crate::i18n;

pub(super) fn battery_detail_rows(battery: &BatteryInfo) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if let Some(watts) = battery.current_power_w() {
        rows.push((
            i18n::t("system.battery_power").to_string(),
            format!("{watts:.1} W"),
        ));
    }
    if !battery.technology.is_empty() {
        rows.push((
            i18n::t("system.battery_chemistry").to_string(),
            battery.technology.clone(),
        ));
    }
    if let Some(cycles) = battery.current_cycle_count() {
        rows.push((
            i18n::t("system.battery_cycles").to_string(),
            cycles.to_string(),
        ));
    }
    if !battery.model_name.is_empty() {
        rows.push((
            i18n::t("system.battery_model").to_string(),
            battery.model_name.clone(),
        ));
    }
    if !battery.manufacturer.is_empty() {
        rows.push((
            i18n::t("system.battery_manufacturer").to_string(),
            battery.manufacturer.clone(),
        ));
    }
    rows
}

pub(super) fn thermal_control_rows(sensors: &SensorCenterSnapshot) -> Vec<(String, String)> {
    let thermal = &sensors.thermal_control;
    let mut rows = Vec::new();
    for device in &thermal.cooling_devices {
        let state = match (
            device.current_state.current_value().copied(),
            device.maximum_state.current_value().copied(),
        ) {
            (Some(current), Some(maximum)) => Some(format!("{current}/{maximum}")),
            _ => None,
        };
        let activity = device
            .activity
            .current_value()
            .copied()
            .map(|kind| match kind {
                ThermalCoolingActivity::Active => i18n::t("system.cooling_active"),
                ThermalCoolingActivity::Inactive => i18n::t("system.cooling_inactive"),
            });
        let value = match (state, activity) {
            (Some(state), Some(activity)) => format!("{state} · {activity}"),
            (Some(state), None) => state,
            (None, Some(activity)) => activity.to_string(),
            (None, None) => crate::gpui_app::formatting::missing_value(),
        };
        rows.push((
            format!("{} — {}", i18n::t("system.cooling"), device.id),
            value,
        ));
    }
    let throttle = &thermal.throttle;
    let core = throttle.core_events_observation().current_value().copied();
    let package = throttle
        .package_events_observation()
        .current_value()
        .copied();
    let value = match (core, package) {
        (Some(core), Some(package)) => format!(
            "{} {core} · {} {package}",
            i18n::t("system.throttle_core"),
            i18n::t("system.throttle_package")
        ),
        (Some(core), None) => format!("{} {core}", i18n::t("system.throttle_core")),
        (None, Some(package)) => {
            format!("{} {package}", i18n::t("system.throttle_package"))
        }
        (None, None) => crate::gpui_app::formatting::missing_value(),
    };
    rows.push((i18n::t("system.throttle").to_string(), value));
    rows
}
