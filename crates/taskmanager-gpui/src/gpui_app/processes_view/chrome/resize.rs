//! Processes-column drag-resize adapter. The reusable GPUI handle, typed drag
//! payload, and reference interaction live in `taskmanager-ui`; this page
//! module only maps the generic column index onto `SortCol` and owns the
//! process-table session anchor.

use gpui::{
    App, Div, DragMoveEvent, Entity, InteractiveElement, ParentElement, Pixels, Stateful, Styled,
    div,
};
use std::collections::HashMap;

use crate::gpui_app::processes_view::rows::{column_index, default_width};
use crate::gpui_app::root::RootView;
use taskmanager_shell::SortCol;
use taskmanager_theme::Theme;
use taskmanager_ui::data::table::{
    ColumnResizeDrag, ColumnResizeHandleProps, column_resize_handle, width_from_drag,
};

/// Resolve a column's live header width (user override else default) — the
/// header counterpart of `cells::live_width`, keeping header + body pixel-aligned
/// after a drag.
pub(super) fn header_col_width(widths: &HashMap<SortCol, Pixels>, col: SortCol) -> Pixels {
    widths
        .get(&col)
        .copied()
        .unwrap_or_else(|| default_width(col))
}

/// Mount the reference GPUI resize handle for one process-table column. The
/// page keeps the `SortCol` identity and its anchor state; the shared helper
/// owns the hitbox, typed drag construction, propagation stop, and outside
/// release hook.
fn proc_resize_handle(
    theme: &Theme,
    col: SortCol,
    start_width: Pixels,
    entity: &Entity<RootView>,
) -> Stateful<Div> {
    let col_index = column_index(col);
    let owner = entity.entity_id();
    let ent_move = entity.clone();
    let ent_start = entity.clone();
    let ent_finish = entity.clone();

    column_resize_handle(ColumnResizeHandleProps {
        id: ("proc-resize-h", col_index).into(),
        owner,
        column: col_index,
        start_width,
        palette: theme.palette(),
        on_drag_start: move |cx: &mut App| {
            // A stale anchor can survive a release outside the window; every
            // newly promoted drag starts a fresh coordinate session.
            ent_start.update(cx, |view, _cx| {
                view.processes_state.resize_anchor_x = None;
            });
        },
        on_drag_move: move |event: &DragMoveEvent<ColumnResizeDrag>, cx: &mut App| {
            let payload = *event.drag(cx);
            if payload.owner != owner || payload.column != col_index {
                return;
            }
            ent_move.update(cx, |view, cx| {
                // The shared reference helper anchors the first motion and
                // measures every later motion from it.
                let new_width = width_from_drag(
                    payload.start_width,
                    &mut view.processes_state.resize_anchor_x,
                    event.event.position.x,
                );
                view.resize_proc_col(col, new_width, cx);
            });
        },
        on_drag_finish: move |cx: &mut App| {
            ent_finish.update(cx, |view, cx| {
                view.processes_state.resize_anchor_x = None;
                cx.notify();
            });
        },
    })
    .debug_selector(move || format!("tm-proc-resize-h:{col_index}"))
}

/// Wrap a sized header cell with its right-edge resize handle. The caller
/// applies `.w(width)` (or `.flex_grow()` for Name) to the `sort_cell` before
/// passing it in. Mirrors the shared table crate's `render_th` cell+handle
/// composition; called only for resizable columns — `Name` (the flexible growable
/// identity column) stays a direct flex child of the header row and is never
/// passed here.
pub(super) fn mount_resize_handle(
    cell: Stateful<Div>,
    col: SortCol,
    start_width: Pixels,
    theme: &Theme,
    entity: &Entity<RootView>,
) -> Div {
    div()
        .flex()
        .flex_row()
        .h_full()
        .child(cell)
        .child(proc_resize_handle(theme, col, start_width, entity))
}
