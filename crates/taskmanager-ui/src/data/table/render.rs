//! Rendering of `TableState`: resize/move header drag elements, resize
//! scroll helpers, cell/header/row render methods and the `Render` impl.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AppContext, Bounds, Context, Div, DragMoveEvent, Empty, EntityId,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, Refineable, Render,
    SharedString, SizeRefinement, Stateful, StatefulInteractiveElement, StyleRefinement, Styled,
    Window, canvas, div, px,
};

use crate::primitives::scrollbar::rail::ScrollbarRail;
use crate::primitives::scrollbar::{SCROLLBAR_HEIGHT, SCROLLBAR_WIDTH, Scrollbar, ScrollbarShow};
use crate::styled::{blend, hover_fill};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_theme::with_alpha;

use super::{
    ColGroup, ResizeColumn, SortState, TableDelegate, TableEvent, TableRowLayout, TableSelection,
    TableState,
};

impl Render for ResizeColumn {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// Drag payload for column moves (typed, absorbed from gc `DragColumn`).
#[derive(Clone)]
pub(crate) struct DragColumn {
    pub entity_id: EntityId,
    pub name: SharedString,
    pub width: Pixels,
    pub col_ix: usize,
}

impl Render for DragColumn {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::dark().palette();
        let mut el = div()
            .px(tokens::SPACE_8)
            .py(tokens::SPACE_4)
            .opacity(0.9)
            .rounded(palette.small_radius)
            .border_1()
            .border_color(palette.border)
            .bg(hover_fill(palette.surface))
            .child(self.name.clone());
        el.style().refine(&StyleRefinement {
            size: SizeRefinement {
                width: Some(self.width.into()),
                height: Some(px(28.0).into()),
            },
            ..StyleRefinement::default()
        });
        el
    }
}

impl<D: TableDelegate> TableState<D> {
    pub(crate) fn resize_cols(&mut self, ix: usize, size: Pixels, cx: &mut Context<Self>) {
        if !self.col_resizable {
            return;
        }
        const MIN_COL_WIDTH: Pixels = px(10.0);
        const MAX_COL_WIDTH: Pixels = px(1200.0);
        let Some(col_group) = self.col_groups.get_mut(ix) else {
            return;
        };
        if !col_group.is_resizable() {
            return;
        }
        let size = size.floor();
        if size < MIN_COL_WIDTH {
            return;
        }
        let old_width = col_group.width;
        let changed = size - old_width;
        if changed > px(-1.0) && changed < px(1.0) {
            return;
        }
        col_group.width = size.min(MAX_COL_WIDTH);
        cx.notify();
    }

