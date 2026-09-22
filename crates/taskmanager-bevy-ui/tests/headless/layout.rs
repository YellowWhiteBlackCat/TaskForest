//! Behavior tests for the shared Performance layout breakpoint.

use crate::widgets::layout::{
    COMPACT_BREAKPOINT_PX, CPU_CORE_GRID_MIN_WINDOW_HEIGHT_PX, PerformanceLayoutMode,
    cpu_core_grid_visible, performance_layout_mode,
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
