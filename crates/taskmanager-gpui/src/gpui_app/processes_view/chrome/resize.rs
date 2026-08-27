//! Processes-column drag-resize handle: the typed drag payload, the 2px
//! right-edge handle, and the cell+handle mount helper. Ported from the shared
//! `taskmanager-ui` table crate's `render_resize_handle` so the bespoke
//! processes table reuses the same mechanism instead of hardcoding px widths.

use gpui::{
    App, AppContext, Context, Div, DragMoveEvent, Empty, Entity, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, Render, Stateful, StatefulInteractiveElement, Styled,
    Window, div, px,
};
use std::collections::HashMap;

use crate::gpui_app::processes_view::rows::{SortCol, column_index, default_width};
use crate::gpui_app::root::RootView;
use crate::gpui_app::theme::Theme;

/// Typed drag payload for a processes-column resize. Carries the column being
/// dragged plus the width captured at drag start, so each `on_drag_move` computes
/// the new width as a stable delta from the drag origin (`start_width +
/// (cursor.x - anchor_x)`) rather than the moving handle bounds. Implements
/// `gpui::Render` (returning `Empty`) because `on_drag` requires its drag value
/// to be a view the framework can instantiate — mirrors
/// `taskmanager_ui::data::table::ResizeColumn`.
#[derive(Clone, Copy)]
pub(crate) struct ProcResizeCol {
    col: SortCol,
    start_width: Pixels,
}

impl Render for ProcResizeCol {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// 2px-wide right-edge drag handle (matches the shared crate's `HANDLE_SIZE`).
const PROC_RESIZE_HANDLE_SIZE: Pixels = px(2.0);

/// Resolve a column's live header width (user override else default) — the
/// header counterpart of `cells::live_width`, keeping header + body pixel-aligned
/// after a drag.
pub(super) fn header_col_width(widths: &HashMap<SortCol, Pixels>, col: SortCol) -> Pixels {
    widths
        .get(&col)
        .copied()
        .unwrap_or_else(|| default_width(col))
}

/// The 2px right-edge drag handle for one resizable column header, adapted from
/// `taskmanager_ui::data::table::render::render_resize_handle`. Sits on the
/// column's right edge via a negative left margin, shows `cursor_col_resize`,
/// and on drag updates the column's stored width through
/// `RootView::resize_proc_col`. The `payload.col` guard ensures only the
/// dragged column's own handle drives the change (every registered handle's
/// `on_drag_move` fires on capture phase for the active `ProcResizeCol` drag —
/// see gpui `Div::on_drag_move`); drag events are window-scoped, so no extra
/// multi-window guard is needed.
fn proc_resize_handle(
    theme: &Theme,
    col: SortCol,
    start_width: Pixels,
    entity: &Entity<RootView>,
) -> Stateful<Div> {
    let ent_move = entity.clone();
    let ent_drag = entity.clone();
    let ent_up = entity.clone();
    let border = theme.palette().border;
    div()
        .flex()
        .flex_row()
        .id(("proc-resize-h", column_index(col)))
        .occlude()
        .cursor_col_resize()
        .h_full()
        .w(PROC_RESIZE_HANDLE_SIZE)
        .ml(-PROC_RESIZE_HANDLE_SIZE)
        .justify_end()
        .items_center()
        .child(div().h_full().justify_center().bg(border).w(px(1.0)))
        .on_drag_move(
            move |e: &DragMoveEvent<ProcResizeCol>, _win, cx: &mut App| {
                let payload = *e.drag(cx);
                if payload.col != col {
                    return;
                }
                ent_move.update(cx, |view, cx| {
                    let anchor_x = match view.processes_state.resize_anchor_x {
                        Some(x) => x,
                        None => {
                            // First move of this drag: capture the start cursor x so
                            // every later move is a stable delta from drag start.
                            view.processes_state.resize_anchor_x = Some(e.event.position.x);
                            e.event.position.x
                        }
                    };
                    let new_width = payload.start_width + (e.event.position.x - anchor_x);
                    view.resize_proc_col(payload.col, new_width, cx);
                });
            },
        )
        .on_drag(
            ProcResizeCol { col, start_width },
            move |_value, _offset, _win, cx: &mut App| {
                cx.stop_propagation();
                // Reset any stale anchor so the first `on_drag_move` of THIS
                // drag re-captures (covers a prior drag whose mouse-up landed
                // outside the window and never cleared it).
                ent_drag.update(cx, |view, _cx| {
                    view.processes_state.resize_anchor_x = None;
                });
                cx.new(|_| ProcResizeCol { col, start_width })
            },
        )
        .on_mouse_up_out(MouseButton::Left, move |_ev, _win, cx: &mut App| {
            ent_up.update(cx, |view, cx| {
                view.processes_state.resize_anchor_x = None;
                cx.notify();
            });
        })
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
