//! Shared responsive layout contracts for the Bevy frontend.
//!
//! These values describe responsibilities, not page-local coordinates. Page
//! scenes consume the mode and bounds here, then express their hierarchy with
//! `bsn!`; no layout system is allowed to fork a second set of breakpoints.

/// Below this content width the Performance surface becomes the compact
/// device-detail layout: icon rail + device pills + one main graph.
pub(crate) const COMPACT_BREAKPOINT_PX: f32 = 860.0;

/// Wide-layout statistics rail width. It matches the reference surface's
/// pinned ceiling, so both frontends agree on the detail column.
pub(crate) const WIDE_STATS_WIDTH_PX: f32 = 280.0;

/// Device-sidebar budget bounds. The preferred width is NOT a frontend
/// constant: it is the persisted `taskforest-core` config value (one fact,
/// one authority), and these bounds only keep the main column and the
/// statistics rail above their own floors.
pub(crate) const PERFORMANCE_SIDEBAR_MIN_WIDTH_PX: f32 = 220.0;
pub(crate) const PERFORMANCE_SIDEBAR_MAX_WIDTH_PX: f32 = 460.0;
/// Smallest statistics rail that still reads, and the gap between slots.
pub(crate) const PERFORMANCE_STATS_MIN_WIDTH_PX: f32 = 236.0;
pub(crate) const PERFORMANCE_SLOT_GAP_PX: f32 = 12.0;

/// Resolve the device-sidebar width from the shared budget law.
///
/// `preferred` is the persisted config width; `workspace_width` is the page
/// content the root budget hands down. The sidebar may claim surplus only
/// after the main column and the statistics rail keep their floors, so a wide
/// window never lets the sidebar starve the graph and a narrow window clamps
/// the preference instead of clipping a child.
#[must_use]
pub(crate) fn performance_sidebar_width(preferred: f32, workspace_width: f32) -> f32 {
    let capacity = workspace_width
        - PERFORMANCE_STATS_MIN_WIDTH_PX
        - MAIN_GRAPH_MIN_WIDTH_PX
        - PERFORMANCE_SLOT_GAP_PX;
    let max_width = capacity.clamp(
        PERFORMANCE_SIDEBAR_MIN_WIDTH_PX,
        PERFORMANCE_SIDEBAR_MAX_WIDTH_PX,
    );
    let wanted = if preferred.is_finite() {
        preferred
    } else {
        PERFORMANCE_SIDEBAR_MIN_WIDTH_PX
    };
    wanted.clamp(PERFORMANCE_SIDEBAR_MIN_WIDTH_PX, max_width)
}

/// The minimum useful width of the CPU graph column. A page must switch to
/// compact mode before this bound is violated; it must not squeeze labels or
/// invent a horizontal overflow path.
pub(crate) const MAIN_GRAPH_MIN_WIDTH_PX: f32 = 360.0;
/// Minimum window height for the optional multi-row CPU core group. Below
/// this threshold the whole optional group is hidden, so a short capture can
/// never paint a partial final row at the window edge.
pub(crate) const CPU_CORE_GRID_MIN_WINDOW_HEIGHT_PX: f32 = 920.0;
pub(crate) const MEMORY_LEGEND_MIN_WINDOW_HEIGHT_PX: f32 = 750.0;

/// The height of one performance chart slot, in rem.
///
/// The device curves and the GPU metric curve share one slot budget, so the
/// measured canvas and the card that hosts it can never disagree. The slot
/// rides the rem axis: a UI-size choice re-scales the chart together with the
/// surface around it (the Standard step still measures 64 logical px).
pub(crate) const PERFORMANCE_CHART_SLOT_HEIGHT_REM: f32 = 4.0;

/// The width of one centered modal panel, in rem.
///
/// The confirmation gate and the CPU affinity dialog are the product's two
/// centered dialogs, so they share one panel width: a user who has learned
/// the shape of one has learned the other's. The width rides the rem axis
/// with the surface around it (the Standard step still measures 460 logical
/// px).
pub(crate) const MODAL_PANEL_WIDTH_REM: f32 = 28.75;

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

#[must_use]
pub(crate) const fn memory_legend_visible(window_height: f32) -> bool {
    window_height >= MEMORY_LEGEND_MIN_WINDOW_HEIGHT_PX
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
