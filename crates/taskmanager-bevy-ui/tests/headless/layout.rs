//! Behavior tests for the shared Performance layout breakpoint.

use crate::widgets::layout::{
    COMPACT_BREAKPOINT_PX, PerformanceLayoutMode, performance_layout_mode,
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
