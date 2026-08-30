//! Click-to-select hit-testing support (input line): the pure projection from
//! a terminal cell to the table row the renderer painted there. Test-only by
//! contract — pointer input in the shipped binary resolves through the
//! renderer's own frame plan; this module pins that a click and the keyboard
//! address the same visual row projection. The alignment is locked
//! behaviorally by `runtime::seam` tests that render real frames and check the
//! highlight row against this projection.
//!
//! Modeled surface (honest boundaries, each locked by a test):
//! - Bare left click on a DATA row of Applications, Services, Startup, and
//!   Users selects through the same page-specific projection as the keyboard.
//! - Not modeled: the App-history page (the shell clamps its keyboard cursor
//!   there, so pointer selection would diverge from the keyboard), clicks on
//!   headers, borders, and anything outside the panel, and every click while
//!   any modal or overlay owns the screen.

use ratatui::layout::Rect;

use crate::TuiApp;
use crate::ui::TuiFramePlan;

/// Project the current page's table panel inside `frame`. `None` on pages
/// without a keyboard-addressable table. The returned plan is the same pure
/// projection used by the renderer wrapper.
pub(crate) fn table_panel_projection(
    app: &TuiApp,
    frame: Rect,
) -> Option<crate::ui::frame_plan::TablePanelProjection> {
    TuiFramePlan::build(app, frame).table_panel()
}

/// Map a bare left click at absolute cell (`column`, `row`) to the global
/// row index in the renderer's current projection. `None` when the click is
/// not on a visible data row (header, border, margin, outside the panel, or
/// a page/shape without pointer-addressable rows).
pub(crate) fn row_at(app: &TuiApp, frame: Rect, column: u16, row: u16) -> Option<usize> {
    let plan = TuiFramePlan::build(app, frame);
    row_at_plan(&plan, column, row)
}

/// Map a click through a committed immutable frame plan. The runtime uses this
/// entry so a click is interpreted against the frame the user actually saw,
/// even if earlier events in the same terminal burst already changed app state.
pub(crate) fn row_at_plan(plan: &TuiFramePlan, column: u16, row: u16) -> Option<usize> {
    plan.hit_target(column, row)
        .and_then(|target| match target {
            crate::ui::frame_plan::TuiHitTarget::TableRow { index, .. } => Some(index),
            // Overlay cells and overlay control rows are not table rows.
            crate::ui::frame_plan::TuiHitTarget::Overlay { .. }
            | crate::ui::frame_plan::TuiHitTarget::OverlayControl { .. } => None,
        })
}
