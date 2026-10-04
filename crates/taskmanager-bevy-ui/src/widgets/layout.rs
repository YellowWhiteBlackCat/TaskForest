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
pub(crate) const WIDE_STATS_WIDTH_PX: f32 = 320.0;

/// The minimum useful width of the CPU graph column. A page must switch to
/// compact mode before this bound is violated; it must not squeeze labels or
/// invent a horizontal overflow path.
pub(crate) const MAIN_GRAPH_MIN_WIDTH_PX: f32 = 360.0;
/// Minimum window height for the optional multi-row CPU core group. Below
/// this threshold the whole optional group is hidden, so a short capture can
/// never paint a partial final row at the window edge.
pub(crate) const CPU_CORE_GRID_MIN_WINDOW_HEIGHT_PX: f32 = 920.0;

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

#[must_use]
pub(crate) const fn cpu_core_grid_visible(window_height: f32) -> bool {
    window_height >= CPU_CORE_GRID_MIN_WINDOW_HEIGHT_PX
}

#[cfg(test)]
#[path = "../../tests/headless/layout.rs"]
mod tests;

/// Fixed rows include title, two caption lines, coverage, padding, gaps and bottom safety.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SystemDashboardBudget {
    pub(crate) count: usize,
    pub(crate) chart_height: f32,
    pub(crate) card_height: f32,
}
impl SystemDashboardBudget {
    pub(crate) fn resolve(width: f32, height: f32) -> Self {
        let columns = if width >= 528.0 { 2 } else { 1 };
        let available = (height - 8.0).max(0.0);
        if available < 92.0 {
            return Self {
                count: 0,
                chart_height: 0.0,
                card_height: 0.0,
            };
        }
        let rows = (available / 132.0).floor().clamp(1.0, 4.0) as usize;
        let count = (rows * columns).min(4);
        let rows = count.div_ceil(columns);
        let card_height =
            ((available - (rows.saturating_sub(1) as f32 * 8.0)) / rows as f32).clamp(92.0, 200.0);
        let chart_height = if card_height >= 124.0 {
            card_height - 92.0
        } else {
            0.0
        };
        Self {
            count,
            chart_height,
            card_height,
        }
    }
}
