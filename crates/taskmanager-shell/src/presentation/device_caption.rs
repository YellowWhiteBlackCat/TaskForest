//! Single shared presentation projections for device captions.
//!
//! The Bevy sidebar and the Iced rail render the same device facts, so the
//! caption lines belong to one owner that neither surface names after itself.

use taskforest_core::core::device_state::DeviceStatus;
use taskforest_core::core::power::BatteryInfo;
use taskforest_core::core::sensors::SensorReading;
use taskmanager_application::i18n;

use super::{device_status_i18n_key, missing_value};
use taskforest_core::core::percent::percent_text;

/// Append the localized status badge for a non-healthy device to a caption
/// line (healthy devices add nothing — they are the silent majority).
pub fn append_status_badge(caption: &mut String, status: DeviceStatus) {
    if status == DeviceStatus::Healthy {
        return;
    }
    let key = device_status_i18n_key(status);
    if !caption.is_empty() {
        caption.push_str(" · ");
    }
    caption.push_str(i18n::t(key));
}

/// Battery sidebar/rail caption lines: current capacity percentage and status badge.
#[must_use]
pub fn battery_caption(battery: &BatteryInfo) -> (String, String) {
    let cap1 = battery
        .current_capacity_pct()
        .map_or_else(missing_value, percent_text);
    let mut cap2 = battery.status.clone();
    append_status_badge(&mut cap2, battery.device_state.status);
    (cap1, cap2)
}

/// Fan sidebar/rail caption lines: current RPM and channel label with status badge.
#[must_use]
pub fn fan_caption(reading: &SensorReading) -> (String, String) {
    let rpm = reading
        .current_number()
        .map_or_else(missing_value, |value| format!("{value:.0} RPM"));
    let mut cap2 = reading.label().to_owned();
    append_status_badge(&mut cap2, reading.state().status);
    (rpm, cap2)
}

#[cfg(test)]
#[path = "../../tests/headless/presentation/device_caption_tests.rs"]
mod tests;
