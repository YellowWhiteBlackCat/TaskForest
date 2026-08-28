//! test-intent: behavior
//! Behavior coverage for the ported frame-local layout budget: the two
//! independent capacity axes, the exact shared-threshold boundaries, the
//! vertical rail width subtraction, the page presentation allocations, and
//! the old→new breakpoint equivalence where the pre-port magic-number
//! expressions are the oracle.

use super::{
    ChromePresentation, DeviceNavigationPresentation, LayoutProfile, NavOrientation,
    NavigationPresentation, PageLayoutBudget, PerformanceChartInventory,
    PerformanceDetailsPresentation, PerformancePageBudget, SystemPageBudget,
    SystemSurfacePresentation, VerticalSpace, layout_profile, nav_rail_width, vertical_space,
};
use crate::app::Message;
use crate::ui::performance::compact_toolbar_columns;
use iced::Size;

fn frame(width: f32, height: f32) -> Size {
    Size::new(width, height)
}

#[test]
fn typed_layout_profiles_keep_horizontal_and_vertical_capacity_independent() {
    let ultra = PageLayoutBudget::for_viewport(frame(720.0, 480.0));
    assert_eq!(ultra.profile, LayoutProfile::UltraCompact);
    assert_eq!(ultra.vertical_space, VerticalSpace::Constrained);
    assert_eq!(ultra.navigation, NavigationPresentation::IconOnly);
    let ultra_performance = PerformancePageBudget::from_page_layout(ultra);
    assert_eq!(
        ultra_performance.device_navigation,
        DeviceNavigationPresentation::Strip
    );
    assert_eq!(
        ultra_performance.details,
        PerformanceDetailsPresentation::Hidden
    );
    assert_eq!(
        ultra_performance.chart_inventory,
        PerformanceChartInventory::AggregateOnly
    );
    assert_eq!(
        SystemPageBudget::from_page_layout(ultra).surfaces,
        SystemSurfacePresentation::SingleColumn
    );

    let standard = PageLayoutBudget::for_viewport(frame(1180.0, 780.0));
    assert_eq!(standard.profile, LayoutProfile::Standard);
    assert_eq!(standard.vertical_space, VerticalSpace::Standard);
    assert_eq!(standard.page_padding, 16.0);
    assert_eq!(
        PerformancePageBudget::from_page_layout(standard).chart_inventory,
        PerformanceChartInventory::Full
    );
    assert_eq!(
        SystemPageBudget::from_page_layout(standard).surfaces,
        SystemSurfacePresentation::MultiColumn
    );

    // A panoramic but short window must retain wide horizontal composition
    // while independently collapsing height-hungry graph inventory.
    let wide_short = PageLayoutBudget::for_viewport(frame(2048.0, 540.0));
    assert_eq!(wide_short.profile, LayoutProfile::Wide);
    assert_eq!(wide_short.vertical_space, VerticalSpace::Constrained);
    assert_eq!(wide_short.navigation, NavigationPresentation::Labeled);
    let wide_short_performance = PerformancePageBudget::from_page_layout(wide_short);
    assert_eq!(
        wide_short_performance.device_navigation,
        DeviceNavigationPresentation::Sidebar
    );
    assert_eq!(
        wide_short_performance.details,
        PerformanceDetailsPresentation::Pinned
    );
    assert_eq!(
        wide_short_performance.chart_inventory,
        PerformanceChartInventory::AggregateOnly
    );

    // The converse: a tall but narrow window earns generous vertical capacity
    // without buying any horizontal profile.
    let tall_narrow = PageLayoutBudget::for_viewport(frame(720.0, 1200.0));
    assert_eq!(tall_narrow.profile, LayoutProfile::UltraCompact);
    assert_eq!(tall_narrow.vertical_space, VerticalSpace::Generous);

    assert_eq!(layout_profile(frame(900.0, 1200.0)), LayoutProfile::Compact);

    let vertical = PageLayoutBudget::for_frame(frame(900.0, 1200.0), NavOrientation::Vertical);
    assert_eq!(vertical.profile, LayoutProfile::Compact);
    assert_eq!(vertical.navigation, NavigationPresentation::IconOnly);
    let vertical_standard =
        PageLayoutBudget::for_frame(frame(1180.0, 780.0), NavOrientation::Vertical);
    assert_eq!(vertical_standard.profile, LayoutProfile::Compact);
    assert_eq!(
        vertical_standard.navigation,
        NavigationPresentation::Labeled
    );
}

