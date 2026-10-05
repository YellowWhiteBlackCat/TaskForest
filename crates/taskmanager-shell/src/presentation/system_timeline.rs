//! Shared numeric readouts for the four authoritative System dashboard curves.

use super::missing_value;
use taskmanager_application::{
    i18n,
    system_timeline::{TimelineMetric, TimelineSelection, TimelineSeries, TimelineStatistic},
};

#[must_use]
pub fn readout(
    series: &TimelineSeries,
    metric: TimelineMetric,
    statistic: TimelineStatistic,
) -> String {
    series
        .readout(TimelineSelection::new(metric, statistic))
        .map_or_else(missing_value, |value| {
            format!("{:.1} {}", value.value, metric.unit())
        })
}

#[must_use]
pub fn coverage(series: &TimelineSeries, metric: TimelineMetric) -> String {
    i18n::t("dashboard.coverage").replace(
        "{minutes}",
        &format!("{:.1}", series.coverage_ms(metric) as f64 / 60_000.0),
    )
}
