//! One renderer-neutral fold from the shell projection into semantic facts.
//!
//! A frontend owns its application name, toolkit-native modal surfaces and
//! structural process-tree rows. The facts visible in every shape — bounded
//! process rows, CPU graph, memory percentage and the live status line — are
//! computed here once, so no frontend can spell a second conversion.

use taskforest_core::core::process::{ProcessItem, ProcessLiveKey};
use taskmanager_application::process_sort::{ProcessSortAxis, compare_processes};
use taskmanager_ui_contract::{GraphSummary, ProcessRowInput, SemanticSnapshotBuilder};

use crate::{ProcessRowId, ShellApp, process_semantic_key};

/// One bounded semantic snapshot reads at most this many process rows.
pub const MAX_PUBLISHED_ROWS: usize = 64;

/// The live-region status when the shell did not publish explicit feedback.
#[must_use]
pub fn default_status_announcement(shell: &ShellApp) -> String {
    format!("{} processes visible", shell.visible_process_count())
}

/// The CPU utilization graph fact, if the current scalar is observed and finite.
#[must_use]
pub fn semantic_cpu_graph(current_usage_pct: Option<f32>) -> Option<GraphSummary> {
    let current = current_usage_pct
        .filter(|value| value.is_finite())
        .map(|value| f64::from(value.clamp(0.0, 100.0)))?;
    Some(GraphSummary {
        current,
        peak: current,
        maximum: 100.0,
    })
}

/// A typed process percentage once the system total is trustworthy.
#[must_use]
pub fn memory_percentage(value: Option<u64>, total: Option<u64>) -> Option<f64> {
    let (value, total) = (value?, total.filter(|total| *total > 0)?);
    let percentage = (value as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
    percentage.is_finite().then_some(percentage)
}

/// The process-row facts shared by every frontend shape. Selection policy is
/// passed explicitly because a frontend may own its own projection cache.
#[must_use]
pub fn semantic_process_row(
    process: &ProcessItem,
    memory_total_bytes: Option<u64>,
    selected: bool,
) -> ProcessRowInput {
    let name = if process.name.trim().is_empty() {
        String::from("Unnamed process")
    } else {
        process.name.clone()
    };
    ProcessRowInput {
        id: process_semantic_key(process),
        name,
        cpu_percent: process
            .current_cpu_percentage()
            .filter(|value| value.is_finite())
            .map(|value| f64::from(value.clamp(0.0, 100.0))),
        memory_percent: memory_percentage(process.current_memory_bytes(), memory_total_bytes),
        selected,
    }
}

/// The shell's own selection decision over its visible process projection.
#[must_use]
pub fn shell_process_selected(shell: &ShellApp, process: &ProcessItem, index: usize) -> bool {
    let by_identity = match shell.selected_row {
        Some(ProcessRowId::Process(identity)) => ProcessLiveKey::from_process(process)
            .is_some_and(|row_identity| row_identity == identity),
        Some(ProcessRowId::Application(_) | ProcessRowId::Category(_)) => false,
        None => index == shell.selected,
    };
    by_identity || shell.is_process_selected(process)
}

/// The shell-owned header of one canonical semantic snapshot.
///
/// Revision, live status and the observed CPU graph are facts every renderer
/// shares; the frontend passes only its product name. A renderer with its own
/// row sequence (for example the TUI's process-tree groups) appends those rows
/// to this header instead of re-deriving the common facts.
#[must_use]
pub fn semantic_snapshot_header(
    shell: &ShellApp,
    application_name: impl Into<String>,
) -> SemanticSnapshotBuilder {
    let mut builder = SemanticSnapshotBuilder::new(shell.projection().refresh_count)
        .application_name(application_name)
        .status_announcement(semantic_status(
            shell.feedback_text(),
            shell.visible_process_count(),
        ));
    if let Some(summary) = semantic_cpu_graph(cpu_usage(shell)) {
        builder = builder.cpu_graph(summary);
    }
    builder
}

/// The one status-line policy: explicit feedback wins; otherwise the bounded
/// visible-process count is announced.
#[must_use]
pub fn semantic_status(feedback: &str, visible_process_count: usize) -> String {
    if feedback.trim().is_empty() {
        format!("{visible_process_count} processes visible")
    } else {
        feedback.to_owned()
    }
}

#[must_use]
fn cpu_usage(shell: &ShellApp) -> Option<f32> {
    shell
        .projection()
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.cpu.current_global_usage_pct())
}

#[must_use]
fn memory_total(shell: &ShellApp) -> Option<u64> {
    shell
        .projection()
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.memory.current_total_bytes())
}

/// The canonical semantic snapshot for frontends whose process rows come
/// directly from the shell's bounded visible-process projection.
#[must_use]
pub fn base_semantic_snapshot_builder(
    shell: &ShellApp,
    application_name: impl Into<String>,
) -> SemanticSnapshotBuilder {
    let mut builder = semantic_snapshot_header(shell, application_name);
    let mut processes = shell.visible_processes();
    processes.sort_by(|left, right| compare_processes(left, right, ProcessSortAxis::Cpu, false));
    let rows = processes
        .into_iter()
        .take(MAX_PUBLISHED_ROWS)
        .enumerate()
        .map(|(index, process)| {
            semantic_process_row(
                process,
                memory_total(shell),
                shell_process_selected(shell, process, index),
            )
        })
        .collect::<Vec<_>>();
    if !rows.is_empty() {
        builder = builder.process_rows(rows);
    }
    builder
}

#[cfg(test)]
#[path = "../tests/headless/accessibility_projection.rs"]
mod accessibility_projection_tests;
