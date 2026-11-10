use super::*;
use crate::presentation::thresholds::{LayoutAxis, number};

#[test]
fn the_horizontal_bands_classify_on_their_edges() {
    assert_eq!(
        LayoutProfile::for_content_width(ULTRA_COMPACT_CONTENT_WIDTH - 1.0),
        LayoutProfile::UltraCompact
    );
    assert_eq!(
        LayoutProfile::for_content_width(ULTRA_COMPACT_CONTENT_WIDTH),
        LayoutProfile::Compact,
        "the band boundary belongs to the roomier bucket"
    );
    assert_eq!(
        LayoutProfile::for_content_width(COMPACT_CONTENT_WIDTH - 1.0),
        LayoutProfile::Compact
    );
    assert_eq!(
        LayoutProfile::for_content_width(COMPACT_CONTENT_WIDTH),
        LayoutProfile::Standard
    );
    assert_eq!(
        LayoutProfile::for_content_width(WIDE_CONTENT_WIDTH - 1.0),
        LayoutProfile::Standard
    );
    assert_eq!(
        LayoutProfile::for_content_width(WIDE_CONTENT_WIDTH),
        LayoutProfile::Wide
    );
}

#[test]
fn the_padding_token_is_a_pure_fold_of_the_bucket_it_describes() {
    for profile in LayoutProfile::ALL {
        let padding = profile.page_padding();
        assert!(padding > 0.0, "{profile:?} must spend a real inset");
        assert_eq!(
            profile.admits_prose_detail(),
            profile != LayoutProfile::UltraCompact,
            "prose detail admits exactly where the column stops wrapping hard"
        );
    }
    assert!(
        LayoutProfile::UltraCompact.page_padding() < LayoutProfile::Wide.page_padding(),
        "a tighter slot never spends a wider inset"
    );
}

#[test]
fn the_runway_ladder_degrades_in_one_direction() {
    let charts_band = 520.0;
    assert_eq!(
        PerformanceVerticalRunway::for_content_height(charts_band - 1.0, charts_band),
        PerformanceVerticalRunway::Core
    );
    assert_eq!(
        PerformanceVerticalRunway::for_content_height(charts_band, charts_band),
        PerformanceVerticalRunway::Charts
    );
    assert_eq!(
        PerformanceVerticalRunway::for_content_height(
            PERFORMANCE_RUNWAY_CORE_FLOOR - 1.0,
            charts_band
        ),
        PerformanceVerticalRunway::Floor,
        "below the composed core-stack floor the header band drops first"
    );

    let mut last_carried_core = false;
    let mut last_carried_below = false;
    for rung in PerformanceVerticalRunway::ALL {
        assert!(
            !last_carried_core || rung.carries_core_stack(),
            "a roomier rung never drops what the tighter one kept"
        );
        assert!(
            !last_carried_below || rung.carries_below(),
            "the below band is admitted monotonically"
        );
        last_carried_core = last_carried_core || rung.carries_core_stack();
        last_carried_below = last_carried_below || rung.carries_below();
    }
    assert!(
        !PerformanceVerticalRunway::Core.carries_below(),
        "Core is headline-only so the no-scroll Performance contract stays explicit"
    );
}

#[test]
fn the_chart_inventory_folds_both_axes_without_conflating_them() {
    for profile in LayoutProfile::ALL {
        for rung in PerformanceVerticalRunway::ALL {
            let inventory = chart_inventory_from_axes(profile, rung);
            let expected = if matches!(
                (profile, rung),
                (
                    LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide,
                    PerformanceVerticalRunway::Charts
                )
            ) {
                PerformanceChartInventory::Full
            } else {
                PerformanceChartInventory::AggregateOnly
            };
            assert_eq!(inventory, expected, "{profile:?} x {rung:?}");
        }
    }
}

#[test]
fn each_navigation_form_names_exactly_one_rail_width() {
    assert_eq!(
        NavigationPresentation::IconOnly.rail_width(),
        NAV_RAIL_COMPACT_WIDTH
    );
    assert_eq!(NavigationPresentation::Labeled.rail_width(), NAV_RAIL_WIDTH);
    assert!(
        NavigationPresentation::IconOnly.rail_width()
            < NavigationPresentation::Labeled.rail_width(),
        "the icon rail never takes more room than the labelled rail"
    );
}

