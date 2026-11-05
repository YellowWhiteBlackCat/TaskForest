use super::{bounded_heading, bounded_stat_text, geometry_contract};
use crate::ui::device_chart::{DEVICE_CHART_HEIGHT, primary_graph_height};
use crate::ui::responsive::{
    PERFORMANCE_SIDEBAR_MIN_WIDTH, PERFORMANCE_STATS_MAX_WIDTH, PERFORMANCE_STATS_MIN_WIDTH,
    performance_budget_for_perf_frame,
};
use iced::{Length, Size};
use taskmanager_theme::tokens;
use taskmanager_ui_contract::presentation::vocabulary::{
    DeviceNavigationPresentation, PERFORMANCE_MAIN_MIN_WIDTH, PerformanceChartInventory,
    PerformanceDetailsPresentation,
};

#[test]
fn device_sidebar_gap_has_no_resize_cursor_and_keeps_both_slots_bounded() {
    use iced::advanced::Layout;
    use iced::advanced::layout::Limits;
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::Tree;
    use iced::{Point, Rectangle, mouse};

    let renderer = iced::futures::executor::block_on(iced::Renderer::new(
        crate::theme_binding::BUNDLED_UI_FONT,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer");

    for viewport in [Size::new(1280.0, 720.0), Size::new(1920.0, 540.0)] {
        let mut app = crate::IcedApp::demo();
        let _ = app.update(crate::app::Message::WindowResized(viewport));
        let mut page = crate::ui::performance::performance_page(&app);
        let mut tree = Tree::new(&page);
        let node =
            page.as_widget_mut()
                .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, viewport));
        let children = node.children();
        assert_eq!(children.len(), 3, "the wide page has three slots");
        let sidebar = children[0].bounds();
        let gap = children[1].bounds();
        let detail = children[2].bounds();
        assert!(sidebar.x + sidebar.width <= gap.x);
        assert!(gap.x + gap.width <= detail.x);
        assert!(detail.x + detail.width <= viewport.width);

        let cursor = mouse::Cursor::Available(Point::new(
            gap.x + gap.width / 2.0,
            node.bounds().y + node.bounds().height / 2.0,
        ));
        assert_eq!(
            page.as_widget().mouse_interaction(
                &tree,
                Layout::new(&node),
                cursor,
                &Rectangle::new(Point::ORIGIN, viewport),
                &renderer,
            ),
            mouse::Interaction::None,
            "the non-resizable gap must not advertise a resize action"
        );
    }
}

#[test]
fn geometry_contract_titles_come_from_the_public_type_scale() {
    let desktop = geometry_contract(false);
    let compact = geometry_contract(true);
    // The device-detail heading keeps the GPUI perf-page rung and the strip
    // frame steps one rung down: both sizes and the weight are theme tokens,
    // never numbers this frontend invents (ADR-063).
    assert_eq!(desktop.title_size, tokens::FONT_26);
    assert_eq!(desktop.title_weight, tokens::FONT_WEIGHT_EXTRA_BOLD);
    assert!(!desktop.compact);
    assert_eq!(compact.title_size, tokens::FONT_20);
    assert_eq!(compact.title_weight, tokens::FONT_WEIGHT_EXTRA_BOLD);
    assert!(compact.compact);
    assert!(compact.title_size < desktop.title_size);
    assert_eq!(compact.subtitle_size, tokens::FONT_12);
    assert_eq!(desktop.subtitle_size, tokens::FONT_15);
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
fn stat_projection_is_single_line_and_bounded_for_dense_rails() {
    let long = "Interrupts 2350000 · CPU19 110000 (5%) · CPU0 3% · CPU1 3% · CPU2 3% · CPU3 3%";
    let bounded = bounded_stat_text(long, false, false);
    assert_eq!(bounded.chars().count(), 20);
    assert!(bounded.ends_with('…'));
    assert_eq!(bounded_stat_text("Load basis", false, true), "Load basis");
    assert_eq!(
        bounded_stat_text("CPU power preference", true, true),
        "CPU power pre…"
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
    let wide = performance_budget_for_perf_frame(Size::new(1920.0, 1080.0), true);
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
        wide.main_width >= PERFORMANCE_MAIN_MIN_WIDTH,
        "the main viewport keeps its readable floor"
    );

    // A hidden sidebar collapses navigation to the strip at ANY width — the
    // devices stay reachable (GPUI F9 parity), never a bare detail page.
    let hidden = performance_budget_for_perf_frame(Size::new(1920.0, 1080.0), false);
    assert_eq!(
        hidden.device_navigation,
        DeviceNavigationPresentation::Strip
    );

    // A mid-width frame that cannot carry rail + stats + main together keeps
    // the devices on the strip; the statistics rail still pins beside the
    // graphs when the main floor survives.
    let mid = performance_budget_for_perf_frame(Size::new(800.0, 900.0), true);
    assert_eq!(mid.device_navigation, DeviceNavigationPresentation::Strip);
    assert_eq!(mid.details, PerformanceDetailsPresentation::Pinned);

    // A narrow frame with no room for a pinned rail STACKS the statistics
    // below the main viewport instead of hiding the facts (GPUI parity).
    let narrow = performance_budget_for_perf_frame(Size::new(480.0, 900.0), true);
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
    let tiny = performance_budget_for_perf_frame(Size::new(300.0, 900.0), true);
    assert_eq!(tiny.details, PerformanceDetailsPresentation::Hidden);
}

#[test]
fn frame_budget_types_the_vertical_ladder_from_the_content_height() {
    use taskmanager_ui_contract::presentation::vocabulary::PerformanceVerticalRunway;
    // A tall window composes the full chart inventory …
    let tall = performance_budget_for_perf_frame(Size::new(1920.0, 1080.0), true);
    assert_eq!(tall.vertical, PerformanceVerticalRunway::Charts);
    // … a short-but-WIDE window keeps its sidebar and pinned stats while the
    // ladder drops the header band and summaries (the old compact flag would
    // have collapsed the whole page to the strip).
    let short_wide = performance_budget_for_perf_frame(Size::new(1280.0, 420.0), true);
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
        PerformanceChartInventory::AggregateOnly,
        "the Floor rung admits only the aggregate chart"
    );
    // … and the Charts rung requires BOTH axes.
    let core = performance_budget_for_perf_frame(Size::new(1280.0, 560.0), true);
    assert_eq!(core.vertical, PerformanceVerticalRunway::Core);
    assert!(core.vertical.carries_core_stack());
    assert_eq!(
        core.chart_inventory,
        PerformanceChartInventory::AggregateOnly,
        "the Core rung still drops secondary charts"
    );
}
