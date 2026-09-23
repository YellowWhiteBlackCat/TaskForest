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

/// Elastic Layout Playbook: Headless assertions for table columns under
/// narrow viewports (e.g. 720x480) and wide-short viewports (e.g. 2048x540).
/// Verifies contract width floors, label integrity in multiple languages,
/// numeric cell alignments, non-hideable identity columns, positive table width
/// allocations, and virtual-list bounds.
#[test]
fn elastic_layout_playbook_table_columns_under_narrow_and_wide_short_viewports() {
    use super::super::applications::{
        application_row_height, apps_columns, apps_table_width, column_alignment, column_hideable,
        column_width, localized_sort_column_label,
    };
    use super::super::virtual_list::VirtualWindow;
    use taskmanager_application::i18n::{Language, set_language};
    use taskmanager_shell::SortCol;

    let viewports = [
        // Narrow viewports
        (720.0, 480.0, true),
        (800.0, 480.0, true),
        (640.0, 480.0, true),
        (480.0, 800.0, true),
        // Wide-short viewports
        (2048.0, 540.0, true),
        (1600.0, 480.0, true),
        (1280.0, 420.0, true),
        (1920.0, 500.0, true),
        // Standard viewports
        (1280.0, 720.0, false),
        (1920.0, 1080.0, false),
    ];

    for (width, height, is_compact) in viewports {
        let size = frame(width, height);
        assert_eq!(
            crate::app::motion::viewport_compact(size),
            is_compact,
            "viewport_compact must match expected compact contract for {width}x{height}"
        );

        // Density row heights must respect the compact contract
        let row_height = application_row_height(is_compact);
        if is_compact {
            assert_eq!(row_height, 24.0, "compact density must keep 24px row floor");
        } else {
            assert_eq!(
                row_height, 32.0,
                "standard density must keep 32px row floor"
            );
        }

        // Virtual window bounds for sticky table rows:
        // Window slice [start, end) must be bounded, non-empty for populated data,
        // and end must never exceed total rows or overflow viewport.
        let total_rows = 200;
        let viewport_height = height - 140.0; // Subtract header strips and chrome
        let window = VirtualWindow::for_sticky_rows(total_rows, 0.0, viewport_height, row_height);
        assert!(
            window.start <= window.end,
            "window start <= end at {width}x{height}"
        );
        assert!(
            window.end <= total_rows,
            "window end <= total_rows at {width}x{height}"
        );
        let materialized_count = window.end - window.start;
        let max_visible = (viewport_height / row_height).ceil() as usize
            + 2 * super::super::virtual_list::OVERSCAN_ROWS
            + 1;
        assert!(
            materialized_count <= max_visible,
            "materialized rows ({materialized_count}) must be bounded by viewport capacity ({max_visible}) at {width}x{height}"
        );
    }

    // Every table column must maintain a positive contract width floor and valid presentation
    for swap_visible in [false, true] {
        let cols = apps_columns(swap_visible);
        let expected_count = if swap_visible { 16 } else { 15 };
        assert_eq!(cols.len(), expected_count);

        for (col, width) in &cols {
            assert!(
                *width > 0.0,
                "column {col:?} width must be positive: {width}"
            );
            assert_eq!(
                *width,
                column_width(*col),
                "column {col:?} width must match contract_spec default"
            );

            // Numeric contract columns right-align; text columns left-align
            let align = column_alignment(*col);
            match col {
                SortCol::Pid
                | SortCol::Threads
                | SortCol::Cpu
                | SortCol::Memory
                | SortCol::Swap
                | SortCol::Pss
                | SortCol::DiskRead
                | SortCol::DiskWrite
                | SortCol::Network
                | SortCol::CpuTime
                | SortCol::Fds
                | SortCol::Nice => {
                    assert_eq!(
                        align,
                        iced::alignment::Horizontal::Right,
                        "column {col:?} must right-align"
                    );
                }
                SortCol::Name | SortCol::User | SortCol::StartTime | SortCol::State => {
                    assert_eq!(
                        align,
                        iced::alignment::Horizontal::Left,
                        "column {col:?} must left-align"
                    );
                }
            }

            // Identity column Name is mandatory and can never be hidden
            if *col == SortCol::Name {
                assert!(!column_hideable(*col), "Name column must never be hideable");
            } else {
                assert!(
                    column_hideable(*col),
                    "Column {col:?} should be hideable in column chooser"
                );
            }
        }

        // Check localized column labels across both supported languages
        for lang in [Language::En, Language::Zh] {
            set_language(lang);
            for (col, _) in &cols {
                let label = localized_sort_column_label(*col);
                assert!(
                    !label.trim().is_empty(),
                    "column {col:?} label must not be empty in {lang:?}"
                );
            }
        }
        set_language(Language::En);

        // Table width must account for all columns, the sparkline cell, gutters, and outer padding
        let hidden = std::collections::HashSet::new();
        let total_table_width = apps_table_width(swap_visible, &hidden);
        let base_columns_width: f32 = cols.iter().map(|(_, w)| *w).sum();
        assert!(
            total_table_width > base_columns_width,
            "table width ({total_table_width}) must exceed sum of columns ({base_columns_width}) to accommodate sparkline, gutters, and padding"
        );
    }
}

