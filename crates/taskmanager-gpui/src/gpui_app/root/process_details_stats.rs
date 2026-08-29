//! Pure numeric projection for the Process Properties performance graphs.

use crate::gpui_app::formatting::missing_value;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences, format_quantity_f64};

use super::ProcessHistories;

pub(super) struct ProcessPerformancePeaks {
    pub cpu: String,
    pub memory: String,
    pub disk_read: String,
    pub disk_write: String,
}

pub(super) fn performance_peaks(
    item: &ProcessItem,
    histories: &ProcessHistories,
    preferences: &UnitPreferences,
) -> ProcessPerformancePeaks {
    ProcessPerformancePeaks {
        cpu: optional_peak(&histories.cpu, item.current_cpu_percentage(), |peak| {
            format!("{peak:.1}%")
        }),
        memory: optional_peak(
            &histories.memory,
            item.current_memory_bytes().map(|bytes| bytes as f32),
            |peak| format_quantity_f64(f64::from(peak), QuantityFamily::Memory, false, preferences),
        ),
        disk_read: optional_peak(
            &histories.disk_read,
            item.current_disk_read_bytes_per_sec()
                .map(|bytes| bytes as f32),
            |peak| format_quantity_f64(f64::from(peak), QuantityFamily::Drive, true, preferences),
        ),
        disk_write: optional_peak(
            &histories.disk_write,
            item.current_disk_write_bytes_per_sec()
                .map(|bytes| bytes as f32),
            |peak| format_quantity_f64(f64::from(peak), QuantityFamily::Drive, true, preferences),
        ),
    }
}

fn optional_peak(samples: &[f32], current: Option<f32>, format: impl Fn(f32) -> String) -> String {
    taskmanager_shell::presentation::peak_of(samples, current).map_or_else(missing_value, format)
}
