//! Renderer text projection for typed Applications group aggregates.
//!
//! The source metrics remain typed in the shared row projection. This module
//! produces the full value/status text consumed by the Applications row and
//! marks cells whose detail needs a tooltip when the table width clips them.

use taskmanager_application::i18n::t;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarAvailability;
use taskmanager_core::core::process::aggregate::AggregateMetric;
use taskmanager_shell::ProcessRowAggregate;
use taskmanager_shell::presentation::{control_error_detail, optional_bytes, optional_duration};

/// Renderer text for one typed aggregate cell. `is_current_zero` keeps the
/// gray-zero preference tied to a real current measurement; stale historical
/// zeroes never acquire current-value styling. `has_detail` enables a tooltip
/// for status and failure text that cannot fit in the bounded table cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AggregateCellProjection {
    /// Full explanation used by the bounded cell's tooltip.
    pub(crate) text: String,
    /// Short value/status token that remains visible at narrow column widths.
    pub(crate) compact_text: String,
    pub(crate) is_current_zero: bool,
    pub(crate) has_detail: bool,
}

/// The full aggregate cell set used by an Applications group row. Keeping
/// this as one projection makes every aggregate column follow the same
/// availability and coverage grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GroupAggregateCellProjection {
    pub(crate) cpu: AggregateCellProjection,
    pub(crate) memory: AggregateCellProjection,
    pub(crate) memory_pss: AggregateCellProjection,
    pub(crate) swap: AggregateCellProjection,
    pub(crate) disk_read: AggregateCellProjection,
    pub(crate) disk_write: AggregateCellProjection,
    pub(crate) cpu_time: AggregateCellProjection,
    pub(crate) threads: AggregateCellProjection,
    pub(crate) fds: AggregateCellProjection,
}

/// Project typed group facts into the exact strings consumed by the row
/// widgets. This is only a renderer projection: `ProcessRowAggregate` remains
/// typed and owned by the shared row projection.
#[must_use]
pub(crate) fn project_group_aggregate_cells(
    metrics: &ProcessRowAggregate,
) -> GroupAggregateCellProjection {
    GroupAggregateCellProjection {
        cpu: aggregate_cell(
            metrics.cpu(),
            |value| format!("{value:>5.1}%"),
            |value| *value == 0.0,
        ),
        memory: aggregate_cell(
            metrics.memory(),
            |value| optional_bytes(Some(*value)),
            |value| *value == 0,
        ),
        memory_pss: aggregate_cell(
            metrics.memory_pss(),
            |value| optional_bytes(Some(*value)),
            |value| *value == 0,
        ),
        swap: aggregate_cell(
            metrics.swap(),
            |value| optional_bytes(Some(*value)),
            |value| *value == 0,
        ),
        disk_read: aggregate_cell(
            metrics.disk_read(),
            |value| optional_bytes(Some(*value)),
            |value| *value == 0,
        ),
        disk_write: aggregate_cell(
            metrics.disk_write(),
            |value| optional_bytes(Some(*value)),
            |value| *value == 0,
        ),
        cpu_time: aggregate_cell(
            metrics.cpu_time(),
            |value| optional_duration(Some(*value)),
            |value| *value == 0,
        ),
        threads: aggregate_cell(metrics.threads(), ToString::to_string, |value| *value == 0),
        fds: aggregate_cell(metrics.fds(), ToString::to_string, |value| *value == 0),
    }
}

fn aggregate_cell<T>(
    metric: &AggregateMetric<T>,
    format_value: impl Fn(&T) -> String,
    is_zero: impl Fn(&T) -> bool,
) -> AggregateCellProjection {
    match metric.availability() {
        ScalarAvailability::Available => {
            metric
                .current_value()
                .map_or_else(unknown_aggregate_cell, |value| AggregateCellProjection {
                    text: format_value(value),
                    compact_text: format_value(value).trim().to_owned(),
                    is_current_zero: is_zero(value),
                    has_detail: false,
                })
        }
        ScalarAvailability::Partial(failure) => metric.current_value().map_or_else(
            || unavailable_aggregate_cell(failure),
            |value| AggregateCellProjection {
                compact_text: format!(
                    "{}P{}/{}",
                    compact_numeric_value(format_value(value)),
                    metric.current_member_count(),
                    metric.member_count(),
                ),
                text: format!(
                    "{} · {}/{} current · {}: {}",
                    format_value(value),
                    metric.current_member_count(),
                    metric.member_count(),
                    t("disk.usage_partial"),
                    control_error_detail(failure),
                ),
                is_current_zero: is_zero(value),
                has_detail: true,
            },
        ),
        ScalarAvailability::Stale(failure) => metric.last_known_value().map_or_else(
            || unavailable_aggregate_cell(failure),
            |value| AggregateCellProjection {
                compact_text: format!(
                    "{}S{}/{}",
                    compact_numeric_value(format_value(value)),
                    metric.known_member_count(),
                    metric.member_count(),
                ),
                text: format!(
                    "{} · {}/{} known · {}: {}",
                    format_value(value),
                    metric.known_member_count(),
                    metric.member_count(),
                    t("device.stale"),
                    control_error_detail(failure),
                ),
                is_current_zero: false,
                has_detail: true,
            },
        ),
        ScalarAvailability::Unknown => unknown_aggregate_cell(),
        ScalarAvailability::Unavailable(failure) => unavailable_aggregate_cell(failure),
    }
}

fn compact_numeric_value(value: String) -> String {
    value.trim().replace(".0", "").replace(' ', "")
}

fn unknown_aggregate_cell() -> AggregateCellProjection {
    AggregateCellProjection {
        text: t("proc_insights.unknown").to_owned(),
        compact_text: t("proc_insights.unknown").to_owned(),
        is_current_zero: false,
        has_detail: false,
    }
}

fn unavailable_aggregate_cell(failure: FailureKind) -> AggregateCellProjection {
    AggregateCellProjection {
        text: format!(
            "{}: {}",
            t("proc_insights.unavailable"),
            control_error_detail(failure),
        ),
        compact_text: t("proc_insights.unavailable").to_owned(),
        is_current_zero: false,
        has_detail: true,
    }
}
