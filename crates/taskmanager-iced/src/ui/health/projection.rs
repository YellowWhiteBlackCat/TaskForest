//! Pure system-health observation projection.

use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::sensors::{SensorCenterSnapshot, SensorQuantity};
use taskmanager_shell::presentation::{missing_value, temperature_c_precise};

pub(super) struct HealthObservation {
    pub cpu_usage_pct: Option<f32>,
    pub cpu_frequency: String,
    pub cpu_temperature_c: Option<f32>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub swap_used_bytes: Option<u64>,
    pub swap_total_bytes: Option<u64>,
}

impl From<&SystemSnapshot> for HealthObservation {
    fn from(snapshot: &SystemSnapshot) -> Self {
        Self {
            cpu_usage_pct: snapshot.cpu.current_global_usage_pct(),
            cpu_frequency: crate::ui::perf_overview::cpu_frequency_readout_for_source(
                snapshot.cpu.current_frequency_mhz(),
                snapshot.cpu.frequency_source.is_bogomips(),
            ),
            cpu_temperature_c: snapshot.cpu.current_temperature_c(),
            memory_used_bytes: snapshot.memory.current_used_bytes(),
            memory_total_bytes: snapshot.memory.current_total_bytes(),
            swap_used_bytes: snapshot.memory.current_swap_used_bytes(),
            swap_total_bytes: snapshot.memory.current_swap_total_bytes(),
        }
    }
}

/// One folded thermal-zone row: the shared reading's own source label plus its
/// formatted current measurement. `present` is false for a channel whose read
/// failed or has not landed yet, which keeps the row a typed absence instead of
/// a fabricated `0.0 °C`.
#[derive(Debug)]
pub(super) struct ThermalZoneRow {
    pub label: String,
    pub value: String,
    pub present: bool,
}

/// Fold the shared sensor center's temperature readings: exactly one row per
/// thermal zone in projection order, each named by the reading's own source
/// label — the same traversal semantics as the GPUI health-page sensor section
/// (`sensor_rows(..., Temperature, ...)`) over the same shared `SensorReading`
/// facts. An unread/failed zone keeps its row with the shared dash (never
/// `0.0 °C`), and the °C spelling stays the shared presentation helper.
#[must_use]
pub(super) fn thermal_zone_rows(sensors: &SensorCenterSnapshot) -> Vec<ThermalZoneRow> {
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

#[must_use]
pub(super) fn thermal_readings(snapshot: &SystemSnapshot) -> Vec<(String, f32)> {
    let mut readings = Vec::new();
    if let Some(temperature) = snapshot.cpu.current_temperature_c() {
        readings.push(("CPU Package".to_owned(), temperature));
    }
    readings.extend(snapshot.gpu.iter().enumerate().filter_map(|(index, gpu)| {
        let temperature = gpu.current_temperature_c()?;
        let label = if gpu.brand.is_empty() {
            format!("GPU {index}")
        } else {
            format!("GPU {index} ({})", gpu.brand)
        };
        Some((label, temperature))
    }));
    readings
}