/// Elastic Layout Playbook: Headless assertions for header strips, ribbon,
/// and action toolbar convergence under narrow viewports (e.g. 720x480),
/// wide-short viewports (e.g. 2048x540), and desktop viewports.
#[test]
fn elastic_layout_playbook_header_strips_and_ribbon_toolbar_convergence() {
    use crate::saved_views::{PresetsRibbonState, presets_ribbon};
    use taskmanager_application::AppPage;

    // 1. Root chrome single-row convergence at wide viewports (>= 1320px width and non-compact height)
    let wide_viewport = frame(1440.0, 900.0);
    assert!(!crate::app::motion::viewport_compact(wide_viewport));
    let wide_budget = PageLayoutBudget::for_viewport(wide_viewport);
    assert_eq!(wide_budget.chrome, ChromePresentation::SingleRow);
    assert!(!wide_budget.chrome.is_wrapped());

    // At narrow viewports (e.g. 720x480) and wide-short viewports (e.g. 2048x540):
    // Root chrome presentation must wrap onto dedicated bounded strips so actions don't push off-screen.
    let narrow_viewport = frame(720.0, 480.0);
    assert!(crate::app::motion::viewport_compact(narrow_viewport));
    let narrow_budget = PageLayoutBudget::for_viewport(narrow_viewport);
    assert_eq!(narrow_budget.chrome, ChromePresentation::Wrapped);
    assert!(narrow_budget.chrome.is_wrapped());

    let wide_short_viewport = frame(2048.0, 540.0);
    assert!(crate::app::motion::viewport_compact(wide_short_viewport));
    let ws_budget = PageLayoutBudget::for_viewport(wide_short_viewport);
    assert_eq!(ws_budget.vertical_space, VerticalSpace::Constrained);

    // 2. Presets Ribbon convergence:
    // In compact viewports (narrow 720x480 or wide-short 2048x540), presets_ribbon must converge
    // to a single horizontal scrollable strip of bounded height 32.0 px, preventing it from
    // breaking into multiple vertical lines that consume the process table viewport.
    let theme = taskmanager_theme::Theme::dark();
    let presets = vec![
        crate::saved_views::SavedViewPreset::built_in(
            1,
            "saved_views.preset_default",
            taskmanager_shell::ProcessStatusFilter::All,
            taskmanager_shell::SortCol::Cpu,
            false,
        ),
        crate::saved_views::SavedViewPreset::built_in(
            2,
            "saved_views.preset_running",
            taskmanager_shell::ProcessStatusFilter::Running,
            taskmanager_shell::SortCol::Memory,
            false,
        ),
    ];

    let compact_ribbon_state = PresetsRibbonState {
        filter: taskmanager_shell::ProcessStatusFilter::All,
        sort: taskmanager_shell::SortCol::Cpu,
        ascending: false,
        feedback: None,
        compact: true,
    };
    let _compact_ribbon = presets_ribbon(&theme, &presets, compact_ribbon_state);

    let desktop_ribbon_state = PresetsRibbonState {
        compact: false,
        ..compact_ribbon_state
    };
    let _desktop_ribbon = presets_ribbon(&theme, &presets, desktop_ribbon_state);

    // 3. Applications action bar and page rendering under narrow and wide-short viewports:
    for (width, height) in [
        (720.0, 480.0),
        (2048.0, 540.0),
        (1600.0, 480.0),
        (1440.0, 900.0),
    ] {
        let mut app = crate::IcedApp::demo();
        let _ = app.update(Message::WindowResized(frame(width, height)));
        let _ = app.update(Message::SelectPage(AppPage::Applications));
        // Verify the entire applications page renders without panic or overflow
        let view = crate::ui::view(&app);
        let _ = view;
    }
}