#[test]
fn shared_thresholds_flip_exactly_at_the_gpui_boundaries() {
    let width_tiers = [
        (840.0, LayoutProfile::UltraCompact, LayoutProfile::Compact),
        (1080.0, LayoutProfile::Compact, LayoutProfile::Standard),
        (1600.0, LayoutProfile::Standard, LayoutProfile::Wide),
    ];
    for (threshold, below, at) in width_tiers {
        assert_eq!(
            layout_profile(frame(threshold - 1.0, 960.0)),
            below,
            "one pixel below {threshold} must stay {below:?}"
        );
        assert_eq!(
            layout_profile(frame(threshold, 960.0)),
            at,
            "exactly {threshold} must already be {at:?}"
        );
    }

    let height_tiers = [
        (700.0, VerticalSpace::Constrained, VerticalSpace::Standard),
        (960.0, VerticalSpace::Standard, VerticalSpace::Generous),
    ];
    for (threshold, below, at) in height_tiers {
        assert_eq!(
            vertical_space(frame(1180.0, threshold - 1.0)),
            below,
            "one pixel below {threshold} must stay {below:?}"
        );
        assert_eq!(
            vertical_space(frame(1180.0, threshold)),
            at,
            "exactly {threshold} must already be {at:?}"
        );
    }
}

#[test]
fn vertical_nav_subtraction_earns_the_body_profile_after_the_rail() {
    assert_eq!(nav_rail_width(NavigationPresentation::IconOnly), 54.0);
    assert_eq!(nav_rail_width(NavigationPresentation::Labeled), 144.0);

    // The labeled rail is earned only once the body keeps the UltraCompact
    // floor after subtracting the full rail: 984 - 144 == 840.
    let icon_only = PageLayoutBudget::for_frame(frame(983.9, 1200.0), NavOrientation::Vertical);
    assert_eq!(icon_only.navigation, NavigationPresentation::IconOnly);
    let labeled = PageLayoutBudget::for_frame(frame(984.0, 1200.0), NavOrientation::Vertical);
    assert_eq!(labeled.navigation, NavigationPresentation::Labeled);

    // Rail presentation and page profile are separate facts: 900px needs the
    // icon rail (900 - 144 < 840) while the remaining 846px body still earns
    // Compact. The 894px window is the exact-floor case (894 - 54 == 840).
    let narrow = PageLayoutBudget::for_frame(frame(900.0, 1200.0), NavOrientation::Vertical);
    assert_eq!(narrow.navigation, NavigationPresentation::IconOnly);
    assert_eq!(narrow.profile, LayoutProfile::Compact);
    let floor = PageLayoutBudget::for_frame(frame(894.0, 1200.0), NavOrientation::Vertical);
    assert_eq!(floor.navigation, NavigationPresentation::IconOnly);
    assert_eq!(floor.profile, LayoutProfile::Compact);
    let tiny = PageLayoutBudget::for_frame(frame(800.0, 1200.0), NavOrientation::Vertical);
    assert_eq!(tiny.navigation, NavigationPresentation::IconOnly);
    assert_eq!(tiny.profile, LayoutProfile::UltraCompact);

    // A horizontal nav consumes no body width: the frame budget equals the
    // full-viewport allocation.
    assert_eq!(
        PageLayoutBudget::for_frame(frame(1180.0, 780.0), NavOrientation::Horizontal),
        PageLayoutBudget::for_viewport(frame(1180.0, 780.0))
    );
}

#[test]
fn every_profile_and_vertical_pair_has_one_explicit_page_allocation() {
    for profile in [
        LayoutProfile::UltraCompact,
        LayoutProfile::Compact,
        LayoutProfile::Standard,
        LayoutProfile::Wide,
    ] {
        for vertical_capacity in [
            VerticalSpace::Constrained,
            VerticalSpace::Standard,
            VerticalSpace::Generous,
        ] {
            let layout = PageLayoutBudget {
                profile,
                vertical_space: vertical_capacity,
                page_padding: 16.0,
                navigation: NavigationPresentation::Labeled,
                chrome: ChromePresentation::SingleRow,
            };
            let performance = PerformancePageBudget::from_page_layout(layout);
            assert_eq!(
                performance.device_navigation,
                match profile {
                    LayoutProfile::UltraCompact => DeviceNavigationPresentation::Strip,
                    LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                        DeviceNavigationPresentation::Sidebar
                    }
                }
            );
            assert_eq!(
                performance.details,
                match profile {
                    LayoutProfile::UltraCompact => PerformanceDetailsPresentation::Hidden,
                    LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                        PerformanceDetailsPresentation::Pinned
                    }
                }
            );
            assert_eq!(
                performance.chart_inventory,
                match (profile, vertical_capacity) {
                    (LayoutProfile::UltraCompact, _) | (_, VerticalSpace::Constrained) => {
                        PerformanceChartInventory::AggregateOnly
                    }
                    (
                        LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide,
                        VerticalSpace::Standard | VerticalSpace::Generous,
                    ) => PerformanceChartInventory::Full,
                }
            );
            assert_eq!(
                SystemPageBudget::from_page_layout(layout).surfaces,
                match profile {
                    LayoutProfile::UltraCompact => SystemSurfacePresentation::SingleColumn,
                    LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                        SystemSurfacePresentation::MultiColumn
                    }
                }
            );
        }
    }
}

