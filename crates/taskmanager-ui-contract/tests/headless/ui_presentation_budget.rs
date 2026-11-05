use crate::FrontendShape;
use crate::presentation::budget::{
    ContentSlot, PageLayoutBudget, PerformancePageBudget, PerformanceSlotRequest, SystemPageBudget,
};
use crate::presentation::thresholds::{LayoutBands, bands};
use crate::presentation::vocabulary::{
    ChromePresentation, DeviceNavigationPresentation, LayoutProfile, NavigationPresentation,
    PERFORMANCE_SIDEBAR_MAX_WIDTH, PerformanceChartInventory, PerformanceDetailsPresentation,
    PerformanceVerticalRunway, SystemSurfacePresentation, VerticalSpace,
    runway_from_vertical_space,
};

fn shape(frontend: FrontendShape) -> LayoutBands {
    bands(frontend)
}

#[test]
fn content_slot_classification_lands_on_a_stable_fixed_point() {
    let gpui = shape(FrontendShape::Gpui);
    let slot = ContentSlot::for_body(1_180.0, 780.0, &gpui);
    assert_eq!(slot.profile, LayoutProfile::Standard);
    assert_eq!(slot.page_padding, slot.profile.page_padding());
    assert_eq!(slot.width, 1_180.0 - slot.page_padding * 2.0);
    assert_eq!(slot.height, 780.0 - slot.page_padding * 2.0);
    assert_eq!(
        ContentSlot::for_body(slot.width, 780.0, &gpui).profile,
        slot.profile,
        "classifying the already-padded slot must not flip the bucket"
    );

    let no_room = ContentSlot::for_body(0.0, 0.0, &gpui);
    assert_eq!(no_room.profile, LayoutProfile::UltraCompact);
    assert_eq!(no_room.width, 0.0);
    assert_eq!(no_room.height, 0.0);
    assert_eq!(no_room.vertical_space, VerticalSpace::Constrained);
}

#[test]
fn one_vertical_frame_keeps_the_horizontal_and_vertical_axes_independent() {
    let gpui = shape(FrontendShape::Gpui);
    let wide_short = PageLayoutBudget::for_vertical_frame(2_400.0, 520.0, &gpui);
    let tall_narrow = PageLayoutBudget::for_vertical_frame(900.0, 1_300.0, &gpui);
    assert_eq!(wide_short.profile, LayoutProfile::Wide);
    assert_eq!(wide_short.vertical_space, VerticalSpace::Constrained);
    assert_eq!(tall_narrow.profile, LayoutProfile::UltraCompact);
    assert_eq!(tall_narrow.vertical_space, VerticalSpace::Generous);
    assert_eq!(
        SystemPageBudget::from_page_layout(wide_short).surfaces,
        SystemSurfacePresentation::MultiColumn
    );
    assert_eq!(
        SystemPageBudget::from_page_layout(tall_narrow).surfaces,
        SystemSurfacePresentation::SingleColumn
    );
}

#[test]
fn the_iced_chrome_band_decides_wrapping_without_touching_the_page_profile() {
    let iced = shape(FrontendShape::Iced);
    let wrapped = PageLayoutBudget::for_viewport(1_319.0, 720.0, &iced);
    let single_row = PageLayoutBudget::for_viewport(1_320.0, 720.0, &iced);
    assert_eq!(wrapped.profile, single_row.profile);
    assert_eq!(wrapped.navigation, single_row.navigation);
    assert_eq!(
        wrapped.chrome,
        Some(ChromePresentation::Wrapped),
        "the 1320 band is registered, not a literal in the page code"
    );
    assert_eq!(single_row.chrome, Some(ChromePresentation::SingleRow));
    assert_eq!(
        PageLayoutBudget::for_viewport(1_319.0, 720.0, &shape(FrontendShape::Gpui)).chrome,
        None,
        "GPUI has no width-based chrome band: it subtracts measured chrome"
    );
}

#[test]
fn navigation_is_decided_by_the_body_left_behind_the_rail_not_by_the_raw_window() {
    let iced = shape(FrontendShape::Iced);
    let icon_only = PageLayoutBudget::for_vertical_frame(980.0, 780.0, &iced);
    let labelled = PageLayoutBudget::for_vertical_frame(1_100.0, 780.0, &iced);
    assert_eq!(icon_only.navigation, NavigationPresentation::IconOnly);
    assert_eq!(labelled.navigation, NavigationPresentation::Labeled);
    assert!(
        icon_only.content_width < labelled.content_width,
        "the icon rail really leaves a wider body than the labelled rail"
    );
}

#[test]
fn performance_admission_never_starves_the_main_floor() {
    let gpui = shape(FrontendShape::Gpui);
    for width in [720.0, 840.0, 1_080.0, 1_280.0, 1_600.0, 2_400.0] {
        let budget = PerformancePageBudget::allocate(
            PerformanceSlotRequest {
                profile: LayoutProfile::for_content_width(width),
                workspace_width: width,
                content_height: 628.0,
                page_padding: 16.0,
                sidebar_visible: true,
                persisted_sidebar_width: None,
            },
            &gpui,
        );
        assert!(
            budget.main_width >= 0.0,
            "the main viewport never receives a negative allocation at {width}"
        );
        if budget.details == PerformanceDetailsPresentation::Pinned {
            assert!(
                budget.main_width
                    >= LayoutProfile::for_content_width(width).performance_main_floor()
                        - gpui.performance_stats_min_width,
                "a pinned rail still preserves a usable primary viewport at {width}"
            );
            assert!(
                budget.main_trailing_inset <= gpui.performance_stats_min_width,
                "the trailing inset is a bounded part of the slot budget at {width}"
            );
        }
        if budget.details == PerformanceDetailsPresentation::Pinned {
            assert!(
                budget.stats_width >= gpui.performance_stats_min_width
                    && budget.stats_width <= gpui.performance_stats_max_width,
                "a pinned statistics rail stays inside its registered floor/ceiling at {width}"
            );
            assert!(
                budget.main_width + budget.stats_width + budget.main_trailing_inset
                    <= width - budget.sidebar_width,
                "the pinned slots stay inside the remaining workspace at {width}"
            );
        } else {
            assert_eq!(
                budget.stats_width, gpui.performance_stats_min_width,
                "a non-pinned rail reports its floor, not a fabricated width, at {width}"
            );
        }
        assert_eq!(budget.workspace_width, width);
    }
}

