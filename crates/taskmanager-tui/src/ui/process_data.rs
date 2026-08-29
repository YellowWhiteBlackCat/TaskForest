//! Pure process observation folds consumed by TUI renderers.
//!
//! Process table cells and Properties performance peaks share this one typed
//! read boundary. Ratatui code receives owned strings or primitive display
//! facts and never reaches into `ProcessItem::current_*` observations.

use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::time::LocalTimeRulesObservation;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences, format_quantity_f64};
use taskmanager_shell::presentation::{MISSING_VALUE, optional_nice, peak_of, start_clock_local};

pub(super) struct ProcessCellData {
    pub(super) cpu: Option<f32>,
    pub(super) memory: Option<u64>,
    pub(super) pss: Option<u64>,
    pub(super) swap: Option<u64>,
    pub(super) user: String,
    pub(super) threads: String,
    pub(super) fds: String,
    pub(super) nice: String,
    pub(super) start_time: String,
    pub(super) cpu_time: String,
    pub(super) disk_read: Option<u64>,
    pub(super) disk_write: Option<u64>,
}

pub(super) fn process_cell_data(
    process: &ProcessItem,
    local_time_rules: &LocalTimeRulesObservation,
) -> ProcessCellData {
    ProcessCellData {
        cpu: process.current_cpu_percentage(),
        memory: process.current_memory_bytes(),
        pss: process.current_memory_pss_bytes(),
        swap: process.current_swap_bytes(),
        user: process.current_user().unwrap_or_default(),
        threads: process
            .current_threads()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| value.to_string()),
        fds: process
            .current_fds()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| value.to_string()),
        nice: optional_nice(process.current_nice()),
        start_time: start_clock_local(process.current_start_time_secs(), local_time_rules),
        cpu_time: process
            .current_cpu_time_secs()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| format!("{value:.1}s")),
        disk_read: process.current_disk_read_bytes_per_sec(),
        disk_write: process.current_disk_write_bytes_per_sec(),
    }
}

pub(super) struct ProcessPerformancePeaks {
    pub(super) cpu: Option<String>,
    pub(super) memory: Option<String>,
    pub(super) disk_read: Option<String>,
    pub(super) disk_write: Option<String>,
}

pub(super) fn process_performance_peaks(item: &ProcessItem) -> ProcessPerformancePeaks {
    let units = UnitPreferences::default();
    ProcessPerformancePeaks {
        cpu: peak_of(&item.cpu_history, item.current_cpu_percentage())
            .map(|peak| format!("{peak:.1}%")),
        memory: peak_of(
            &item.mem_history,
            item.current_memory_bytes().map(|bytes| bytes as f32),
        )
        .map(|peak| format_quantity_f64(f64::from(peak), QuantityFamily::Memory, false, &units)),
        disk_read: peak_of(
            &item.disk_read_history,
            item.current_disk_read_bytes_per_sec()
                .map(|bytes| bytes as f32),
        )
        .map(|peak| format_quantity_f64(f64::from(peak), QuantityFamily::Drive, true, &units)),
        disk_write: peak_of(
            &item.disk_write_history,
            item.current_disk_write_bytes_per_sec()
                .map(|bytes| bytes as f32),
        )
        .map(|peak| format_quantity_f64(f64::from(peak), QuantityFamily::Drive, true, &units)),
    }
}

/// Unknown swap capacity keeps the column visible; only a confirmed zero
/// proves that the host has no swap device.
pub(super) fn swap_column_visible(snapshot: Option<&SystemSnapshot>) -> bool {
    snapshot.is_none_or(|snapshot| snapshot.memory.current_swap_total_bytes() != Some(0))
}
