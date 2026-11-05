//! Behavior tests for the shared Performance layout breakpoint.

use crate::widgets::layout::{
    COMPACT_BREAKPOINT_PX, CPU_CORE_GRID_MIN_WINDOW_HEIGHT_PX, MEMORY_LEGEND_MIN_WINDOW_HEIGHT_PX,
    PERFORMANCE_SIDEBAR_MAX_WIDTH_PX, PERFORMANCE_SIDEBAR_MIN_WIDTH_PX, PerformanceLayoutMode,
    cpu_core_grid_visible, memory_legend_visible, performance_layout_mode,
    performance_sidebar_width,
};

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
fn optional_core_grid_is_whole_group_or_hidden_by_height() {
    assert!(!cpu_core_grid_visible(
        CPU_CORE_GRID_MIN_WINDOW_HEIGHT_PX - 1.0
    ));
    assert!(cpu_core_grid_visible(CPU_CORE_GRID_MIN_WINDOW_HEIGHT_PX));
}

#[test]
fn memory_legend_admits_the_full_group_only_above_its_floor() {
    // The reference surface's memory card drops the legend rows whole at the
    // 720p capture height; a half-painted legend is never an option.
    assert!(!memory_legend_visible(720.0));
    assert!(!memory_legend_visible(
        MEMORY_LEGEND_MIN_WINDOW_HEIGHT_PX - 1.0
    ));
    assert!(memory_legend_visible(MEMORY_LEGEND_MIN_WINDOW_HEIGHT_PX));
    assert!(memory_legend_visible(780.0));
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
