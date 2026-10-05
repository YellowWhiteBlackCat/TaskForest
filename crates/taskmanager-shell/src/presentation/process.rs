//! Process-list presentation summaries shared by all frontends.

use crate::presentation::{MISSING_VALUE, bytes};
use taskmanager_application::{ProcessInsightFacet, i18n, i18n::t, project_process_resources};
use taskmanager_core::LimitValue;
use taskmanager_core::ProcessItem;
use taskmanager_core::core::process::{format_ancestor_lineage, process_ancestor_lineage};
use taskmanager_core::core::process_telemetry::ProcessResourceSnapshot;

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
    soft: Option<LimitValue>,
    hard: Option<LimitValue>,
    unlimited_label: &str,
) -> Option<String> {
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
#[path = "../../tests/headless/presentation_process_tests.rs"]
mod tests;

/// Neutral labels for the independent process-inspection selectors.
#[must_use]
pub fn process_insight_facet_label(facet: ProcessInsightFacet) -> &'static str {
    i18n::t(match facet {
        ProcessInsightFacet::Network => "common.network",
        ProcessInsightFacet::Gpu => "common.gpu",
        ProcessInsightFacet::Resources => "proc_insights.resource_limits",
        ProcessInsightFacet::Isolation => "proc_insights.isolation",
        ProcessInsightFacet::Threads => "proc_insights.threads",
        ProcessInsightFacet::OpenFiles => "proc_insights.open_files",
        ProcessInsightFacet::Environment => "prop.environment",
    })
}

/// Complete current resource limits with quantity units and typed missing values.
pub fn process_resource_rows(resources: &ProcessResourceSnapshot) -> Vec<(String, String)> {
    let projection = project_process_resources(resources);
    let memory = match (projection.memory_usage_bytes, projection.memory_limit) {
        (Some(used), Some(LimitValue::Value(limit))) => {
            Some(format!("{} / {}", bytes(used), bytes(limit)))
        }
        (Some(used), Some(LimitValue::Unlimited)) => Some(format!("{} / ∞", bytes(used))),
        (Some(used), None) => Some(bytes(used)),
        (None, Some(LimitValue::Value(limit))) => Some(format!("— / {}", bytes(limit))),
        (None, Some(LimitValue::Unlimited)) => Some("— / ∞".to_owned()),
        (None, None) => None,
    };
    let memory = projection
        .memory_limit
        .and_then(|limit| limit.usage_percent(projection.memory_usage_bytes))
        .map_or(memory.clone(), |percent| {
            memory.map(|value| format!("{value} ({percent:.0}%)"))
        });
    let cpu_quota = match (
        projection.cpu_time_quota_micros,
        projection.cpu_time_period_micros,
    ) {
        (Some(LimitValue::Unlimited), _) => Some("CPU ∞".to_owned()),
        (Some(LimitValue::Value(quota)), Some(period)) if period > 0 => Some(format!(
            "CPU {:.0}%",
            (quota as f64 / period as f64) * 100.0
        )),
        (Some(LimitValue::Value(quota)), _) => Some(format!("CPU {quota}µs")),
        (None, _) => None,
    };
    let pids = match (projection.process_count, projection.process_limit) {
        (Some(count), Some(LimitValue::Value(limit))) => {
            Some(format!("{count} / {limit} {}", t("proc_insights.pids")))
        }
        (Some(count), Some(LimitValue::Unlimited)) => {
            Some(format!("{count} / ∞ {}", t("proc_insights.pids")))
        }
        (Some(count), None) => Some(format!("{count} {}", t("proc_insights.pids"))),
        (None, Some(LimitValue::Value(limit))) => {
            Some(format!("— / {limit} {}", t("proc_insights.pids")))
        }
        (None, Some(LimitValue::Unlimited)) => Some(format!("— / ∞ {}", t("proc_insights.pids"))),
        (None, None) => None,
    };
    let pids = projection
        .process_limit
        .and_then(|limit| limit.usage_percent(projection.process_count))
        .map_or(pids.clone(), |percent| {
            pids.map(|value| format!("{value} ({percent:.0}%)"))
        });
    let resource_group = projection.resource_group.map(ToOwned::to_owned);

    [
        ("common.memory", memory),
        ("proc_insights.cpu_quota", cpu_quota),
        ("proc_insights.pids", pids),
        ("proc_insights.resource_group", resource_group),
    ]
    .into_iter()
    .map(|(label, value)| {
        (
            t(label).to_owned(),
            value.unwrap_or_else(|| MISSING_VALUE.to_owned()),
        )
    })
    .collect()
}
