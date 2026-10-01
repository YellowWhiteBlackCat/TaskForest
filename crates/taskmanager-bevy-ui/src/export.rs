//! Structured export formatting helpers for the Bevy frontend.
//!
//! Provides formatting helpers for multi-format process and snapshot export,
//! delegating pure serialization to the core export engine.

use taskmanager_core::core::export::{
    ExportExtras, processes_to_csv, processes_to_html, snapshot_to_json,
    snapshot_to_json_with_extras,
};
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_shell::presentation::{bytes, missing_value};

/// Format a single process item as tab-separated values (TSV).
#[must_use]
pub fn process_to_tsv(process: &ProcessItem) -> String {
    let user = process.current_user().unwrap_or_else(missing_value);
    let cpu = process
        .current_cpu_percentage()
        .map_or_else(missing_value, |value| format!("{value:.1}%"));
    let memory = process
        .current_memory_bytes()
        .map_or_else(missing_value, bytes);
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        process.pid,
        process.name,
        cpu,
        memory,
        if user.is_empty() { "—" } else { &user },
        process.status,
    )
}

/// Format multiple processes as multi-row TSV with a header.
#[must_use]
pub fn processes_to_tsv(processes: &[ProcessItem]) -> String {
    let mut output = String::from("PID\tName\tCPU\tMemory\tUser\tStatus\n");
    for proc in processes {
        output.push_str(&process_to_tsv(proc));
        output.push('\n');
    }
    output
}

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
