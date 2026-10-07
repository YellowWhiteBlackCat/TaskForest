//! Fan channel projections for the Performance page (P0-MC-01).
//!
//! A missing sensor snapshot is `None` (honest absence); a snapshot with no fan
//! channel is an empty list — the Fan section then renders no block, never an
//! idle fan.

use super::*;
use taskmanager_core::core::sensors::{SensorQuantity, SensorReading};

/// The fan channels the sensor center reports (`FanSpeed` readings).
pub(crate) fn fans(shell: &ShellApp) -> Option<Vec<&SensorReading>> {
    shell.projection().sensors.as_ref().map(|sensors| {
        sensors
            .readings
            .iter()
            .filter(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
            .collect()
    })
}

/// One fan channel's joined fact line: RPM is the headline reading, with the
/// same honest dash an unprobed channel renders (never a fabricated idle RPM).
pub(crate) fn fan_fact_line(fan: &SensorReading) -> String {
    let rpm = fan
        .current_number()
        .map_or_else(missing_value, |value| format!("{value:.0} RPM"));
    format!("{} {rpm}", t("fan.rpm"))
}

/// A fan row's short caption (the RPM readout), for the sidebar's accessory
/// text; an unread channel keeps the shared dash.
pub(crate) fn fan_caption(fan: &SensorReading) -> String {
    fan.current_number()
        .map_or_else(missing_value, |value| format!("{value:.0} RPM"))
}
