//! Behavior tests for the shared Performance layout breakpoint.

use crate::widgets::layout::{
    COMPACT_BREAKPOINT_PX, PERFORMANCE_SIDEBAR_MAX_WIDTH_PX, PERFORMANCE_SIDEBAR_MIN_WIDTH_PX,
    PerformanceLayoutMode, performance_layout_mode, performance_sidebar_width,
};
use crate::widgets::layout_budget::PerformanceFrameBudget;
use taskmanager_ui_contract::presentation::vocabulary::VerticalSpace;

#[test]
fn performance_layout_switches_before_the_graph_becomes_unusable() {
    assert_eq!(
        performance_layout_mode(COMPACT_BREAKPOINT_PX - 1.0),
        PerformanceLayoutMode::Compact
    );
    assert_eq!(
        performance_layout_mode(COMPACT_BREAKPOINT_PX),
        PerformanceLayoutMode::Wide
    );
}

#[test]
fn vertical_groups_admit_from_the_shared_vertical_capacity_band() {
    let constrained = PerformanceFrameBudget::for_page_slot(1180.0, 480.0);
    let standard = PerformanceFrameBudget::for_page_slot(1180.0, 780.0);
    let generous = PerformanceFrameBudget::for_page_slot(1180.0, 1000.0);
    assert_eq!(constrained.page.vertical_space, VerticalSpace::Constrained);
    assert_eq!(standard.page.vertical_space, VerticalSpace::Standard);
    assert_eq!(generous.page.vertical_space, VerticalSpace::Generous);
    assert!(!constrained.admits_full_inventory());
    assert!(standard.admits_full_inventory());
    assert!(generous.admits_full_inventory());
}

#[test]
fn sidebar_width_spends_the_shared_preference_inside_the_budget() {
    // The persisted preference (the shared default 260) is honoured on a wide
    // page. A page too narrow to leave the graph and the statistics rail their
    // floors clamps the preference instead of clipping a child; an enormous
    // page stops at the ceiling; a non-finite preference falls back to the
    // floor rather than painting a zero-width rail.
    assert_eq!(performance_sidebar_width(260.0, 1252.0), 260.0);
    assert_eq!(performance_sidebar_width(400.0, 900.0), 292.0);
    assert_eq!(
        performance_sidebar_width(5000.0, 1252.0),
        PERFORMANCE_SIDEBAR_MAX_WIDTH_PX
    );
    assert_eq!(
        performance_sidebar_width(10.0, 1252.0),
        PERFORMANCE_SIDEBAR_MIN_WIDTH_PX
    );
    assert_eq!(
        performance_sidebar_width(f32::NAN, 1252.0),
        PERFORMANCE_SIDEBAR_MIN_WIDTH_PX
    );
}