#[test]
fn a_stacked_rail_drops_whole_when_the_registered_height_floor_cannot_fit() {
    let gpui = shape(FrontendShape::Gpui);
    // 220 (sidebar floor) + 236 (stats floor) + 360 (main floor) + 12 gap
    // = 828 admits the sidebar; 828-220-360-12 = 236 leaves no room for a
    // pinned rail, so the width path lands on Stacked and the height floor
    // decides whether it may stay.
    let stacked_width = 600.0;
    let request = PerformanceSlotRequest {
        profile: LayoutProfile::Compact,
        workspace_width: stacked_width,
        content_height: 519.0,
        page_padding: 16.0,
        sidebar_visible: true,
        persisted_sidebar_width: None,
    };
    let hidden = PerformancePageBudget::allocate(request, &gpui);
    assert_eq!(hidden.details, PerformanceDetailsPresentation::Hidden);
    let admitted = PerformancePageBudget::allocate(
        PerformanceSlotRequest {
            content_height: 520.0,
            ..request
        },
        &gpui,
    );
    assert_eq!(admitted.details, PerformanceDetailsPresentation::Stacked);

    let iced = shape(FrontendShape::Iced);
    let no_floor = PerformancePageBudget::allocate(request, &iced);
    assert_eq!(
        no_floor.details,
        PerformanceDetailsPresentation::Stacked,
        "Iced registers no stacked-height floor, so the rail stays at every height"
    );
}

#[test]
fn the_persisted_sidebar_preference_is_clamped_inside_the_frame() {
    let gpui = shape(FrontendShape::Gpui);
    let budget = PerformancePageBudget::allocate(
        PerformanceSlotRequest {
            profile: LayoutProfile::Wide,
            workspace_width: 1_400.0,
            content_height: 780.0,
            page_padding: 16.0,
            sidebar_visible: true,
            persisted_sidebar_width: Some(900.0),
        },
        &gpui,
    );
    assert_eq!(
        budget.device_navigation,
        DeviceNavigationPresentation::Sidebar
    );
    assert!(budget.sidebar_width >= gpui.performance_sidebar_min_width);
    assert!(budget.sidebar_width <= PERFORMANCE_SIDEBAR_MAX_WIDTH);
    assert!(
        budget.sidebar_width + budget.stats_width + budget.main_width <= 1_400.0,
        "the three slots plus their gap never exceed the measured workspace"
    );

    let hidden = PerformancePageBudget::allocate(
        PerformanceSlotRequest {
            profile: LayoutProfile::Wide,
            workspace_width: 1_400.0,
            content_height: 780.0,
            page_padding: 16.0,
            sidebar_visible: false,
            persisted_sidebar_width: Some(900.0),
        },
        &gpui,
    );
    assert_eq!(
        hidden.device_navigation,
        DeviceNavigationPresentation::Strip
    );
    assert_eq!(hidden.sidebar_width, 0.0);
}

#[test]
fn the_chart_inventory_folds_the_measured_height_rung_not_the_coarse_bucket() {
    let gpui = shape(FrontendShape::Gpui);
    let iced = shape(FrontendShape::Iced);
    let request = PerformanceSlotRequest {
        profile: LayoutProfile::Standard,
        workspace_width: 1_400.0,
        content_height: 560.0,
        page_padding: 16.0,
        sidebar_visible: true,
        persisted_sidebar_width: None,
    };
    let gpui_budget = PerformancePageBudget::allocate(request, &gpui);
    let iced_budget = PerformancePageBudget::allocate(request, &iced);
    assert_eq!(gpui_budget.vertical, PerformanceVerticalRunway::Charts);
    assert_eq!(
        gpui_budget.chart_inventory,
        PerformanceChartInventory::Full,
        "GPUI registers the 520 Charts rung"
    );
    assert_eq!(iced_budget.vertical, PerformanceVerticalRunway::Core);
    assert_eq!(
        iced_budget.chart_inventory,
        PerformanceChartInventory::AggregateOnly,
        "Iced registers the 650 Charts rung, so 560 still drops the per-core matrix"
    );
}

#[test]
fn the_coarse_constructor_is_a_pure_fold_of_the_typed_buckets() {
    let gpui = shape(FrontendShape::Gpui);
    for profile in LayoutProfile::ALL {
        for vertical in VerticalSpace::ALL {
            let page = PageLayoutBudget {
                profile,
                vertical_space: vertical,
                page_padding: profile.page_padding(),
                navigation: profile.navigation_default(),
                content_width: 0.0,
                content_height: 0.0,
                chrome: None,
            };
            let budget = PerformancePageBudget::coarse(page, &gpui);
            assert_eq!(budget.content_height, 0.0);
            assert_eq!(budget.workspace_width, 0.0);
            assert_eq!(budget.main_width, 0.0);
            assert_eq!(budget.main_trailing_inset, page.page_padding);
            assert_eq!(
                budget.vertical,
                runway_from_vertical_space(vertical),
                "the coarse path maps {vertical:?} to the registered legacy rung"
            );
        }
    }
}
