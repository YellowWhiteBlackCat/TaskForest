//! Battery device projection and formatting for the Performance page.

use std::collections::BTreeSet;

use bevy::app::App;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;
use taskmanager_application::i18n::t;
use taskmanager_core::core::power::BatteryInfo;
use taskmanager_shell::presentation::duration;

use crate::app::ShellTrack;
use crate::drain::ShellProjectionFolded;

use super::missing_value;

/// The battery ids whose folded status asks the presentation to animate.
#[derive(Resource, Default)]
pub(crate) struct BatteryChargeStates {
    charging_ids: BTreeSet<String>,
}

impl BatteryChargeStates {
    pub(crate) fn is_charging(&self, battery_id: &str) -> bool {
        self.charging_ids.contains(battery_id)
    }
}

/// Refresh charge animation inputs only when the shared shell projection folds.
pub(crate) fn refresh_charge_states(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    mut states: ResMut<BatteryChargeStates>,
) {
    states.charging_ids = super::batteries(track.shell())
        .into_iter()
        .flatten()
        .filter(|battery| battery_is_charging(battery))
        .map(|battery| battery.id.clone())
        .collect();
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<BatteryChargeStates>();
    app.add_observer(refresh_charge_states);
}

fn battery_is_charging(battery: &BatteryInfo) -> bool {
    battery.status.eq_ignore_ascii_case("Charging")
}

/// The observed charge percentage of one battery.
///
/// The renderer never reads the observation itself (ARCH.md §8.1): this data
/// layer owns the read and hands the folded share down to the icon scene, so
/// the scene paints a fact instead of folding one.
pub(crate) fn battery_charge_pct(battery: &BatteryInfo) -> Option<u8> {
    battery.current_capacity_pct()
}

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
