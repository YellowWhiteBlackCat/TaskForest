//! Column-width drag semantics for the GPUI table family (CORE-08).
//!
//! This module is the reference surface for `ColumnDragResize`: it owns the
//! typed GPUI drag payload, the standard resize gutter, and the reference
//! width transition. Table-specific identity, persistence, and downstream
//! effects stay with the caller. Keeping those concerns out of this module
//! lets bespoke tables (such as the Applications process table) reuse the
//! interaction without making the reference layer depend on a page model.

use gpui::{
    AnyElement, App, AppContext, Context, Div, DragMoveEvent, ElementId, EntityId,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, Render, Stateful,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use taskmanager_theme::Palette;

/// The reference resize gutter width. The visual surface may choose a larger
/// acquisition area in another frontend; GPUI's reference table uses this
/// two-pixel gutter and paints a one-pixel divider inside it.
pub const COLUMN_RESIZE_HANDLE_SIZE: Pixels = px(2.0);

/// Reference lower bound for a resizable column. Frontend-specific geometry
/// may choose a stricter floor when its text/layout system needs one; this is
/// the GPUI table policy and is kept in one place for all GPUI consumers.
pub const COLUMN_RESIZE_MIN_WIDTH: f32 = 10.0;

/// Reference upper bound for a resizable column.
pub const COLUMN_RESIZE_MAX_WIDTH: f32 = 1200.0;

/// Typed payload carried by a column-width drag.
///
/// `owner` prevents capture-phase handlers belonging to other tables from
/// reacting to the same active drag. `start_width` makes the transition
/// independent of a stale or moving layout bounds snapshot; the caller owns
/// the first pointer position used as the session anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnResizeDrag {
    pub owner: EntityId,
    pub column: usize,
    pub start_width: Pixels,
}

impl Render for ColumnResizeDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // GPUI requires a Render value for an active drag even when the
        // interaction is a resize rather than a visible drag-and-drop item.
        gpui::Empty
    }
}

/// Apply the GPUI reference width policy: floor to whole pixels, reject the
/// unusable lower range and sub-pixel jitter, and saturate at the upper bound.
/// The function is pure so page adapters and behavior tests can share the
/// exact transition without sharing page state.
#[must_use]
pub fn clamp_column_width(new: Pixels, old: Pixels) -> Option<Pixels> {
    let new = new.floor();
    if new < px(COLUMN_RESIZE_MIN_WIDTH) {
        return None;
    }
    let changed = new - old;
    if changed > px(-1.0) && changed < px(1.0) {
        return None;
    }
    Some(new.min(px(COLUMN_RESIZE_MAX_WIDTH)))
}

/// Resolve the next width in a drag session. The first pointer sample becomes
/// the session anchor; subsequent samples are measured from that fixed point,
/// so a moving/stale layout bound cannot introduce cumulative drift.
pub fn width_from_drag(
    start_width: Pixels,
    anchor_x: &mut Option<Pixels>,
    cursor_x: Pixels,
) -> Pixels {
    let anchor = *anchor_x.get_or_insert(cursor_x);
    start_width + (cursor_x - anchor)
}

/// Build the reference GPUI resize gutter.
///
/// The callbacks deliberately receive only the drag event and `App`: the
/// caller remains the owner of its table/page state, while this helper owns
/// the common hitbox, typed drag construction, propagation stop, and
/// outside-release hook. The start callback is run when GPUI promotes the
/// press to a drag, which is the correct point to clear a stale session
/// anchor.
pub struct ColumnResizeHandleProps<FStart, FMove, FFinish> {
    pub id: ElementId,
    pub owner: EntityId,
    pub column: usize,
    pub start_width: Pixels,
    pub palette: Palette,
    pub on_drag_start: FStart,
    pub on_drag_move: FMove,
    pub on_drag_finish: FFinish,
}