#[test]
fn chrome_presentation_stays_unspent_without_a_band() {
    assert_eq!(ChromePresentation::for_width(4_000.0, None), None);
    let band = Some(1_320.0);
    assert_eq!(
        ChromePresentation::for_width(1_319.0, band),
        Some(ChromePresentation::Wrapped)
    );
    assert_eq!(
        ChromePresentation::for_width(1_320.0, band),
        Some(ChromePresentation::SingleRow)
    );
    assert!(ChromePresentation::Wrapped.is_wrapped());
    assert!(!ChromePresentation::SingleRow.is_wrapped());
}

#[test]
fn the_page_slot_folds_are_exhaustive_over_the_grid() {
    for profile in LayoutProfile::ALL {
        let device = device_navigation_from_profile(profile);
        let details = details_from_profile(profile);
        let surfaces = system_surfaces_from_profile(profile);
        match profile {
            LayoutProfile::UltraCompact => {
                assert_eq!(device, DeviceNavigationPresentation::Strip);
                assert_eq!(details, PerformanceDetailsPresentation::Hidden);
                assert_eq!(surfaces, SystemSurfacePresentation::SingleColumn);
            }
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                assert_eq!(device, DeviceNavigationPresentation::Sidebar);
                assert_eq!(details, PerformanceDetailsPresentation::Pinned);
                assert_eq!(surfaces, SystemSurfacePresentation::MultiColumn);
            }
        }
    }
    for vertical in VerticalSpace::ALL {
        let rung = runway_from_vertical_space(vertical);
        let expected = match vertical {
            VerticalSpace::Constrained => PerformanceVerticalRunway::Core,
            VerticalSpace::Standard | VerticalSpace::Generous => PerformanceVerticalRunway::Charts,
        };
        assert_eq!(rung, expected, "{vertical:?} must map to the legacy rung");
        assert!(
            rung.carries_core_stack(),
            "the coarse constructor never drops the core stack"
        );
    }
}

#[test]
fn the_shared_bands_are_stated_once_at_their_exact_values() {
    assert_eq!(NAV_RAIL_WIDTH, 144.0);
    assert_eq!(NAV_RAIL_COMPACT_WIDTH, 54.0);
    assert_eq!(ULTRA_COMPACT_CONTENT_WIDTH, 840.0);
    assert_eq!(COMPACT_CONTENT_WIDTH, 1080.0);
    assert_eq!(WIDE_CONTENT_WIDTH, 1600.0);
    assert_eq!(PERFORMANCE_MAIN_MIN_WIDTH, 360.0);
    assert_eq!(PERFORMANCE_ULTRA_COMPACT_MAIN_MIN_WIDTH, 320.0);
    assert_eq!(PERFORMANCE_SLOT_GAP, 12.0);
    assert_eq!(PERFORMANCE_SIDEBAR_MAX_WIDTH, 460.0);
    assert_eq!(PERFORMANCE_STATS_STACK_HEIGHT, 220.0);
    assert_eq!(PERFORMANCE_RUNWAY_CORE_FLOOR, 380.0);
    assert_eq!(BOTTOM_SAFETY_BAND, 8.0);
    const {
        // The smallest admissible window is a per-shape band and lives in the
        // threshold registry, not here: this module owns the bands every
        // shape spends, and a shape that spends no window floor (the terminal
        // owns its viewport, Bevy takes the compositor's size) cannot be
        // expressed by a shared constant.
        let min_width = number(crate::FrontendShape::Gpui, LayoutAxis::MinimumWindowWidth);
        let min_height = number(crate::FrontendShape::Gpui, LayoutAxis::MinimumWindowHeight);
        assert!(MAX_WINDOW_WIDTH > min_width && MAX_WINDOW_HEIGHT > min_height);
        assert!(
            min_width - NAV_RAIL_WIDTH - BOTTOM_SAFETY_BAND < ULTRA_COMPACT_CONTENT_WIDTH,
            "the smallest admissible window must still reach the Ultra-Compact rung, so the \
             degradation path is exercised rather than unreachable"
        );
    }
}