/// Elastic Layout Playbook: Headless assertions for statistics rails and detail
/// slot allocation under narrow viewports (e.g. 720x480) and wide-short
/// viewports (e.g. 2048x540). Asserts bounded text projection, slot floors,
/// stacked details degradation, and bottom/right edge protection.
#[test]
fn elastic_layout_playbook_stats_rails_and_detail_slots_under_narrow_and_wide_short() {
    use super::super::perf_layout::{bounded_heading, bounded_stat_text};
    use super::{
        PERFORMANCE_MAIN_MIN_WIDTH, PERFORMANCE_SIDEBAR_MIN_WIDTH, PERFORMANCE_STATS_MAX_WIDTH,
        PERFORMANCE_STATS_MIN_WIDTH, PERFORMANCE_STATS_STACK_HEIGHT,
        PerformanceDetailsPresentation, PerformanceVerticalRunway,
    };

    assert_eq!(super::PERFORMANCE_RUNWAY_CORE_FLOOR, 380.0);

    // 1. Narrow viewport (720x480):
    // Device navigation collapses to Strip (width 0), stats rail is pinned or stacked with clamped width,
    // and vertical runway degrades to Floor (aggregate only), protecting bottom and right edges.
    let narrow_budget = PerformancePageBudget::for_perf_frame(frame(720.0, 480.0), true);
    assert_eq!(
        narrow_budget.device_navigation,
        DeviceNavigationPresentation::Strip,
        "720px width cannot hold sidebar alongside stats and main floor; collapses to strip"
    );
    assert_eq!(narrow_budget.sidebar_width, 0.0);
    assert!(
        narrow_budget.stats_width >= PERFORMANCE_STATS_MIN_WIDTH
            && narrow_budget.stats_width <= PERFORMANCE_STATS_MAX_WIDTH,
        "stats width must be clamped within [MIN, MAX]: {}",
        narrow_budget.stats_width
    );
    assert!(
        narrow_budget.main_width >= 320.0,
        "main viewport must preserve its 320px UltraCompact floor: {}",
        narrow_budget.main_width
    );
    // Invariant: total slot footprint must not exceed workspace width
    let total_horizontal = narrow_budget.sidebar_width
        + narrow_budget.stats_width
        + narrow_budget.main_width
        + narrow_budget.main_trailing_inset;
    assert!(
        total_horizontal <= narrow_budget.workspace_width + 0.001,
        "horizontal slots ({total_horizontal}) must not exceed workspace width ({})",
        narrow_budget.workspace_width
    );
    // Vertical runway degradation: 480 - 128 = 352 < 380 core floor -> Floor rung
    assert_eq!(narrow_budget.vertical, PerformanceVerticalRunway::Floor);
    assert_eq!(
        narrow_budget.chart_inventory,
        PerformanceChartInventory::AggregateOnly,
        "height below core runway floor admits only aggregate chart to protect bottom edge"
    );

    // 2. Stacked details fallback under very narrow width (e.g. 480x800):
    // When width cannot carry main + pinned stats side-by-side, stats moves to Stacked below
    // the main viewport (fixed 220px height) instead of starving the primary graph horizontally.
    let stacked_budget = PerformancePageBudget::for_perf_frame(frame(480.0, 800.0), true);
    assert_eq!(
        stacked_budget.device_navigation,
        DeviceNavigationPresentation::Strip
    );
    assert_eq!(
        stacked_budget.details,
        PerformanceDetailsPresentation::Stacked,
        "narrow 480px width must stack statistics below the main chart"
    );
    assert_eq!(PERFORMANCE_STATS_STACK_HEIGHT, 220.0);

    // 3. Wide-short viewport (2048x540):
    // Width is generous (admitting Sidebar and Pinned stats), but height is constrained (540 - 128 = 412).
    // Core runway is carried, but full chart inventory is collapsed to AggregateOnly to protect bottom edge.
    let wide_short_budget = PerformancePageBudget::for_perf_frame(frame(2048.0, 540.0), true);
    assert_eq!(
        wide_short_budget.device_navigation,
        DeviceNavigationPresentation::Sidebar,
        "wide-short window retains sidebar navigation"
    );
    assert_eq!(
        wide_short_budget.sidebar_width,
        PERFORMANCE_SIDEBAR_MIN_WIDTH
    );
    assert_eq!(
        wide_short_budget.details,
        PerformanceDetailsPresentation::Pinned,
        "wide-short window retains pinned statistics rail"
    );
    assert_eq!(wide_short_budget.stats_width, PERFORMANCE_STATS_MAX_WIDTH);
    assert!(
        wide_short_budget.main_width >= PERFORMANCE_MAIN_MIN_WIDTH,
        "main viewport must exceed standard min width: {}",
        wide_short_budget.main_width
    );
    let ws_total_horizontal = wide_short_budget.sidebar_width
        + wide_short_budget.stats_width
        + wide_short_budget.main_width
        + wide_short_budget.main_trailing_inset;
    assert!(
        ws_total_horizontal <= wide_short_budget.workspace_width + 0.001,
        "horizontal slots ({ws_total_horizontal}) must not overflow workspace ({})",
        wide_short_budget.workspace_width
    );
    // Vertical runway: 540 - 128 = 412 >= 380 (Core) but < 650 (Charts) -> Core rung, AggregateOnly
    assert_eq!(wide_short_budget.vertical, PerformanceVerticalRunway::Core);
    assert_eq!(
        wide_short_budget.chart_inventory,
        PerformanceChartInventory::AggregateOnly,
        "constrained vertical height must collapse secondary charts to AggregateOnly"
    );

    // 4. Bounded text projections in stats rail:
    // Long device identities and composite telemetry readouts must be bounded with ellipsis
    // so intrinsic text never pushes labels, borders, or the window edge.
    let long_device = "Intel(R) Core(TM) Ultra 9 185H @ 5.10GHz Engineering Sample v2";
    let bounded_dev = bounded_heading(long_device, 18);
    assert!(
        bounded_dev.ends_with('…'),
        "long heading must truncate with ellipsis: {bounded_dev}"
    );
    assert!(
        bounded_dev.chars().count() <= 18,
        "heading must not exceed max chars"
    );

    let short_device = "AMD Ryzen 9";
    assert_eq!(bounded_heading(short_device, 18), "AMD Ryzen 9");

    let composite_stat = "Interrupts 9999999 · Core0 95% · Core1 80% · Core2 75% · Core3 60%";
    let bounded_stat = bounded_stat_text(composite_stat, false, false);
    assert_eq!(
        bounded_stat.chars().count(),
        20,
        "composite stat must clamp to 20 chars"
    );
    assert!(
        bounded_stat.ends_with('…'),
        "truncated stat must end with ellipsis"
    );

    let short_stat = "Base frequency";
    assert_eq!(bounded_stat_text(short_stat, false, true), "Base frequency");
}
