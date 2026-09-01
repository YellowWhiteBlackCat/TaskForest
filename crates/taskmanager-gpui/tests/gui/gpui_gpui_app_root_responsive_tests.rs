use gpui::{px, size};

use super::{
    DeviceNavigationPresentation, FrameBudget, FrameChromeBudget, LayoutProfile,
    NavigationPresentation, PageLayoutBudget, PerformanceChartInventory,
    PerformanceDetailsPresentation, PerformancePageBudget, SystemPageBudget,
    SystemSurfacePresentation, VerticalSpace, layout_profile, parse_window_size,
    settings_content_max_height,
};
use crate::gpui_app::root::NavOrientation;

#[test]
fn typed_layout_profiles_keep_horizontal_and_vertical_capacity_independent() {
    let ultra = PageLayoutBudget::for_viewport(size(px(720.0), px(480.0)));
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

    let standard = PageLayoutBudget::for_viewport(size(px(1180.0), px(780.0)));
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
    let wide_short = PageLayoutBudget::for_viewport(size(px(2048.0), px(540.0)));
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

    assert_eq!(
        layout_profile(size(px(900.0), px(1200.0))),
        LayoutProfile::Compact
    );

    let vertical =
        PageLayoutBudget::for_frame(size(px(900.0), px(1200.0)), NavOrientation::Vertical);
    // The profile is now resolved against the post-rail, post-padding content
    // slot. At this size the icon rail plus its outer inset leaves an 818px
    // page canvas, which is intentionally UltraCompact even though the outer
    // body is 834px wide.
    assert_eq!(vertical.profile, LayoutProfile::UltraCompact);
    assert_eq!(vertical.navigation, NavigationPresentation::IconOnly);
    let vertical_standard =
        PageLayoutBudget::for_frame(size(px(1180.0), px(780.0)), NavOrientation::Vertical);
    assert_eq!(vertical_standard.profile, LayoutProfile::Compact);
    assert_eq!(
        vertical_standard.navigation,
        NavigationPresentation::Labeled
    );
}

#[test]
fn root_frame_budget_deducts_shell_before_page_policy() {
    let frame = FrameBudget::for_root(
        size(px(1180.0), px(780.0)),
        NavOrientation::Horizontal,
        FrameChromeBudget::new(36.0, false, false),
    );
    assert_eq!(frame.navigation, NavigationPresentation::Labeled);
    assert_eq!(frame.body.width, px(1180.0));
    assert_eq!(frame.body.height, px(730.0));
    assert_eq!(frame.content.size.width, px(1148.0));
    assert_eq!(frame.content.size.height, px(698.0));
    assert_eq!(frame.content.profile, LayoutProfile::Standard);
    assert_eq!(frame.content.vertical_space, VerticalSpace::Standard);

    let vertical = FrameBudget::for_root(
        size(px(1180.0), px(780.0)),
        NavOrientation::Vertical,
        FrameChromeBudget::new(36.0, false, false),
    );
    assert_eq!(vertical.navigation, NavigationPresentation::Labeled);
    assert_eq!(vertical.navigation_width, 156.0);
    assert_eq!(vertical.body.width, px(1024.0));
    assert_eq!(vertical.content.size.width, px(1000.0));

    let with_csd_and_alert = FrameBudget::for_root(
        size(px(1180.0), px(780.0)),
        NavOrientation::Horizontal,
        FrameChromeBudget::new(36.0, true, true),
    );
    assert_eq!(with_csd_and_alert.body.height, px(662.0));
    assert_eq!(with_csd_and_alert.content.size.height, px(630.0));
    assert_eq!(
        with_csd_and_alert.content.vertical_space,
        VerticalSpace::Constrained
    );
}

#[test]
fn performance_slots_reflow_as_a_unit_instead_of_starving_the_main_view() {
    let medium = FrameBudget::for_root(
        size(px(900.0), px(780.0)),
        NavOrientation::Horizontal,
        FrameChromeBudget::new(36.0, false, false),
    );
    let medium_perf = PerformancePageBudget::from_frame(medium, true, 320.0);
    assert_eq!(
        medium_perf.device_navigation,
        DeviceNavigationPresentation::Sidebar
    );
    assert_eq!(medium_perf.details, PerformanceDetailsPresentation::Pinned);
    assert!(medium_perf.sidebar_width < 320.0);
    assert!(medium_perf.main_width >= super::PERFORMANCE_MAIN_MIN_WIDTH);

    let tight = FrameBudget::for_root(
        size(px(560.0), px(780.0)),
        NavOrientation::Horizontal,
        FrameChromeBudget::new(36.0, false, false),
    );
    let tight_perf = PerformancePageBudget::from_frame(tight, true, 260.0);
    assert_eq!(
        tight_perf.device_navigation,
        DeviceNavigationPresentation::Strip
    );
    assert_eq!(tight_perf.details, PerformanceDetailsPresentation::Stacked);
    assert_eq!(tight_perf.sidebar_width, 0.0);
    assert!(tight_perf.main_width >= super::PERFORMANCE_MAIN_MIN_WIDTH);

    let short_vertical = FrameBudget::for_root(
        size(px(560.0), px(480.0)),
        NavOrientation::Horizontal,
        FrameChromeBudget::new(36.0, false, false),
    );
    let short_vertical_perf = PerformancePageBudget::from_frame(short_vertical, true, 260.0);
    assert_eq!(
        short_vertical_perf.details,
        PerformanceDetailsPresentation::Hidden,
        "a short stacked layout hides the rail instead of clipping its bottom"
    );

    let hidden_sidebar = PerformancePageBudget::from_frame(medium, false, 320.0);
    assert_eq!(
        hidden_sidebar.device_navigation,
        DeviceNavigationPresentation::Strip
    );
    assert_eq!(hidden_sidebar.sidebar_width, 0.0);
    assert_eq!(
        hidden_sidebar.details,
        PerformanceDetailsPresentation::Pinned
    );

    let invalid_preference = PerformancePageBudget::from_frame(medium, true, f32::NAN);
    assert!(invalid_preference.sidebar_width.is_finite());
}

#[test]
fn capture_size_and_settings_height_are_bounded() {
    assert_eq!(
        parse_window_size("720x480"),
        Some(size(px(720.0), px(480.0)))
    );
    assert_eq!(
        parse_window_size("300x200"),
        Some(size(px(720.0), px(480.0)))
    );
    assert_eq!(parse_window_size("bad"), None);
    assert_eq!(
        settings_content_max_height(size(px(720.0), px(480.0))),
        280.0
    );
    assert_eq!(
        settings_content_max_height(size(px(1180.0), px(780.0))),
        580.0
    );
}

#[test]
fn every_performance_and_system_profile_state_has_one_explicit_allocation() {
    for profile in [
        LayoutProfile::UltraCompact,
        LayoutProfile::Compact,
        LayoutProfile::Standard,
        LayoutProfile::Wide,
    ] {
        for vertical_space in [
            VerticalSpace::Constrained,
            VerticalSpace::Standard,
            VerticalSpace::Generous,
        ] {
            let layout = PageLayoutBudget {
                profile,
                vertical_space,
                page_padding: 16.0,
                navigation: NavigationPresentation::Labeled,
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
                match (profile, vertical_space) {
                    (LayoutProfile::UltraCompact, _) | (_, VerticalSpace::Constrained) => {
                        PerformanceChartInventory::AggregateOnly
                    }
                    _ => PerformanceChartInventory::Full,
                }
            );
            assert_eq!(performance.main_trailing_inset, layout.page_padding);
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