pub fn column_resize_handle<FStart, FMove, FFinish>(
    props: ColumnResizeHandleProps<FStart, FMove, FFinish>,
) -> Stateful<Div>
where
    FStart: Fn(&mut App) + 'static,
    FMove: Fn(&DragMoveEvent<ColumnResizeDrag>, &mut App) + 'static,
    FFinish: Fn(&mut App) + 'static,
{
    let ColumnResizeHandleProps {
        id,
        owner,
        column,
        start_width,
        palette,
        on_drag_start,
        on_drag_move,
        on_drag_finish,
    } = props;
    div()
        .flex()
        .flex_row()
        .id(id)
        .occlude()
        .cursor_col_resize()
        .h_full()
        .w(COLUMN_RESIZE_HANDLE_SIZE)
        .ml(-COLUMN_RESIZE_HANDLE_SIZE)
        .justify_end()
        .items_center()
        .child(
            div()
                .h_full()
                .justify_center()
                .bg(crate::theme_binding::fill(palette.border))
                .w(px(1.0)),
        )
        .on_drag_move(
            move |event: &DragMoveEvent<ColumnResizeDrag>, _window, cx| {
                on_drag_move(event, cx);
            },
        )
        .on_drag(
            ColumnResizeDrag {
                owner,
                column,
                start_width,
            },
            move |drag, _offset, _window, cx| {
                cx.stop_propagation();
                on_drag_start(cx);
                cx.new(|_| *drag)
            },
        )
        .on_mouse_up_out(MouseButton::Left, move |_event, _window, cx| {
            on_drag_finish(cx)
        })
}

impl<D: super::TableDelegate> super::TableState<D> {
    /// Apply the reference width transition to one live column group.
    pub(crate) fn resize_cols(&mut self, ix: usize, size: Pixels, cx: &mut Context<Self>) {
        if !self.col_resizable {
            return;
        }
        let Some(col_group) = self.col_groups.get_mut(ix) else {
            return;
        };
        if !col_group.is_resizable() {
            return;
        }
        let old_width = col_group.width;
        if let Some(width) = clamp_column_width(size, old_width) {
            col_group.width = width;
            cx.notify();
        }
    }

    /// Scroll horizontally while resizing near the table edges.
    pub(crate) fn scroll_table_by_col_resizing(
        &mut self,
        mouse_position: Point<Pixels>,
        col_group: &super::ColGroup,
    ) {
        if mouse_position.x > self.bounds.right() {
            return;
        }
        let mut offset = self.horizontal_scroll_handle.offset();
        let col_bounds = col_group.bounds;
        if mouse_position.x < self.bounds.left()
            && col_bounds.right() < self.bounds.left() + px(20.0)
        {
            offset.x += px(1.0);
        } else if mouse_position.x > self.bounds.right()
            && col_bounds.right() > self.bounds.right() - px(20.0)
        {
            offset.x -= px(1.0);
        }
        self.horizontal_scroll_handle.set_offset(offset);
    }

    /// The resize gutter for one resizable column header. The generic table
    /// owns the table-state callbacks; the shared helper owns the GPUI hitbox,
    /// payload, and outside-release wiring.
    pub(crate) fn render_resize_handle(
        &self,
        ix: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(col_group) = self.col_groups.get(ix) else {
            return div().into_any_element();
        };
        if !self.col_resizable || !col_group.is_resizable() {
            return div().into_any_element();
        }

        let owner = cx.entity_id();
        let move_state = cx.entity();
        let start_state = move_state.clone();
        let finish_state = move_state.clone();
        let palette = self.palette;

        column_resize_handle(ColumnResizeHandleProps {
            id: ("resizable-handle", ix).into(),
            owner,
            column: ix,
            start_width: col_group.width,
            palette,
            on_drag_start: move |cx: &mut App| {
                start_state.update(cx, |table, _| {
                    table.resize_anchor_x = None;
                });
            },
            on_drag_move: move |event: &DragMoveEvent<ColumnResizeDrag>, cx: &mut App| {
                let drag = *event.drag(cx);
                if drag.owner != owner {
                    return;
                }
                let position = event.event.position;
                move_state.update(cx, |table, cx| {
                    let Some(col_group) = table.col_groups.get(drag.column).cloned() else {
                        return;
                    };
                    let new_width =
                        width_from_drag(drag.start_width, &mut table.resize_anchor_x, position.x);
                    table.resizing_col = Some(drag.column);
                    table.resize_cols(drag.column, new_width, cx);
                    table.scroll_table_by_col_resizing(position, &col_group);
                });
            },
            on_drag_finish: move |cx: &mut App| {
                finish_state.update(cx, |table, cx| table.finish_col_resize(cx));
            },
        })
        .into_any_element()
    }

    /// End a column resize: emit the new widths. Also called from the
    /// window-level `MouseUp` fallback when the pointer is released outside
    /// the window and the handle-local callback cannot observe it.
    pub(crate) fn finish_col_resize(&mut self, cx: &mut Context<Self>) {
        self.resize_anchor_x = None;
        if self.resizing_col.is_none() {
            return;
        }
        self.resizing_col = None;
        let new_widths = self.col_groups.iter().map(|group| group.width).collect();
        cx.emit(super::TableEvent::ColumnWidthsChanged(new_widths));
        cx.notify();
    }
}
