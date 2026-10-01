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

/// Format the open-file descriptor count against the process's soft/hard limits
/// from the shared resource projection.
///
/// When limits are available, formats:
/// - Both finite & differing: `"{count} / {soft} ({pct:.0}%) [max {hard}]"`
/// - Both finite & equal: `"{count} / {soft} ({pct:.0}%)"`
/// - Soft finite, hard unlimited: `"{count} / {soft} ({pct:.0}%) [max {unlimited_label}]"`
/// - Soft unlimited: `"{count} / {unlimited_label}"`
///
/// Returns `None` if neither soft nor hard limit is available.
#[must_use]
pub fn format_open_files_saturation(
    count: u64,
    soft: Option<taskmanager_core::LimitValue>,
    hard: Option<taskmanager_core::LimitValue>,
    unlimited_label: &str,
) -> Option<String> {
    use taskmanager_core::LimitValue;

    match (soft, hard) {
        (None, None) => None,
        (Some(LimitValue::Unlimited), _) => Some(format!("{count} / {unlimited_label}")),
        (Some(LimitValue::Value(soft_val)), Some(LimitValue::Value(hard_val))) => {
            let pct = (count as f64 / soft_val.max(1) as f64) * 100.0;
            if soft_val == hard_val {
                Some(format!("{count} / {soft_val} ({pct:.0}%)"))
            } else {
                Some(format!("{count} / {soft_val} ({pct:.0}%) [max {hard_val}]"))
            }
        }
        (Some(LimitValue::Value(soft_val)), Some(LimitValue::Unlimited)) => {
            let pct = (count as f64 / soft_val.max(1) as f64) * 100.0;
            Some(format!(
                "{count} / {soft_val} ({pct:.0}%) [max {unlimited_label}]"
            ))
        }
        (Some(LimitValue::Value(soft_val)), None) => {
            let pct = (count as f64 / soft_val.max(1) as f64) * 100.0;
            Some(format!("{count} / {soft_val} ({pct:.0}%)"))
        }
        (None, Some(LimitValue::Value(hard_val))) => Some(format!("{count} [max {hard_val}]")),
        (None, Some(LimitValue::Unlimited)) => Some(format!("{count} [max {unlimited_label}]")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskmanager_core::LimitValue;

    #[test]
    fn format_open_files_saturation_variants() {
        assert_eq!(
            format_open_files_saturation(
                42,
                Some(LimitValue::Value(1024)),
                Some(LimitValue::Value(4096)),
                "∞",
            ),
            Some("42 / 1024 (4%) [max 4096]".to_string())
        );
        assert_eq!(
            format_open_files_saturation(
                512,
                Some(LimitValue::Value(1024)),
                Some(LimitValue::Value(1024)),
                "∞",
            ),
            Some("512 / 1024 (50%)".to_string())
        );
        assert_eq!(
            format_open_files_saturation(
                10,
                Some(LimitValue::Value(100)),
                Some(LimitValue::Unlimited),
                "Unlimited",
            ),
            Some("10 / 100 (10%) [max Unlimited]".to_string())
        );
        assert_eq!(
            format_open_files_saturation(
                10,
                Some(LimitValue::Unlimited),
                Some(LimitValue::Unlimited),
                "∞",
            ),
            Some("10 / ∞".to_string())
        );
        assert_eq!(format_open_files_saturation(10, None, None, "∞"), None);
    }
}
