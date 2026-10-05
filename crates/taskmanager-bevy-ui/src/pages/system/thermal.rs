//! Thermal-zone sensor traversal and presentation for the System page.
//!
//! Delivers true parity for `FeatureId::ThermalZoneSensors` (`power.thermal-zones`).
//! Traverses every `SensorQuantity::Temperature` reading in the shared `SensorCenterSnapshot`,
//! naming each reading's source label and formatting its temperature honestly.

use bevy::scene::Scene;
use taskmanager_application::i18n::t;
use taskmanager_core::core::sensors::{SensorCenterSnapshot, SensorQuantity};
use taskmanager_shell::presentation::{missing_value, temperature_c_precise};

use super::{SystemFactGroup, SystemFactRow, fact_row_scene, section_card_scene};
use crate::palette::UiPalette;

/// One projected thermal-zone row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThermalZoneRow {
    /// Source sensor label (e.g., "acpitz", "x86_pkg_temp", "coretemp").
    pub(crate) label: String,
    /// Display string: precise temperature (e.g. "54.5 °C") or shared dash.
    pub(crate) value: String,
    /// Whether an observed temperature value was present.
    pub(crate) present: bool,
}

/// Project thermal-zone temperature readings from the shared sensor snapshot.
///
/// Strictly mirrors GPUI (`sensor_rows(..., SensorGroup::Temperature, ...)`)
/// and Iced (`ui::health::projection::thermal_zone_rows`):
/// - Traverses every reading where `quantity() == SensorQuantity::Temperature`.
/// - Non-temperature channels (e.g., fan speed, power) are strictly excluded.
/// - Unreadable / missing temperature readings render the shared dash (`—`),
///   never a fabricated `0.0 °C`.
#[must_use]
pub(crate) fn thermal_zone_rows(sensors: &SensorCenterSnapshot) -> Vec<ThermalZoneRow> {
    sensors
        .readings
        .iter()
        .filter(|reading| reading.quantity() == &SensorQuantity::Temperature)
        .map(|reading| {
            let current = reading.current_number();
            ThermalZoneRow {
                label: reading.label().to_owned(),
                value: current
                    .map_or_else(missing_value, |value| temperature_c_precise(value as f32)),
                present: current.is_some(),
            }
        })
        .collect()
}

/// Construct a section card scene for thermal zones if any temperature readings are present.
pub(crate) fn thermal_zone_card_scene(
    thermal_rows: &[ThermalZoneRow],
    palette: &UiPalette,
) -> Option<Box<dyn Scene>> {
    if thermal_rows.is_empty() {
        return None;
    }
    let rows: Vec<Box<dyn Scene>> = thermal_rows
        .iter()
        .map(|row| {
            Box::new(fact_row_scene(
                &SystemFactRow {
                    group: SystemFactGroup::Hardware,
                    label: row.label.clone(),
                    value: row.value.clone(),
                },
                palette,
            )) as Box<dyn Scene>
        })
        .collect();
    Some(Box::new(section_card_scene(
        t("common.thermal_zones").to_owned(),
        rows,
        palette,
    )))
}
