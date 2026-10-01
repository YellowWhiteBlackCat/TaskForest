//! Process-list presentation summaries shared by all frontends.

use taskmanager_application::i18n;
use taskmanager_core::ProcessItem;
pub use taskmanager_core::core::process::ProcessAncestorNode;
use taskmanager_core::core::process::{format_ancestor_lineage, process_ancestor_lineage};

/// Retrieve and format the ancestor lineage for a target PID from a process inventory.
#[must_use]
pub fn process_ancestor_lineage_summary(
    processes: Option<&[ProcessItem]>,
    target_pid: u32,
) -> String {
    processes.map_or_else(
        || "—".to_string(),
        |items| format_ancestor_lineage(&process_ancestor_lineage(items, target_pid)),
    )
}

/// Count live processes in Linux's uninterruptible `D` state. The count is
/// derived from the shared process projection so a missing snapshot remains
/// distinguishable from a measured zero.
#[must_use]
pub fn uninterruptible_process_count(processes: Option<&[ProcessItem]>) -> Option<usize> {
    processes.map(|items| {
        items
            .iter()
            .filter(|process| process.status_kind().is_uninterruptible())
            .count()
    })
}

/// Compact aggregate diagnostic for the process-page subtitle. It is omitted
/// when the inventory has not arrived or when the measured count is zero.
#[must_use]
pub fn uninterruptible_process_summary(processes: Option<&[ProcessItem]>) -> Option<String> {
    uninterruptible_process_count(processes)
        .filter(|count| *count > 0)
        .map(|count| format!("{} {count}", i18n::t("proc.d_state")))
}
