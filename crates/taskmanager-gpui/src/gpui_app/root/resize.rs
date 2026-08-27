//! Processes-column resize: the `[10px, 1200px]` width clamp + `<1px`-jitter
//! rule shared between the live drag handle (`processes_view::chrome::resize`)
//! and the config-load clamp (`root::persistence`), plus the `RootView` methods
//! that read / apply a column's stored width.

use gpui::{Context, Pixels, px};

use crate::gpui_app::processes_view;
use crate::gpui_app::processes_view::rows::default_width;

use super::RootView;

impl RootView {
    /// Resolve the live render width of a processes-table column: the user's
    /// resized override if present, else `SortCol::default_width`. Both the
    /// header (`sort_header_row`) and the body (`append_body_cells`) read this so
    /// header + body stay pixel-aligned after a drag. `Name` is non-resizable and
    /// sized via `flex_grow`; this still returns its floor for any caller that
    /// wants a numeric fallback.
    pub fn proc_col_width(&self, col: processes_view::SortCol) -> Pixels {
        self.processes_state
            .col_widths
            .get(&col)
            .copied()
            .unwrap_or_else(|| default_width(col))
    }

    /// Apply a candidate width to a processes-table column through the same clamp
    /// the shared table crate's `resize_cols` enforces (`[10px, 1200px]`;
    /// sub-floor and <1px jitter dropped). Used by the per-handle `on_drag_move`
    /// handler; the <1px jitter rule prevents redundant writes (and re-renders)
    /// for sub-pixel mouse noise. `Name` is non-resizable — the handle is never
    /// mounted on it, so this is never called with `Name` from the drag path;
    /// calling it directly is a no-op for `Name` (the clamp still runs but the
    /// result is never applied to a fixed `.w(..)` — Name keeps its `flex_grow`
    /// sizing).
    pub fn resize_proc_col(
        &mut self,
        col: processes_view::SortCol,
        size: Pixels,
        cx: &mut Context<Self>,
    ) {
        let old = self.proc_col_width(col);
        if let Some(clamped) = clamp_proc_col_width(size, old) {
            self.processes_state.col_widths.insert(col, clamped);
            cx.notify();
        }
    }

    /// Apply a candidate sidebar width through the sidebar clamp (`[200, 460]`;
    /// sub-floor and <1px jitter dropped). Used by the sidebar edge-handle's
    /// `on_drag_move`; the <1px jitter rule prevents redundant re-renders for
    /// sub-pixel mouse noise (same discipline as `resize_proc_col`).
    pub fn resize_sidebar(&mut self, size: Pixels, cx: &mut Context<Self>) {
        let mut sidebar = self.presentation.sidebar().clone();
        if let Some(clamped) = clamp_sidebar_width(size, sidebar.width) {
            sidebar.width = clamped;
            self.presentation.set_sidebar(sidebar);
            cx.notify();
        }
    }
}

/// Floor for a processes-column width, in device pixels (matches the shared
/// table crate's `resize_cols` floor at `crates/taskmanager-ui/src/data/table/
/// render.rs`). Shared between the drag clamp ([`clamp_proc_col_width`]) and the
/// config-load clamp ([`crate::gpui_app::root::persistence`]) so a hand-edited
/// config file cannot smuggle in an unusable column sliver.
pub(crate) const PROC_COL_MIN_WIDTH: f32 = 10.0;
/// Floor for the devices-sidebar width (device pixels): the narrowest the user
/// can drag it before the device rows lose their icon + label. Shared between
/// the drag clamp and the config-load clamp.
pub(crate) const SIDEBAR_MIN_WIDTH: f32 = 200.0;
/// Ceiling for the devices-sidebar width: the widest before it starves the
/// content pane. Shared between the drag clamp and the config-load clamp.
pub(crate) const SIDEBAR_MAX_WIDTH: f32 = 460.0;
/// Ceiling for a processes-column width, in device pixels (matches the shared
/// table crate's `resize_cols` ceiling). Shared between the drag clamp and the
/// config-load clamp so an absurd hand-edited width clamps down rather than
/// blowing out the table layout.
pub(crate) const PROC_COL_MAX_WIDTH: f32 = 1200.0;

/// Clamp a candidate processes-column width with the exact bounds + jitter rule
/// the shared table crate's `TableState::resize_cols` uses
/// (`crates/taskmanager-ui/src/data/table/render.rs`): floor to `10px`, ceiling
/// to `1200px`, and drop any change smaller than `1px` (sub-pixel mouse noise).
/// Returns `None` when the candidate should be ignored (below floor or jitter);
/// otherwise `Some(clamped)` with the ceiling applied. Pure / host-independent so
/// the unit test can pin the three boundary classes.
pub(crate) fn clamp_proc_col_width(new: Pixels, old: Pixels) -> Option<Pixels> {
    const MIN_COL_WIDTH: Pixels = px(PROC_COL_MIN_WIDTH);
    const MAX_COL_WIDTH: Pixels = px(PROC_COL_MAX_WIDTH);
    let new = new.floor();
    if new < MIN_COL_WIDTH {
        return None;
    }
    let changed = new - old;
    if changed > px(-1.0) && changed < px(1.0) {
        return None;
    }
    Some(new.min(MAX_COL_WIDTH))
}

/// Clamp a candidate sidebar width: floor `200px`, ceiling `460px`, drop any
/// change smaller than `1px` (sub-pixel mouse noise). Returns `None` when the
/// candidate should be ignored (below floor or jitter); otherwise
/// `Some(clamped)` with the ceiling applied. Mirrors [`clamp_proc_col_width`];
/// pure / host-independent so the unit test pins the boundary classes.
pub(crate) fn clamp_sidebar_width(new: Pixels, old: Pixels) -> Option<Pixels> {
    const MIN_WIDTH: Pixels = px(SIDEBAR_MIN_WIDTH);
    const MAX_WIDTH: Pixels = px(SIDEBAR_MAX_WIDTH);
    let new = new.floor();
    if new < MIN_WIDTH {
        return None;
    }
    let changed = new - old;
    if changed > px(-1.0) && changed < px(1.0) {
        return None;
    }
    Some(new.min(MAX_WIDTH))
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_resize_tests.rs"]
mod tests;
