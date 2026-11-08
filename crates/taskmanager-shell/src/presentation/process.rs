//! Process-list presentation summaries shared by all frontends.

use crate::presentation::{MISSING_VALUE, bytes};
use taskforest_core::LimitValue;
use taskforest_core::ProcessItem;
use taskforest_core::core::metrics::SystemSnapshot;
use taskforest_core::core::percent::{percent_text, precise_percent_text};
use taskforest_core::core::process::{format_ancestor_lineage, process_ancestor_lineage};
use taskforest_core::core::process_telemetry::OpenFileEntry;
use taskforest_core::core::process_telemetry::ProcessResourceSnapshot;
use taskmanager_application::{ProcessInsightFacet, i18n, i18n::t, project_process_resources};

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
/// Whether the process table's swap column earns its place.
///
/// Only a **measured zero** swap total proves the host has no swap device;
/// unknown telemetry (and the absence of a snapshot) keeps the column
/// visible, so absence is never fabricated into a hidden column. Every
/// frontend renders the processes table with the same answer.
#[must_use]
pub fn swap_column_visible(snapshot: Option<&SystemSnapshot>) -> bool {
    snapshot.is_none_or(|snapshot| snapshot.memory.current_swap_total_bytes() != Some(0))
}

#[must_use]
pub fn uninterruptible_process_count(processes: Option<&[ProcessItem]>) -> Option<usize> {
    processes.map(|items| {
        items
            .iter()
            .filter(|process| process.status_kind().is_uninterruptible())
            .count()
    })
}

/// Share of the observed process inventory currently in uninterruptible sleep.
/// A missing or empty inventory has no denominator and therefore no ratio.
#[must_use]
pub fn uninterruptible_process_ratio(processes: Option<&[ProcessItem]>) -> Option<f32> {
    let processes = processes.filter(|items| !items.is_empty())?;
    let count = processes
        .iter()
        .filter(|process| process.status_kind().is_uninterruptible())
        .count();
    Some(count as f32 / processes.len() as f32 * 100.0)
}

/// Compact aggregate diagnostic for the process-page subtitle. It is omitted
/// only when the inventory has not arrived or is empty. A measured zero stays
/// visible with its true zero-percent ratio instead of disappearing.
#[must_use]
pub fn uninterruptible_process_summary(processes: Option<&[ProcessItem]>) -> Option<String> {
    let count = uninterruptible_process_count(processes)?;
    let ratio = uninterruptible_process_ratio(processes)?;
    Some(format!(
        "{} {count} · {}",
        i18n::t("proc.d_state"),
        precise_percent_text(ratio)
    ))
}

/// Format the open-file descriptor count against the process's soft/hard limits
/// from the shared resource projection.
///
/// When limits are available, formats:
/// - Both finite & differing: `"{count} / {soft} ({pct}%) [max {hard}]"`
/// - Both finite & equal: `"{count} / {soft} ({pct}%)"`
/// - Soft finite, hard unlimited: `"{count} / {soft} ({pct}%) [max {unlimited_label}]"`
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
                Some(format!("{count} / {soft_val} ({})", percent_text(pct)))
            } else {
                Some(format!(
                    "{count} / {soft_val} ({}) [max {hard_val}]",
                    percent_text(pct)
                ))
            }
        }
        (Some(LimitValue::Value(soft_val)), Some(LimitValue::Unlimited)) => {
            let pct = (count as f64 / soft_val.max(1) as f64) * 100.0;
            Some(format!(
                "{count} / {soft_val} ({}) [max {unlimited_label}]",
                percent_text(pct)
            ))
        }
        (Some(LimitValue::Value(soft_val)), None) => {
            let pct = (count as f64 / soft_val.max(1) as f64) * 100.0;
            Some(format!("{count} / {soft_val} ({})", percent_text(pct)))
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
            memory.map(|value| format!("{value} ({})", percent_text(percent)))
        });
    let cpu_quota = match (
        projection.cpu_time_quota_micros,
        projection.cpu_time_period_micros,
    ) {
        (Some(LimitValue::Unlimited), _) => Some("CPU ∞".to_owned()),
        (Some(LimitValue::Value(quota)), Some(period)) if period > 0 => Some(format!(
            "CPU {}",
            percent_text((quota as f64 / period as f64) * 100.0)
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
            pids.map(|value| format!("{value} ({})", percent_text(percent)))
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

/// Format a single process item as tab-separated values (TSV).
#[must_use]
pub fn process_to_tsv(process: &ProcessItem) -> String {
    let user = process
        .current_user()
        .unwrap_or_else(|| MISSING_VALUE.to_owned());
    let cpu = process
        .current_cpu_percentage()
        .map_or_else(|| MISSING_VALUE.to_owned(), precise_percent_text);
    let memory = process
        .current_memory_bytes()
        .map_or_else(|| MISSING_VALUE.to_owned(), bytes);
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

/// Render one open-file descriptor row: `fd {fd} [{kind}] → {target}`.
///
/// This is the SINGLE open-file row spelling (ADR-020): the GPUI Properties
/// dialog, the TUI Insights preview, and the Iced Insights facet must not
/// disagree about how the same descriptor reads. The `fd` prefix names the
/// number's meaning, the bracketed kind is the typed handle class, and the
/// arrow separates the descriptor from its resolved path.
///
/// A descriptor whose readlink failed keeps its row: the target falls back to
/// the caller's typed unreadable marker, never a blank cell and never a
/// fabricated path. A deleted descriptor keeps its target and gains the
/// `[deleted]` marker, so the row stays honest about what the host reported.
#[must_use]
pub fn format_open_file_row(entry: &OpenFileEntry, unreadable: &str) -> String {
    let target = entry
        .target
        .clone()
        .unwrap_or_else(|| unreadable.to_string());
    if entry.deleted {
        format!(
            "fd {} [{}] → {} [deleted]",
            entry.fd,
            entry.resolved_kind(),
            target
        )
    } else {
        format!("fd {} [{}] → {}", entry.fd, entry.resolved_kind(), target)
    }
}