#[test]
fn chrome_presentation_matches_the_pre_port_single_row_breakpoint() {
    // The oracle is the pre-port ui.rs expression: wrapped below 1320px.
    let mut width = 240.0;
    while width <= 2200.0 {
        assert_eq!(
            ChromePresentation::for_width(width).is_wrapped(),
            width < 1320.0,
            "chrome flip must stay exact at {width}"
        );
        width += 1.0;
    }
    for width in [1319.0, 1319.999, 1320.0, 1320.001] {
        assert_eq!(
            ChromePresentation::for_width(width).is_wrapped(),
            width < 1320.0,
            "the 1320 boundary itself must not drift at {width}"
        );
    }

    // The full frame budget carries the same chrome fact for every height,
    // and a vertical rail never changes the window-width chrome decision.
    for height in [300.0, 480.0, 540.0, 700.0, 780.0, 960.0, 1200.0] {
        for width in [240.0, 720.0, 900.0, 1180.0, 1319.0, 1320.0, 1600.0, 2048.0] {
            let expected = ChromePresentation::for_width(width);
            assert_eq!(
                PageLayoutBudget::for_viewport(frame(width, height)).chrome,
                expected
            );
            assert_eq!(
                PageLayoutBudget::for_frame(frame(width, height), NavOrientation::Vertical).chrome,
                expected
            );
        }
    }
}

#[test]
fn wrapped_toolbar_columns_match_the_pre_port_chunk_breakpoint() {
    // The oracle is the pre-port performance.rs expression: three columns
    // below 560px, five from 560px up.
    let mut width = 320.0;
    while width <= 1400.0 {
        let expected = if width < 560.0 { 3 } else { 5 };
        assert_eq!(
            compact_toolbar_columns(width),
            expected,
            "toolbar chunk flip must stay exact at {width}"
        );
        width += 1.0;
    }
    for width in [559.0, 559.999, 560.0, 560.001] {
        let expected = if width < 560.0 { 3 } else { 5 };
        assert_eq!(compact_toolbar_columns(width), expected);
    }
}

#[test]
fn device_navigation_follows_the_frame_budget_slot_authority() {
    // The pre-port compact flag (820px width OR 540px height) is RETIRED: the
    // one authority is the typed slot allocation from the real tracked
    // viewport (GPUI `from_frame` parity). The strip appears when the sidebar
    // is hidden OR the frame cannot carry all three semantic slots — never
    // merely because a window is short.
    let cases = [
        // (width, height, sidebar_visible, expected_strip)
        (719.0, 700.0, true, true),  // workspace below the sidebar floor
        (803.0, 700.0, true, true),  // still below the UltraCompact slot floor
        (900.0, 700.0, true, false), // all three slots fit
        (1920.0, 1080.0, true, false),
        (1920.0, 1080.0, false, true), // hidden sidebar collapses to the strip
        // Short-but-wide windows KEEP the sidebar (the vertical ladder owns
        // height degradation, not the navigation axis).
        (1280.0, 420.0, true, false),
        (1000.0, 500.0, true, false),
    ];
    for (width, height, sidebar_visible, expected_strip) in cases {
        let budget = PerformancePageBudget::for_perf_frame(frame(width, height), sidebar_visible);
        let actual_strip = budget.device_navigation == DeviceNavigationPresentation::Strip;
        assert_eq!(
            actual_strip, expected_strip,
            "device navigation must follow the slot allocation at {width}x{height} (sidebar visible: {sidebar_visible})"
        );
        // The hidden sidebar keeps every device reachable through the strip;
        // a visible sidebar that cannot fit moves the same devices to the
        // strip — the preference is never silently discarded, only deferred.
        if !sidebar_visible {
            assert_eq!(
                budget.sidebar_width, 0.0,
                "a strip frame carries no sidebar slot"
            );
        }
    }
}

#[test]
fn root_view_renders_on_both_sides_of_the_chrome_boundary() {
    // The chrome seam replacement must not only classify identically; the
    // real view must still compose at the widths immediately around the old
    // 1320px flip.
    for width in [1319.0, 1320.0] {
        let mut app = crate::IcedApp::demo_for_capture();
        let _ = app.update(Message::WindowResized(frame(width, 780.0)));
        let _ = crate::ui::view(&app);
    }
}
