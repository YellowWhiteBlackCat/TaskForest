//! Structured export formatting helpers for the Bevy frontend.
//!
//! Provides formatting helpers for multi-format process and snapshot export,
//! delegating pure serialization to the core export engine.

use taskforest_core::core::export::{
    ExportExtras, processes_to_csv, processes_to_html, snapshot_to_json,
    snapshot_to_json_with_extras,
};
use taskforest_core::core::metrics::SystemSnapshot;
use taskforest_core::core::process::ProcessItem;

/// Export a system snapshot and processes to JSON string.
#[must_use]
pub fn export_snapshot_json(snapshot: &SystemSnapshot, processes: &[ProcessItem]) -> String {
    snapshot_to_json(snapshot, processes)
}

/// Export a system snapshot and processes to JSON string with hardware/extras.
#[must_use]
pub fn export_snapshot_json_with_extras(
    snapshot: &SystemSnapshot,
    processes: &[ProcessItem],
    extras: ExportExtras<'_>,
) -> String {
    snapshot_to_json_with_extras(snapshot, processes, extras)
}

/// Export processes to CSV string.
#[must_use]
pub fn export_processes_csv(processes: &[ProcessItem]) -> String {
    processes_to_csv(processes)
}

/// Export a system snapshot and processes to self-contained HTML report.
#[must_use]
pub fn export_processes_html(snapshot: &SystemSnapshot, processes: &[ProcessItem]) -> String {
    processes_to_html(snapshot, processes)
}