    /// Scroll horizontally while resizing near the table edges.
    pub(crate) fn scroll_table_by_col_resizing(
        &mut self,
        mouse_position: Point<Pixels>,
        col_group: &ColGroup,
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

    pub(crate) fn render_cell(&self, col_ix: usize) -> Div {
        let Some(col_group) = self.col_groups.get(col_ix) else {
            return div();
        };
        div()
            .w(col_group.width)
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .justify_center()
            // Column gutter: the same inner padding the processes table's
            // cells carry (`cells.rs` / `sort_cell`), so header labels and
            // body text sit away from the column edges and the table border.
            // `.w()` sets the border box, so the resize handle and column
            // boundaries keep their geometry — the padding only insets the
            // content.
            .pl(tokens::SPACE_8)
            .pr(tokens::SPACE_8)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_align(col_group.column.text_align)
    }

    /// Column-selection highlight wrapper (column mode only).
    pub(crate) fn render_col_wrap(&self, col_ix: usize) -> Div {
        let el = div().h_full();
        let selectable = self.col_selectable
            && self
                .col_groups
                .get(col_ix)
                .map(|group| group.column.selectable)
                .unwrap_or(false);
        if selectable && self.selection == TableSelection::Column(col_ix) {
            el.bg(with_alpha(self.palette.accent, 0.15))
        } else {
            el
        }
    }

    /// One cell: selection wrap + sized cell + delegate td.
    pub(crate) fn render_row_cell(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        self.render_col_wrap(col_ix).child(
            self.render_cell(col_ix)
                .text_color(self.palette.fg)
                .font_weight(tokens::FONT_WEIGHT_BODY.into())
                .child(self.delegate.render_td(row_ix, col_ix, window, cx)),
        )
    }

    /// Render the header cell for `col_ix`. Uses `get()` and skips
    /// rendering when the group is gone — no panic after `refresh()`
    /// shrinks the column set (附录 A-2 fix).
    pub(crate) fn render_th(
        &mut self,
        col_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(col_group) = self.col_groups.get(col_ix).cloned() else {
            return div();
        };
        let entity_id = cx.entity_id();
        let movable = self.col_movable && col_group.column.movable;
        let name = col_group.column.name.clone();
        let palette = self.palette;

        // Delegate content first (mutable borrow ends at the statement).
        let mut content = div()
            .size_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(tokens::SPACE_4)
            .child(self.delegate.render_th(col_ix, window, cx));
        if let Some(sort_icon) = self.render_sort_icon(col_ix, &col_group, cx) {
            content = content.child(sort_icon);
        }

        let mut cell = self
            .render_cell(col_ix)
            .id(("col-header", col_ix))
            .text_color(palette.fg_muted)
            .font_weight(tokens::FONT_WEIGHT_HEADER.into())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.on_col_head_click(col_ix, cx);
            }))
            .child(content);

        if movable {
            cell = cell
                .on_drag(
                    DragColumn {
                        entity_id,
                        col_ix,
                        name,
                        width: col_group.width,
                    },
                    |drag, _, _, cx| {
                        cx.stop_propagation();
                        cx.new(|_| drag.clone())
                    },
                )
                .drag_over::<DragColumn>(move |mut style, _, _, _| {
                    style.border_color = Some(palette.accent.into());
                    style
                })
                .on_drop(cx.listener(move |table, drag: &DragColumn, _, cx| {
                    if drag.entity_id != cx.entity_id() {
                        return;
                    }
                    table.move_column(drag.col_ix, col_ix, cx);
                }));
        }

        let resize_handle = self.render_resize_handle(col_ix, window, cx);
        let bounds_canvas = {
            let view = cx.entity().clone();
            canvas(
                move |bounds, _, cx| {
                    view.update(cx, |r, _| {
                        if let Some(group) = r.col_groups.get_mut(col_ix) {
                            group.bounds = bounds;
                        }
                    })
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full()
        };

        div()
            .flex()
            .flex_row()
            .h_full()
            .relative()
            .child(cell)
            .child(resize_handle)
            .child(bounds_canvas)
    }

    /// The sort indicator for `col_ix` (click cycles the sort). Returns an
    /// owned element so no state borrow survives the call.
    pub(crate) fn render_sort_icon(
        &self,
        col_ix: usize,
        col_group: &ColGroup,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.sortable || col_group.column.sort.is_none() {
            return None;
        }
        let (icon_id, active) = match col_group.sort {
            SortState::Ascending => (taskmanager_ui_contract::IconId::NavigateUp, true),
            SortState::Descending => (taskmanager_ui_contract::IconId::NavigateDown, true),
            SortState::Unsorted => (taskmanager_ui_contract::IconId::NavigateUp, false),
        };
        let palette = self.palette;
        Some(
            div()
                .id(("icon-sort", col_ix))
                .p(tokens::SPACE_2)
                .rounded(palette.small_radius)
                .opacity(if active { 1.0 } else { 0.45 })
                .hover(|this| this.bg(hover_fill(palette.surface)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.perform_sort(col_ix, cx);
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(if active {
                            palette.accent
                        } else {
                            palette.fg_muted
                        })
                        .child(taskmanager_icons::icon(icon_id).size(px(12.0))),
                )
                .into_any_element(),
        )
    }

    /// The 2px drag handle on the right edge of a resizable column header.
    pub(crate) fn render_resize_handle(
        &self,
        ix: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const HANDLE_SIZE: Pixels = px(2.0);
        let resizable = self.col_resizable
            && self
                .col_groups
                .get(ix)
                .map(ColGroup::is_resizable)
                .unwrap_or(false);
        if !resizable {
            return div().into_any_element();
        }
        let palette = self.palette;

        div()
            .flex()
            .flex_row()
            .id(("resizable-handle", ix))
            .occlude()
            .cursor_col_resize()
            .h_full()
            .w(HANDLE_SIZE)
            .ml(-HANDLE_SIZE)
            .justify_end()
            .items_center()
            .child(
                div()
                    .h_full()
                    .justify_center()
                    .bg(palette.border)
                    .w(px(1.0)),
            )
            .on_drag_move(
                cx.listener(move |view, e: &DragMoveEvent<ResizeColumn>, _window, cx| {
                    let ResizeColumn((entity_id, drag_ix)) = e.drag(cx);
                    if cx.entity_id() != *entity_id {
                        return;
                    }
                    let Some(col_group) = view.col_groups.get(*drag_ix).cloned() else {
                        return;
                    };
                    view.resizing_col = Some(*drag_ix);
                    view.resize_cols(
                        *drag_ix,
                        e.event.position.x - HANDLE_SIZE - col_group.bounds.left(),
                        cx,
                    );
                    view.scroll_table_by_col_resizing(e.event.position, &col_group);
                }),
            )
            .on_drag(ResizeColumn((cx.entity_id(), ix)), |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| *drag)
            })
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| view.finish_col_resize(cx)),
            )
            .into_any_element()
    }

    /// End a column resize: emit the new widths. Also called from the
    /// window-level `MouseUp` fallback (附录 A-10 fix: releasing outside the
    /// window may not deliver `on_mouse_up_out`).
    pub(crate) fn finish_col_resize(&mut self, cx: &mut Context<Self>) {
        if self.resizing_col.is_none() {
            return;
        }
        self.resizing_col = None;
        let new_widths = self.col_groups.iter().map(|g| g.width).collect();
        cx.emit(TableEvent::ColumnWidthsChanged(new_widths));
        cx.notify();
    }

    /// Render the header row: the leading fixed columns outside the scroll
    /// container, the rest inside a track-scrolled strip (absorption
    /// §4.3-C). The fixed columns must be rendered first so the column
    /// virtual list can read `col_groups[].bounds` (absorption 4.6-9).
    pub(crate) fn render_table_header(
        &mut self,
        fixed_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let horizontal_scroll_handle = self.horizontal_scroll_handle.clone();
        let view = cx.entity().clone();
        let palette = self.palette;

        if fixed_count == 0 {
            self.fixed_head_cols_bounds = Bounds::default();
        }

        let mut header = self.delegate.render_header(window, cx);
        let style = header.style().clone();
        let mut header = header
            .flex()
            .flex_row()
            .w_full()
            .h(self.options.row_height)
            .flex_shrink_0()
            .border_b_1()
            .border_color(palette.border)
            .text_color(palette.fg_muted)
            .font_weight(tokens::FONT_WEIGHT_HEADER.into())
            .bg(blend(palette.surface, palette.fg, 0.035));
        header.style().refine(&style);

        if fixed_count > 0 {
            let fixed = {
                let view = view.clone();
                div()
                    .flex()
                    .flex_row()
                    .relative()
                    .h_full()
                    .children((0..fixed_count).map(|col_ix| self.render_th(col_ix, window, cx)))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w_0()
                            .flex_shrink_0()
                            .border_r_1()
                            .border_color(palette.border),
                    )
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                view.update(cx, |r, _| r.fixed_head_cols_bounds = bounds)
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
            };
            header = header.child(fixed);
        }

        header.child(
            div()
                .flex()
                .flex_row()
                .id("table-head")
                .size_full()
                .overflow_scroll()
                .relative()
                .track_scroll(&horizontal_scroll_handle)
                .bg(hover_fill(palette.surface))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .relative()
                        .children(
                            (fixed_count..self.col_groups.len())
                                .map(|col_ix| self.render_th(col_ix, window, cx)),
                        )
                        .child(self.delegate.render_last_empty_col(window, cx)),
                ),
        )
    }

    /// Render one row (real or fake stripe filler) from the frame-local layout
    /// projection. It borrows the shared width slice and has no heap or cache.
    pub(crate) fn render_table_row(
        &mut self,
        row_ix: usize,
        layout: TableRowLayout<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let TableRowLayout {
            rows_count,
            fixed_count,
            col_sizes,
            horizontal_visible_range,
            horizontal_offset,
            columns_count,
            is_filled,
        } = layout;
        let palette = self.palette;
        let row_height = self.options.row_height;
        let is_stripe_row = self.options.stripe && !row_ix.is_multiple_of(2);
        let right_content_width = col_sizes
            .iter()
            .copied()
            .fold(px(0.0), |sum, width| sum + width);

        if row_ix < rows_count {
            let is_selected = self.selection == TableSelection::Row(row_ix);
            let is_right_clicked = self.right_clicked_row == Some(row_ix);
            let is_last_row = row_ix + 1 == rows_count;
            let need_render_border = is_selected || !is_last_row || !is_filled;

            let mut tr = self.delegate.render_tr(row_ix, window, cx);
            let style = tr.style().clone();
            let mut tr = tr
                .flex()
                .flex_row()
                .w_full()
                .h(row_height)
                .text_color(palette.fg)
                .font_weight(tokens::FONT_WEIGHT_BODY.into())
                .when(need_render_border, |this| {
                    this.border_b_1().border_color(palette.border)
                })
                .when(is_stripe_row, |this| {
                    this.bg(blend(palette.surface, palette.fg, 0.04))
                })
                .hover(|this| {
                    if is_selected || is_right_clicked {
                        this
                    } else {
                        this.bg(hover_fill(palette.surface))
                    }
                });
            tr.style().refine(&style);

            // Leading fixed columns (real indices, 附录 A-1 fix).
            if fixed_count > 0 {
                tr = tr.child(
                    div()
                        .flex()
                        .flex_row()
                        .relative()
                        .h_full()
                        .children(
                            (0..fixed_count)
                                .map(|col_ix| self.render_row_cell(row_ix, col_ix, window, cx)),
                        )
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .right_0()
                                .bottom_0()
                                .w_0()
                                .flex_shrink_0()
                                .border_r_1()
                                .border_color(palette.border),
                        ),
                );
            }

            // The table-wide range is calculated once per frame.  Every
            // visible row paints the same clipped column band, so horizontal
            // movement no longer runs one variable-size VirtualList layout
            // and range scan per row.
            tr = tr.child(
                div().flex_1().h_full().overflow_hidden().relative().child(
                    div()
                        .flex()
                        .flex_row()
                        .relative()
                        .h_full()
                        .w(right_content_width)
                        .flex_shrink_0()
                        .left(horizontal_offset)
                        .children(horizontal_visible_range.clone().map(|relative_ix| {
                            let col_ix = relative_ix + fixed_count;
                            if col_ix < columns_count {
                                self.render_row_cell(row_ix, col_ix, window, cx)
                                    .into_any_element()
                            } else {
                                self.delegate
                                    .render_last_empty_col(window, cx)
                                    .into_any_element()
                            }
                        })),
                ),
            );

            // Selected row overlay.
            tr = tr.when(is_selected, |this| {
                this.child(
                    div()
                        .top(if row_ix == 0 { px(0.0) } else { px(-1.0) })
                        .left(px(0.0))
                        .right(px(0.0))
                        .bottom(px(-1.0))
                        .absolute()
                        .bg(with_alpha(palette.accent, 0.15))
                        .border_1()
                        .border_color(palette.accent),
                )
            });
            // Right-clicked row border.
            tr = tr.when(is_right_clicked, |this| {
                this.child(
                    div()
                        .top(if row_ix == 0 { px(0.0) } else { px(-1.0) })
                        .left(px(0.0))
                        .right(px(0.0))
                        .bottom(px(-1.0))
                        .absolute()
                        .border_1()
                        .border_color(with_alpha(palette.accent, 0.6)),
                )
            });

            tr.on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e, window, cx| {
                    this.on_row_right_click(e, row_ix, window, cx);
                }),
            )
            .on_click(cx.listener(move |this, e, _, cx| {
                this.on_row_left_click(e, row_ix, cx);
            }))
        } else {
            // Fake rows fill the stripe pattern to the bottom.
            self.delegate
                .render_tr(row_ix, window, cx)
                .flex()
                .flex_row()
                .w_full()
                .h(row_height)
                .border_b_1()
                .border_color(palette.border)
                .when(is_stripe_row, |this| {
                    this.bg(blend(palette.surface, palette.fg, 0.04))
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .relative()
                        .h_full()
                        .w(right_content_width)
                        .flex_shrink_0()
                        .left(horizontal_offset)
                        .children(horizontal_visible_range.map(|relative_ix| {
                            let col_ix = relative_ix + fixed_count;
                            if col_ix < columns_count {
                                self.render_cell(col_ix).into_any_element()
                            } else {
                                self.delegate
                                    .render_last_empty_col(window, cx)
                                    .into_any_element()
                            }
                        })),
                )
        }
    }

    /// Extra rows to fill the tail when stripe is on.
    pub(crate) fn calculate_extra_rows_needed(
        &self,
        total_height: Pixels,
        actual_height: Pixels,
        row_height: Pixels,
    ) -> usize {
        let remaining_height = total_height - actual_height;
        if remaining_height > px(0.0) {
            (remaining_height / row_height).floor() as usize
        } else {
            0
        }
    }

    pub(crate) fn render_vertical_scrollbar(&mut self) -> Option<impl IntoElement> {
        Some(
            div()
                .absolute()
                .top(self.options.row_height)
                .right_0()
                .bottom_0()
                .w(px(SCROLLBAR_WIDTH))
                .child(
                    ScrollbarRail::vertical(
                        "table-vscroll",
                        "tm-table-vscroll",
                        Rc::new(self.vertical_scroll_handle.clone()),
                        self.palette,
                    )
                    .track_debug_selector("tm-table-vscroll-track"),
                ),
        )
    }

    pub(crate) fn render_horizontal_scrollbar(&mut self) -> impl IntoElement {
        div()
            .occlude()
            .absolute()
            .left(self.fixed_head_cols_bounds.size.width)
            .right(if self.options.scrollbar_visible.right {
                px(SCROLLBAR_WIDTH)
            } else {
                px(0.0)
            })
            .bottom_0()
            .h(px(SCROLLBAR_HEIGHT))
            .child(
                Scrollbar::horizontal(
                    "table-hscroll",
                    Rc::new(self.horizontal_scroll_handle.clone()),
                    self.palette,
                )
                .show(ScrollbarShow::Always),
            )
    }
}
