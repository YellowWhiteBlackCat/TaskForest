//! Availability-preserving display text for process-group aggregate cells.

use taskforest_core::core::metrics::ScalarAvailability;
use taskforest_core::core::process::aggregate::AggregateMetric;
use taskforest_core::core::units::UnitPreferences;
use taskmanager_application::i18n;
use taskmanager_shell::presentation::control_error_detail;

/// One stable text projection for aggregate CPU. Its full string is reused by
/// the table tooltip and accessibility summary; the bounded cell truncates
/// only its visual copy.
pub(crate) fn cpu_aggregate_summary(metric: &AggregateMetric<f32>) -> String {
    aggregate_metric_summary(metric, |value| {
        super::formatting::format_cpu_percent(*value)
    })
}

/// One stable text projection for aggregate memory.
pub(crate) fn memory_aggregate_summary(
    metric: &AggregateMetric<u64>,
    units: UnitPreferences,
) -> String {
    aggregate_metric_summary(metric, |value| {
        super::formatting::format_memory(units, *value)
    })
}

/// Short status token for the bounded aggregate cell; the full explanation
/// remains in the tooltip and accessibility summary.
pub(crate) fn cpu_aggregate_compact(metric: &AggregateMetric<f32>) -> String {
    aggregate_metric_compact(metric, |value| format!("{value:.0}%"))
}

/// Short status token for the bounded aggregate memory cell.
pub(crate) fn memory_aggregate_compact(
    metric: &AggregateMetric<u64>,
    units: UnitPreferences,
) -> String {
    aggregate_metric_compact(metric, |value| {
        super::formatting::format_memory(units, *value)
            .replace(' ', "")
            .replace(".0", "")
    })
}

fn aggregate_metric_compact<T>(
    metric: &AggregateMetric<T>,
    format_value: impl Fn(&T) -> String,
) -> String {
    match metric.availability() {
        ScalarAvailability::Available => metric
            .current_value()
            .map(format_value)
            .unwrap_or_else(|| i18n::t("proc_insights.unknown").to_owned()),
        ScalarAvailability::Partial(_) => metric.current_value().map_or_else(
            || i18n::t("proc_insights.unavailable").to_owned(),
            |value| {
                format!(
                    "{}P{}/{}",
                    format_value(value),
                    metric.current_member_count(),
                    metric.member_count()
                )
            },
        ),
        ScalarAvailability::Stale(_) => metric.last_known_value().map_or_else(
            || i18n::t("proc_insights.unavailable").to_owned(),
            |value| {
                format!(
                    "{}S{}/{}",
                    format_value(value),
                    metric.known_member_count(),
                    metric.member_count()
                )
            },
        ),
        ScalarAvailability::Unknown => i18n::t("proc_insights.unknown").to_owned(),
        ScalarAvailability::Unavailable(_) => i18n::t("proc_insights.unavailable").to_owned(),
    }
}

fn aggregate_metric_summary<T>(
    metric: &AggregateMetric<T>,
    format_value: impl FnOnce(&T) -> String,
) -> String {
    let partial_label = i18n::t("disk.usage_partial");
    match metric.availability() {
        ScalarAvailability::Available => metric
            .current_value()
            .map(format_value)
            .unwrap_or_else(|| i18n::t("proc_insights.unknown").to_owned()),
        ScalarAvailability::Partial(failure) => metric.current_value().map_or_else(
            || {
                format!(
                    "{} · {}",
                    i18n::t("proc_insights.unavailable"),
                    control_error_detail(failure)
                )
            },
            |value| {
                format!(
                    "{} · {} {}/{} · {}",
                    format_value(value),
                    partial_label,
                    metric.current_member_count(),
                    metric.member_count(),
                    control_error_detail(failure)
                )
            },
        ),
        ScalarAvailability::Stale(failure) => metric.last_known_value().map_or_else(
            || {
                format!(
                    "{} · {}",
                    i18n::t("proc_insights.unavailable"),
                    control_error_detail(failure)
                )
            },
            |value| {
                format!(
                    "{} · {} {}/{} · {}",
                    format_value(value),
                    i18n::t("device.stale"),
                    metric.known_member_count(),
                    metric.member_count(),
                    control_error_detail(failure)
                )
            },
        ),
        ScalarAvailability::Unknown => i18n::t("proc_insights.unknown").to_owned(),
        ScalarAvailability::Unavailable(failure) => format!(
            "{} · {}",
            i18n::t("proc_insights.unavailable"),
            control_error_detail(failure)
        ),
    }
}
