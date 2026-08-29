//! Pure data-layer folds for the System Health page (ARCH.md §8.1): the typed
//! observation reads (sensor measurement values, per-filesystem capacity)
//! live here so the render module only paints folded strings.

use std::path::Path;

use taskmanager_core::core::metrics::DiskMetrics;
use taskmanager_core::core::{FilesystemHealth, SensorQuantity, SensorReading};

use super::SystemHealthText;

/// Folded value presentation for one sensor reading: the formatted current
/// measurement plus whether a value exists at all. Presence is deliberately
/// independent of the kind match — a reading whose value exists but does not
/// match its declared kind still renders in the "has value" tone.
pub(super) struct SensorValueVm {
    pub(super) text: String,
    pub(super) present: bool,
}

pub(super) fn sensor_value_vm(
    reading: &SensorReading,
    copy: &dyn Fn(SystemHealthText) -> String,
) -> SensorValueVm {
    SensorValueVm {
        text: sensor_value_text(reading, copy),
        present: reading.current_number().is_some(),
    }
}

fn sensor_value_text(reading: &SensorReading, copy: &dyn Fn(SystemHealthText) -> String) -> String {
    match (reading.quantity(), reading.current_number()) {
        (SensorQuantity::Temperature, Some(value)) => {
            taskmanager_shell::presentation::temperature_c_precise(value as f32)
        }
        (SensorQuantity::FanSpeed, Some(value)) => {
            taskmanager_shell::presentation::fan_rpm(value as f32)
        }
        (SensorQuantity::Power, Some(value)) => {
            taskmanager_shell::presentation::power_w_precise(value as f32)
        }
        _ => copy(SystemHealthText::Unavailable),
    }
}

/// Capacity fold for one filesystem row: `(used_pct, available_bytes)` when
/// the selected disk is mounted at exactly this filesystem's mount point and
/// reports a usable total; `None` otherwise.
pub(super) fn filesystem_capacity(
    filesystem: &FilesystemHealth,
    disk: Option<&DiskMetrics>,
) -> Option<(f64, u64)> {
    let disk = disk?;
    let total = disk.current_capacity_bytes()?;
    let available = disk.current_available_bytes()?;
    if total == 0
        || disk.mount_point.is_empty()
        || Path::new(&disk.mount_point) != filesystem.mount_point
    {
        return None;
    }
    let used = total.saturating_sub(available);
    let used_pct = crate::gpui_app::formatting::bytes_percent(used, total);
    Some((used_pct, available))
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_system_health_view_stats_tests.rs"]
mod tests;
