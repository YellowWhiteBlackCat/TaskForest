//! Process-row display projection for the Bevy Applications table.
//!
//! This module owns the current/last-known fold and the final cell strings.
//! Scene adapters receive those strings and never read process observations
//! while painting. The projection is deliberately independent of Bevy scene
//! types so the same behavior can be tested without a window.

use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::time::LocalTimeRulesObservation;
use taskmanager_shell::presentation::{
    MISSING_VALUE, bytes, optional_cpu_time_seconds, optional_nice, start_clock_local,
};
use taskmanager_ui_contract::ProcessColumnSpec;

/// One row's cell vector over the contract columns (contract order, widths,
/// and numeric alignment come from the shared vocabulary, never a local copy).
/// The selected row prefixes the Name cell with the TUI's `›` cursor marker.
pub(crate) fn row_cells(
    process: &ProcessItem,
    columns: &[&ProcessColumnSpec],
    selected: bool,
) -> Vec<String> {
    columns
        .iter()
        .map(|column| {
            let text = cell_text(process, column.id);
            if selected && column.id == "Name" {
                format!("› {text}")
            } else {
                text
            }
        })
        .collect()
}

/// One cell's final display text. Unavailable scalars render the shared
/// `MISSING_VALUE` dash, exactly like the TUI cells — a provider failure is
/// never shown as a zero. The Start column uses the unsupported local-time
/// observation, so it renders `—` until this frontend observes a timezone.
fn cell_text(process: &ProcessItem, column: &str) -> String {
    match column {
        "Name" => process.name.clone(),
        "User" => process
            .current_user()
            .unwrap_or_else(|| MISSING_VALUE.to_owned()),
        "PID" => process.pid.to_string(),
        "Threads" => process
            .current_threads()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| value.to_string()),
        "StartTime" => start_clock_local(
            process.current_start_time_secs(),
            &LocalTimeRulesObservation::unsupported(0),
        ),
        "Status" => process.status.clone(),
        "CPU" => process
            .current_cpu_percentage()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| format!("{value:.1}%")),
        "Memory" => process
            .current_memory_bytes()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "Swap" => process
            .current_swap_bytes()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "DiskRead" => process
            .current_disk_read_bytes_per_sec()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "DiskWrite" => process
            .current_disk_write_bytes_per_sec()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "CPUTime" => optional_cpu_time_seconds(process.current_cpu_time_secs()),
        "FDs" => process
            .current_fds()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| value.to_string()),
        "Nice" => optional_nice(process.current_nice()),
        _ => MISSING_VALUE.to_owned(),
    }
}
