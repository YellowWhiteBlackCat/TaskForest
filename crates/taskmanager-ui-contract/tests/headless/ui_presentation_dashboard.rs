use super::*;
use taskmanager_application::system_timeline::TimelineMetric;

/// The count is the product metric set's own cardinality: a shape that
/// restates the number drifts the moment the product defines a new metric.
#[test]
fn the_metric_count_comes_from_the_product_metric_set() {
    assert_eq!(DASHBOARD_METRIC_COUNT, TimelineMetric::ALL.len());
    let window = DashboardMetricWindow::resolve(0, DASHBOARD_METRIC_COUNT);
    assert_eq!(window.count, DASHBOARD_METRIC_COUNT);
    assert_eq!(
        window.metrics().collect::<Vec<_>>(),
        TimelineMetric::ALL.to_vec()
    );
}

/// A window never starts past the metrics it shows, and never holds more
/// metrics than the product owns.
#[test]
fn a_window_stays_inside_the_metrics_it_shows() {
    let wide = DashboardMetricWindow::resolve(3, DASHBOARD_METRIC_COUNT);
    assert_eq!(wide.first, 0, "the whole set is visible");
    assert_eq!(wide.count, DASHBOARD_METRIC_COUNT);

    let narrow = DashboardMetricWindow::resolve(3, 1);
    assert_eq!(
        narrow.first, 3,
        "the last metric may start a one-card window"
    );
    assert_eq!(narrow.metrics().count(), 1);

    let overfull = DashboardMetricWindow::resolve(0, DASHBOARD_METRIC_COUNT + 10);
    assert_eq!(overfull.count, DASHBOARD_METRIC_COUNT);
    assert_eq!(overfull.metrics().count(), DASHBOARD_METRIC_COUNT);
}

/// A paging step moves by one whole visible window, so a page always shows a
/// complete set of metrics.
#[test]
fn paging_moves_by_one_whole_window() {
    let first = DashboardMetricWindow::resolve(0, 2);
    assert_eq!(
        first.paged(MetricPaging::Next),
        DashboardMetricWindow { first: 2, count: 2 }
    );
    assert_eq!(
        first.paged(MetricPaging::Previous),
        DashboardMetricWindow { first: 0, count: 2 }
    );
    assert!(!first.can_page_previous());
    assert!(first.can_page_next());

    let last = DashboardMetricWindow::resolve(2, 2);
    assert!(last.can_page_previous());
    assert!(!last.can_page_next());
    assert_eq!(last.paged(MetricPaging::Next), last);
    assert_eq!(
        last.paged(MetricPaging::Previous),
        DashboardMetricWindow { first: 0, count: 2 }
    );
}

/// The admitted groups never paint more cards than the product owns, and a
/// column count wider than the metric set still admits one group.
#[test]
fn the_group_cap_stays_inside_the_product_count() {
    for columns in 1..=6 {
        let groups = max_metric_groups(columns);
        assert!(groups >= 1, "a dashboard always admits at least one group");
        let painted = DashboardMetricWindow::resolve(0, groups * columns)
            .metrics()
            .count();
        assert!(
            painted <= DASHBOARD_METRIC_COUNT,
            "the painted metrics never exceed the product metric count"
        );
        if columns <= DASHBOARD_METRIC_COUNT {
            assert!(groups * columns <= DASHBOARD_METRIC_COUNT);
        }
    }
}
