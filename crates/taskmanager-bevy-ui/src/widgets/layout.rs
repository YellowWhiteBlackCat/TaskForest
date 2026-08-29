//! Shared responsive layout contracts for the Bevy frontend.
//!
//! These values describe responsibilities, not page-local coordinates. Page
//! scenes consume the mode and bounds here, then express their hierarchy with
//! `bsn!`; no layout system is allowed to fork a second set of breakpoints.

/// Below this content width the Performance surface becomes the compact
/// device-detail layout: icon rail + device pills + one main graph.
pub(crate) const COMPACT_BREAKPOINT_PX: f32 = 860.0;

/// Wide-layout column widths. The main column owns the remaining width and
/// is always allowed to shrink to zero before a child is permitted to
/// overflow.
pub(crate) const WIDE_DEVICE_SIDEBAR_WIDTH_PX: f32 = 256.0;
pub(crate) const WIDE_STATS_WIDTH_PX: f32 = 280.0;

/// The minimum useful width of the CPU graph column. A page must switch to
/// compact mode before this bound is violated; it must not squeeze labels or
/// invent a horizontal overflow path.
pub(crate) const MAIN_GRAPH_MIN_WIDTH_PX: f32 = 360.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PerformanceLayoutMode {
    #[default]
    Wide,
    Compact,
}

#[must_use]
pub(crate) const fn performance_layout_mode(content_width: f32) -> PerformanceLayoutMode {
    if content_width < COMPACT_BREAKPOINT_PX {
        PerformanceLayoutMode::Compact
    } else {
        PerformanceLayoutMode::Wide
    }
}

#[cfg(test)]
#[path = "../../tests/headless/layout.rs"]
mod tests;
