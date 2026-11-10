//! The shared dashboard metric window: the metric count the product owns and
//! the paging arithmetic every shape must agree on.
//!
//! The dashboard presents the product's timeline metrics. WHICH metrics
//! exist, and in which order, is a semantic fact owned by the application
//! layer ([`TimelineMetric::ALL`]); HOW MANY metric cards a shape may draw,
//! and WHERE a paged window may start, is a presentation fact. The shapes
//! re-derived it (a `4` card cap in GPUI, Iced and Bevy, a `3` paging bound
//! in Iced and Bevy), so the count and the paging rule live here once and
//! every shape spends the verdict.

use taskmanager_application::system_timeline::TimelineMetric;

/// The number of dashboard metric cards the product owns.
///
/// The metric SET lives with its semantic owner ([`TimelineMetric::ALL`]);
/// this is the same fact seen from the presentation side, so no shape may
/// restate the number.
pub const DASHBOARD_METRIC_COUNT: usize = TimelineMetric::ALL.len();

/// The number of whole metric groups a shape may admit for a column count.
///
/// The cap keeps the admitted cards inside the product's metric count: a
/// two-column dashboard admits two groups (four cards), a one-column
/// dashboard admits four groups (four cards).
#[must_use]
pub fn max_metric_groups(columns: usize) -> usize {
    let columns = if columns == 0 { 1 } else { columns };
    if DASHBOARD_METRIC_COUNT == 0 {
        0
    } else {
        let groups = DASHBOARD_METRIC_COUNT / columns;
        if groups == 0 { 1 } else { groups }
    }
}

/// Which way a metric window pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricPaging {
    /// Toward the earlier metrics.
    Previous,
    /// Toward the later metrics.
    Next,
}

/// The metric window a dashboard shows: where the window starts and how many
/// metrics it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DashboardMetricWindow {
    /// The first metric the window starts at.
    pub first: usize,
    /// How many metrics the window holds.
    pub count: usize,
}

impl DashboardMetricWindow {
    /// Resolve a requested start offset against the metrics that remain.
    ///
    /// A window never starts past the metrics it shows: when the visible
    /// count already covers every metric, the window starts at the first
    /// metric and the shape shows all of them.
    #[must_use]
    pub fn resolve(requested_first: usize, visible: usize) -> Self {
        let count = visible.min(DASHBOARD_METRIC_COUNT);
        let first = requested_first.min(DASHBOARD_METRIC_COUNT.saturating_sub(count.max(1)));
        Self { first, count }
    }

    /// The window a paging step lands on.
    ///
    /// A step moves by one whole visible window, so a page always shows a
    /// complete set of metrics and the last page starts on the last metric
    /// the shape can show.
    #[must_use]
    pub fn paged(self, step: MetricPaging) -> Self {
        let count = self.count.max(1);
        let first = match step {
            MetricPaging::Previous => self.first.saturating_sub(count),
            MetricPaging::Next => {
                (self.first + count).min(DASHBOARD_METRIC_COUNT.saturating_sub(count))
            }
        };
        Self::resolve(first, self.count)
    }

    /// Whether an earlier page exists.
    #[must_use]
    pub fn can_page_previous(self) -> bool {
        self.first > 0
    }

    /// Whether a later page exists.
    #[must_use]
    pub fn can_page_next(self) -> bool {
        self.first + self.count.max(1) < DASHBOARD_METRIC_COUNT
    }

    /// The metrics this window shows, in product order.
    pub fn metrics(self) -> impl Iterator<Item = TimelineMetric> {
        TimelineMetric::ALL
            .into_iter()
            .skip(self.first)
            .take(self.count)
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_dashboard.rs"]
mod tests;
