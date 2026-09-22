//! Process-list presentation summaries shared by all frontends.

use taskmanager_application::i18n;
use taskmanager_core::ProcessItem;

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
