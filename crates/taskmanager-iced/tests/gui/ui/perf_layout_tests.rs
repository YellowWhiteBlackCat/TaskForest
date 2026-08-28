use super::{bounded_heading, geometry_contract};
use crate::ui::device_chart::{DEVICE_CHART_HEIGHT, primary_graph_height};
use crate::ui::responsive::{
    DeviceNavigationPresentation, PERFORMANCE_SIDEBAR_MIN_WIDTH, PERFORMANCE_STATS_MAX_WIDTH,
    PERFORMANCE_STATS_MIN_WIDTH, PerformanceDetailsPresentation, PerformancePageBudget,
};
use iced::{Length, Size};

#[test]
fn geometry_contract_keeps_explicit_title_sizes() {
    let desktop = geometry_contract(false);
    let compact = geometry_contract(true);
    assert_eq!(desktop.title_size, 24.0);
    assert!(!desktop.compact);
    assert_eq!(compact.title_size, 19.0);
    assert!(compact.compact);
    assert!(compact.title_size < desktop.title_size);
}

#[test]
fn heading_projection_is_bounded_for_long_device_identity() {
    assert_eq!(bounded_heading("CPU", 12), "CPU");
    assert_eq!(
        bounded_heading("Intel Core Ultra 7 358H with extra suffix", 18),
        "Intel Core Ultra …"
    );
}

#[test]
fn primary_device_graph_fills_wide_cards_but_keeps_strip_cards_readable() {
    assert_eq!(
        primary_graph_height(false),
        Length::Fill,
        "sidebar frames must hand the primary graph the left column's remaining height"
    );
    assert_eq!(
        primary_graph_height(true),
        Length::Fixed(DEVICE_CHART_HEIGHT),
        "strip frames must retain an intrinsic graph height inside the page scroll"
    );
}

#[test]
fn frame_budget_allocates_the_three_semantic_slots_like_gpui() {
    // A wide desktop frame with the sidebar visible: Sidebar navigation, a
    // Pinned statistics rail clamped to the shared ceiling, and a readable
    // main viewport.
    let wide = PerformancePageBudget::for_perf_frame(Size::new(1920.0, 1080.0), true);
    assert_eq!(
        wide.device_navigation,
        DeviceNavigationPresentation::Sidebar
    );
    assert_eq!(
        wide.details,
        PerformanceDetailsPresentation::Pinned,
        "a wide frame pins the statistics rail beside the graphs"
    );
    assert_eq!(wide.sidebar_width, PERFORMANCE_SIDEBAR_MIN_WIDTH);
    assert_eq!(wide.stats_width, PERFORMANCE_STATS_MAX_WIDTH);
    assert!(
        wide.main_width >= crate::ui::responsive::PERFORMANCE_MAIN_MIN_WIDTH,
        "the main viewport keeps its readable floor"
    );

    // A hidden sidebar collapses navigation to the strip at ANY width — the
    // devices stay reachable (GPUI F9 parity), never a bare detail page.
    let hidden = PerformancePageBudget::for_perf_frame(Size::new(1920.0, 1080.0), false);
    assert_eq!(
        hidden.device_navigation,
        DeviceNavigationPresentation::Strip
    );

    // A mid-width frame that cannot carry rail + stats + main together keeps
    // the devices on the strip; the statistics rail still pins beside the
    // graphs when the main floor survives.
    let mid = PerformancePageBudget::for_perf_frame(Size::new(800.0, 900.0), true);
    assert_eq!(mid.device_navigation, DeviceNavigationPresentation::Strip);
    assert_eq!(mid.details, PerformanceDetailsPresentation::Pinned);

    // A narrow frame with no room for a pinned rail STACKS the statistics
    // below the main viewport instead of hiding the facts (GPUI parity).
    let narrow = PerformancePageBudget::for_perf_frame(Size::new(480.0, 900.0), true);
    assert_eq!(
        narrow.device_navigation,
        DeviceNavigationPresentation::Strip
    );
    assert_eq!(
        narrow.details,
        PerformanceDetailsPresentation::Stacked,
        "narrow frames keep the statistics available below the graphs"
    );
    assert!(narrow.stats_width >= PERFORMANCE_STATS_MIN_WIDTH);

    // Only an extremely narrow frame hides the statistics rail entirely.
    let tiny = PerformancePageBudget::for_perf_frame(Size::new(300.0, 900.0), true);
    assert_eq!(tiny.details, PerformanceDetailsPresentation::Hidden);
}

#[test]
fn frame_budget_types_the_vertical_ladder_from_the_content_height() {
    use crate::ui::responsive::PerformanceVerticalRunway;
    // A tall window composes the full chart inventory …
    let tall = PerformancePageBudget::for_perf_frame(Size::new(1920.0, 1080.0), true);
    assert_eq!(tall.vertical, PerformanceVerticalRunway::Charts);
    // … a short-but-WIDE window keeps its sidebar and pinned stats while the
    // ladder drops the header band and summaries (the old compact flag would
    // have collapsed the whole page to the strip).
    let short_wide = PerformancePageBudget::for_perf_frame(Size::new(1280.0, 420.0), true);
    assert_eq!(
        short_wide.device_navigation,
        DeviceNavigationPresentation::Sidebar,
        "a short-but-wide window keeps the sidebar (GPUI parity)"
    );
    assert_eq!(
        short_wide.details,
        PerformanceDetailsPresentation::Pinned,
        "a short-but-wide window keeps the pinned statistics rail"
    );
    assert_eq!(short_wide.vertical, PerformanceVerticalRunway::Floor);
    assert!(!short_wide.vertical.carries_core_stack());
    assert_eq!(
        short_wide.chart_inventory,
        crate::ui::responsive::PerformanceChartInventory::AggregateOnly,
        "the Floor rung admits only the aggregate chart"
    );
    // … and the Charts rung requires BOTH axes.
    let core = PerformancePageBudget::for_perf_frame(Size::new(1280.0, 560.0), true);
    assert_eq!(core.vertical, PerformanceVerticalRunway::Core);
    assert!(core.vertical.carries_core_stack());
    assert_eq!(
        core.chart_inventory,
        crate::ui::responsive::PerformanceChartInventory::AggregateOnly,
        "the Core rung still drops secondary charts"
    );
}
