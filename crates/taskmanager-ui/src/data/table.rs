//! Virtualized data table (absorption §4).
//!
//! Row virtualization uses the gpui `uniform_list` primitive (uniform rows,
//! legal per §4.5).  The table computes one horizontal visible range per frame
//! and paints that clipped column band in every visible row; this avoids a
//! separate variable-size virtual-list layout for each row while preserving
//! per-column widths. Sorting is a three-state cycle on a single active column;
//! runtime sort state and widths live in [`ColGroup`] so
//! [`TableState::refresh`] merges them by column `key` instead of losing them
//! (附录 A-9 fix).
//!
//! Defect fixes over gc (附录 A):
//! - A-1: only a **leading** run of columns may be `fixed_left`
//!   ([`leading_fixed_cols_count`]); header and rows iterate real column
//!   indices, never enumeration positions.
//! - A-2: `render_th` uses `get()` and skips rendering — no
//!   `expect("BUG: invalid col index")` panic after `refresh()` shrinks.
//! - A-4/A-10: a window-level `MouseUp` listener (registered from a
//!   paint-stage canvas child) finishes a column resize even when the
//!   pointer is released outside the window.
//! - A-7: `update_visible_range_if_need` keeps the `len <= 1` skip (first
//!   item measurement frame); the tests lock it.
//! - A-8: `load_more` dispatches are gated by an in-flight lock
//!   (`loading_more`) so a delegate without its own lock cannot stack
//!   duplicate load tasks; `is_eof` keeps gc's inverted semantics
//!   (`true` = more data may exist) and is documented. The spawned task
//!   notifies only when `load_more` grew the rows — a no-op `load_more`
//!   must not queue a redraw or the dispatch→notify→redraw cycle loops.
//! - A-9: `refresh()` merges runtime width/sort by column key.
//!
//! Selection is a single [`TableSelection`] enum (row XOR column XOR none):
//! the gc "two Options plus a mode flag" illegal state is unrepresentable.

use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    App, Axis, Bounds, Context, Div, Edges, ElementId, Entity, EntityId, EventEmitter, FocusHandle,
    Focusable, InteractiveElement, IntoElement, KeyBinding, ListSizingBehavior, MouseUpEvent,
    ParentElement, Pixels, Point, Render, RenderOnce, Size, Stateful, Styled, Subscription, Task,
    UniformListScrollHandle, Window, actions, canvas, div, px, uniform_list,
};

use taskmanager_theme::{Palette, Theme};

use crate::data::virtual_list::VirtualListScrollHandle;
use crate::overlays::popup::PopupMenuState;
use crate::primitives::scrollbar::ScrollbarHandle;
use crate::styled::blend;
use taskmanager_theme::tokens;

mod columns;
mod interaction;
mod model;
mod render;

pub use columns::{
    ColGroup, SortState, TableColumn, leading_fixed_cols_count, validate_leading_fixed,
};
pub use model::{TableEvent, TableSelection, TableVisibleRange};
/// The table key context (navigation bindings live under it).
pub const TABLE_CONTEXT: &str = "TaskManagerTable";

actions!(
    table,
    [
        TableSelectUp,
        TableSelectDown,
        TableSelectPrevColumn,
        TableSelectNextColumn,
        TableSelectHome,
        TableSelectEnd,
        TableActivate,
        TableCancel,
    ]
);

/// Register the table keymap (absorption §4.2: up/down rows, left/right
/// columns, Home/End, Enter activates, Escape clears selection).
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", TableCancel, Some(TABLE_CONTEXT)),
        KeyBinding::new("up", TableSelectUp, Some(TABLE_CONTEXT)),
        KeyBinding::new("down", TableSelectDown, Some(TABLE_CONTEXT)),
        KeyBinding::new("left", TableSelectPrevColumn, Some(TABLE_CONTEXT)),
        KeyBinding::new("right", TableSelectNextColumn, Some(TABLE_CONTEXT)),
        KeyBinding::new("home", TableSelectHome, Some(TABLE_CONTEXT)),
        KeyBinding::new("end", TableSelectEnd, Some(TABLE_CONTEXT)),
        KeyBinding::new("enter", TableActivate, Some(TABLE_CONTEXT)),
    ]);
}

/// The table's row scroll handle drives our scrollbar too.
impl ScrollbarHandle for UniformListScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        self.0.borrow().base_handle.offset()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.0.borrow().base_handle.set_offset(offset);
    }

    fn max_offset(&self) -> Size<Pixels> {
        self.0.borrow().base_handle.max_offset()
    }

    fn viewport(&self) -> Bounds<Pixels> {
        self.0.borrow().base_handle.bounds()
    }
}

/// Table appearance options.
#[derive(Clone, Debug)]
pub struct TableOptions {
    /// Zebra striping (fake rows fill the tail so stripes reach the bottom).
    pub stripe: bool,
    /// Rounded container with a border.
    pub bordered: bool,
    /// Uniform row height (rows are virtualized uniformly).
    pub row_height: Pixels,
    /// Whether the vertical/horizontal scrollbars render.
    pub scrollbar_visible: Edges<bool>,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            stripe: false,
            bordered: true,
            row_height: px(28.0),
            scrollbar_visible: Edges::all(true),
        }
    }
}

/// The table delegate: data access + cell rendering (absorption §4.5).
///
/// Deviation from the §4.5 sketch: `context_menu` receives a
/// [`PopupMenuState`] (the state half of the State/Element split) instead of
/// a gc-style menu element, so the delegate only fills items; window-less
/// callbacks drop the gc `window` parameter.
pub trait TableDelegate: Sized + 'static {
    /// The number of columns.
    fn columns_count(&self, cx: &App) -> usize;

    /// The number of rows.
    fn rows_count(&self, cx: &App) -> usize;

    /// The column at `col_ix` (only called during prepare/refresh).
    fn column(&self, col_ix: usize, cx: &App) -> &TableColumn;

    /// Render the cell at `(row_ix, col_ix)` (the only required method).
    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement;

    /// Render the header cell of `col_ix` (default: the column name).
    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .child(self.column(col_ix, cx).name.clone())
    }

    /// Render the row wrapper for `row_ix`.
    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        div().id(("row", row_ix))
    }

    /// Render the table header row wrapper.
    fn render_header(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        div().id("header")
    }

    /// Render the empty state (default: a blank centered slot; business
    /// views provide localized copy).
    fn render_empty(
        &mut self,
        _palette: Palette,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div().size_full().into_any_element()
    }

    /// Render the loading state over the table body (default: a skeleton
    /// with one header row and filled data rows).
    fn render_loading(
        &mut self,
        viewport: Size<Pixels>,
        row_height: Pixels,
        palette: Palette,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let data_rows = (viewport.height / row_height).floor().max(3.0) as usize;
        let skeleton_block = |h: f32, tinted: bool| {
            div()
                .h(px(h))
                .flex_shrink_0()
                .border_b_1()
                .border_color(palette.border)
                .px(tokens::SPACE_10)
                .py(tokens::SPACE_5)
                .child(div().h_full().rounded(palette.small_radius).bg(if tinted {
                    blend(palette.surface, palette.fg, 0.10)
                } else {
                    blend(palette.surface, palette.fg, 0.06)
                }))
        };
        let row_height: f32 = row_height.into();
        div()
            .size_full()
            .flex_col()
            .child(skeleton_block(row_height, true))
            .children((0..data_rows).map(|_| skeleton_block(row_height, false)))
    }

    /// Render the trailing empty column (default: 12px).
    fn render_last_empty_col(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div().w(px(12.0)).h_full().flex_shrink_0()
    }

    /// Width of the trailing empty column returned by
    /// [`TableDelegate::render_last_empty_col`]. Delegates that customize the
    /// element must override both methods together so the header, body
    /// virtual list, and scrollbar share one horizontal extent.
    fn last_empty_col_width(&self) -> Pixels {
        px(12.0)
    }

    /// Whether the table is loading.
    fn loading(&self, _cx: &App) -> bool {
        false
    }

    /// Sort the data of `col_ix` into `sort` order (data sorting is the
    /// delegate's job; the state machine already cycled and reset others).
    fn perform_sort(
        &mut self,
        _col_ix: usize,
        _sort: SortState,
        _cx: &mut Context<TableState<Self>>,
    ) {
    }

    /// Move the column at `from` to `to` (the state keeps its own copy).
    fn move_column(&mut self, _from: usize, _to: usize, _cx: &mut Context<TableState<Self>>) {}

    /// Fill `menu` with the context menu items for `row_ix`.
    fn context_menu(
        &mut self,
        _row_ix: usize,
        menu: PopupMenuState,
        _cx: &mut Context<TableState<Self>>,
    ) -> PopupMenuState {
        menu
    }

    /// Load more data near the bottom (dispatched in a background task).
    fn load_more(&mut self, _cx: &mut Context<TableState<Self>>) {}

    /// `true` while more data may exist. **Inverted naming kept from gc**:
    /// `true` = load_more may still fetch data; return `false` once
    /// exhausted (附录 A-8). A delegate without its own loading lock is
    /// still safe: the table holds an in-flight lock.
    fn is_eof(&self, _cx: &App) -> bool {
        true
    }

    /// Rows remaining before the bottom that trigger `load_more`.
    fn load_more_threshold(&self) -> usize {
        20
    }

    /// The visible row range changed (skipped on the first measurement
    /// frame, `len <= 1`, absorption 4.6-1). Keep this fast.
    fn visible_rows_changed(&mut self, _range: Range<usize>, _cx: &mut Context<TableState<Self>>) {}

    /// The visible column range changed (same len<=1 rule). Keep this fast.
    fn visible_columns_changed(
        &mut self,
        _range: Range<usize>,
        _cx: &mut Context<TableState<Self>>,
    ) {
    }
}

pub(crate) struct TableRowLayout<'a> {
    rows_count: usize,
    fixed_count: usize,
    col_sizes: &'a [Pixels],
    horizontal_visible_range: Range<usize>,
    horizontal_offset: Pixels,
    columns_count: usize,
    is_filled: bool,
}

/// Build the one source of truth for horizontally virtualized column widths.
/// The trailing empty column is part of the virtual list extent as well as the
/// header's visual tail; otherwise the shared handle can travel 12px farther
/// than the body and visibly snap back at the right edge.
pub(crate) fn horizontal_scroll_widths(
    groups: &[ColGroup],
    fixed_count: usize,
    trailing_width: Pixels,
) -> Vec<Pixels> {
    let mut widths: Vec<Pixels> = groups
        .iter()
        .skip(fixed_count)
        .map(|group| group.width)
        .collect();
    widths.push(trailing_width);
    widths
}

/// A right-click context menu opened on a row.
struct ContextMenuOpen {
    menu: Entity<PopupMenuState>,
}

/// Drag payload for column resize (typed, absorbed from gc `ResizeColumn`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResizeColumn(pub (EntityId, usize));

pub struct TableState<D: TableDelegate> {
    focus_handle: FocusHandle,
    delegate: D,
    /// Appearance options (set by the [`Table`] element).
    pub options: TableOptions,
    /// Color contract (set by the [`Table`] element each render).
    pub palette: Palette,
    /// Runtime column groups (width/sort/bounds).
    pub col_groups: Vec<ColGroup>,
    /// Row scroll (uniform list).
    pub vertical_scroll_handle: UniformListScrollHandle,
    /// Column scroll (shared with the header).
    pub horizontal_scroll_handle: VirtualListScrollHandle,
    /// The current selection (row XOR column XOR none).
    pub selection: TableSelection,
    /// Selection wraps at the table edges (default true).
    pub loop_selection: bool,
    /// Column selection enabled (default true).
    pub col_selectable: bool,
    /// Row selection enabled (default true).
    pub row_selectable: bool,
    /// Sorting enabled (default true).
    pub sortable: bool,
    /// Column width dragging enabled (default true).
    pub col_resizable: bool,
    /// Column move dragging enabled (default true).
    pub col_movable: bool,
    /// Fixed-column feature enabled (default true).
    pub col_fixed: bool,
    bounds: Bounds<Pixels>,
    fixed_head_cols_bounds: Bounds<Pixels>,
    right_clicked_row: Option<usize>,
    resizing_col: Option<usize>,
    visible_range: TableVisibleRange,
    loading_more: bool,
    context_menu: Option<ContextMenuOpen>,
    _load_more_task: Task<()>,
    _context_menu_subscription: Option<Subscription>,
}

impl<D: TableDelegate> TableState<D> {
    /// Create a table state bound to `delegate`.
    pub fn new(delegate: D, cx: &mut App) -> Self {
        let mut this = Self {
            focus_handle: cx.focus_handle().tab_stop(true),
            delegate,
            options: TableOptions::default(),
            palette: Theme::dark().palette(),
            col_groups: Vec::new(),
            vertical_scroll_handle: UniformListScrollHandle::new(),
            horizontal_scroll_handle: VirtualListScrollHandle::new(),
            selection: TableSelection::None,
            loop_selection: true,
            col_selectable: true,
            row_selectable: true,
            sortable: true,
            col_resizable: true,
            col_movable: true,
            col_fixed: true,
            bounds: Bounds::default(),
            fixed_head_cols_bounds: Bounds::default(),
            right_clicked_row: None,
            resizing_col: None,
            visible_range: TableVisibleRange::default(),
            loading_more: false,
            context_menu: None,
            _load_more_task: Task::ready(()),
            _context_menu_subscription: None,
        };
        this.prepare_col_groups(cx);
        this
    }

    /// The delegate.
    pub fn delegate(&self) -> &D {
        &self.delegate
    }

    /// The delegate (mutably).
    pub fn delegate_mut(&mut self) -> &mut D {
        &mut self.delegate
    }

    /// Loop row/column selection at the edges (default true).
    #[must_use]
    pub fn loop_selection(mut self, loop_selection: bool) -> Self {
        self.loop_selection = loop_selection;
        self
    }

    /// Enable/disable column moves (default true).
    #[must_use]
    pub fn col_movable(mut self, col_movable: bool) -> Self {
        self.col_movable = col_movable;
        self
    }

    /// Enable/disable column resizing (default true).
    #[must_use]
    pub fn col_resizable(mut self, col_resizable: bool) -> Self {
        self.col_resizable = col_resizable;
        self
    }

    /// Enable/disable sorting (default true).
    #[must_use]
    pub fn sortable(mut self, sortable: bool) -> Self {
        self.sortable = sortable;
        self
    }

    /// Enable/disable row selection (default true).
    #[must_use]
    pub fn row_selectable(mut self, row_selectable: bool) -> Self {
        self.row_selectable = row_selectable;
        self
    }

    /// Enable/disable column selection (default true).
    #[must_use]
    pub fn col_selectable(mut self, col_selectable: bool) -> Self {
        self.col_selectable = col_selectable;
        self
    }

    /// Enable/disable the fixed-column feature (default true).
    #[must_use]
    pub fn col_fixed(mut self, col_fixed: bool) -> Self {
        self.col_fixed = col_fixed;
        self
    }

    /// Rebuild column groups after columns or rows changed. Runtime width
    /// and sort states are merged by column `key`, so a `refresh()` never
    /// silently drops the current sort (附录 A-9 fix).
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let old_groups = std::mem::take(&mut self.col_groups);
        self.prepare_col_groups(cx);
        for new in &mut self.col_groups {
            if let Some(old) = old_groups
                .iter()
                .find(|old| old.column.key == new.column.key)
            {
                if new.column.resizable {
                    new.width = old.width;
                }
                if new.column.sort.is_some() {
                    new.sort = old.sort;
                }
            }
        }
        cx.notify();
    }

    fn prepare_col_groups(&mut self, cx: &App) {
        self.col_groups = (0..self.delegate.columns_count(cx))
            .map(|col_ix| {
                let column = self.delegate.column(col_ix, cx);
                ColGroup {
                    width: column.width,
                    bounds: Bounds::default(),
                    column: column.clone(),
                    sort: column.sort.unwrap_or_default(),
                }
            })
            .collect();
        let _ = validate_leading_fixed(&self.col_groups);
    }

    /// The number of leading fixed columns (respects `col_fixed`).
    pub fn fixed_left_cols_count(&self) -> usize {
        if self.col_fixed {
            leading_fixed_cols_count(&self.col_groups)
        } else {
            0
        }
    }

    /// The currently selected row, if row-selected.
    pub fn selected_row(&self) -> Option<usize> {
        self.selection.row()
    }

    /// The currently selected column, if column-selected.
    pub fn selected_col(&self) -> Option<usize> {
        self.selection.column()
    }

    /// Whether anything is selected.
    pub fn has_selection(&self) -> bool {
        !matches!(self.selection, TableSelection::None)
    }
}

impl<D: TableDelegate> Focusable for TableState<D> {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl<D: TableDelegate> EventEmitter<TableEvent> for TableState<D> {}

impl<D: TableDelegate> Render for TableState<D> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let columns_count = self.delegate.columns_count(cx);
        let fixed_count = if self.col_fixed {
            leading_fixed_cols_count(&self.col_groups)
        } else {
            0
        };
        let rows_count = self.delegate.rows_count(cx);
        let loading = self.delegate.loading(cx);
        let palette = self.palette;
        let row_height = self.options.row_height;

        let total_height = self
            .vertical_scroll_handle
            .0
            .borrow()
            .base_handle
            .bounds()
            .size
            .height;
        let actual_height = row_height * rows_count as f32;
        let extra_rows_count =
            self.calculate_extra_rows_needed(total_height, actual_height, row_height);
        let render_rows_count = if self.options.stripe {
            rows_count + extra_rows_count
        } else {
            rows_count
        };
        let right_clicked_row = self.right_clicked_row;
        let is_filled = total_height > Pixels::ZERO && total_height <= actual_height;

        // Render the context menu opened by a right-click (if any). The
        // menu is a self-rendering view: mounting the entity reuses the
        // cached element tree unless the menu itself is dirty.
        let context_menu_view = self
            .context_menu
            .as_ref()
            .map(|open| open.menu.clone().into_any_element());

        let inner_table = div()
            .id("table-inner")
            .size_full()
            .flex_col()
            .overflow_hidden()
            .child(self.render_table_header(fixed_count, window, cx))
            .when(rows_count > 0 && !loading, |this| {
                this.child(
                    div().id("table-body").flex_grow().size_full().child(
                        uniform_list(
                            "table-uniform-list",
                            render_rows_count,
                            cx.processor(move |table, visible_range: Range<usize>, window, cx| {
                                // Column sizes MUST come from the SAME source
                                // the header uses (`col_group.width`, applied
                                // synchronously via `.w()` in render_cell), not
                                // from `group.bounds.size.width`. The bounds
                                // field is painted by a post-layout canvas in
                                // render_th whose containing block is the whole
                                // columns row, so it resolves to the row width
                                // for every column and is also one frame stale —
                                // both of which desync the body's virtual_list
                                // from the header (and make off-screen columns
                                // unreachable, since header + body share one
                                // horizontal scroll handle). `col_group.width` is
                                // kept current by prepare_col_groups / refresh /
                                // resize_cols, so the body matches the header by
                                // construction.
                                let col_sizes: Rc<Vec<Pixels>> = Rc::new(horizontal_scroll_widths(
                                    &table.col_groups,
                                    fixed_count,
                                    table.delegate.last_empty_col_width(),
                                ));
                                let horizontal_offset = table.horizontal_scroll_handle.offset().x;
                                let horizontal_viewport =
                                    table.horizontal_scroll_handle.bounds().size.width;
                                let horizontal_visible_range = if horizontal_viewport > px(0.0) {
                                    crate::data::virtual_list::visible_range_for(
                                        &col_sizes,
                                        f32::from(horizontal_offset),
                                        f32::from(horizontal_viewport),
                                        0.0,
                                    )
                                } else {
                                    0..col_sizes.len()
                                };
                                table.update_visible_range_if_need(
                                    horizontal_visible_range.clone(),
                                    Axis::Horizontal,
                                    cx,
                                );

                                table.load_more_if_need(rows_count, visible_range.end, window, cx);
                                table.update_visible_range_if_need(
                                    visible_range.clone(),
                                    Axis::Vertical,
                                    cx,
                                );

                                // Self-correct stale scroll positions
                                // after the data shrank (absorption
                                // §4.3-B3).
                                if visible_range.end > rows_count {
                                    table.scroll_to_row(
                                        std::cmp::min(
                                            visible_range.start,
                                            rows_count.saturating_sub(1),
                                        ),
                                        cx,
                                    );
                                }

                                let mut items = Vec::with_capacity(
                                    visible_range.end.saturating_sub(visible_range.start),
                                );
                                for row_ix in visible_range {
                                    items.push(table.render_table_row(
                                        row_ix,
                                        TableRowLayout {
                                            rows_count,
                                            fixed_count,
                                            col_sizes: &col_sizes,
                                            horizontal_visible_range:
                                                horizontal_visible_range.clone(),
                                            horizontal_offset,
                                            columns_count,
                                            is_filled,
                                        },
                                        window,
                                        cx,
                                    ));
                                }
                                items
                            }),
                        )
                        .flex_grow()
                        .size_full()
                        .with_sizing_behavior(ListSizingBehavior::Auto)
                        .track_scroll(self.vertical_scroll_handle.clone())
                        .into_any_element(),
                    ),
                )
            })
            .when_some(context_menu_view, |this, menu| this.child(menu));

        let empty_view = if rows_count == 0 && !loading {
            Some(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(self.delegate.render_empty(palette, window, cx))
                    .into_any_element(),
            )
        } else {
            None
        };

        let loading_view = if loading {
            let viewport = self.bounds.size;
            Some(
                self.delegate
                    .render_loading(viewport, row_height, palette, window, cx)
                    .into_any_element(),
            )
        } else {
            None
        };

        let mut root = div()
            .size_full()
            .when_some(loading_view, |this, view| this.child(view))
            .when_some(empty_view, |this, view| this.child(view))
            .when(rows_count > 0 && !loading, |this| this.child(inner_table))
            .when(right_clicked_row.is_some(), |this| {
                this.on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.right_clicked_row = None;
                    cx.notify();
                }))
            })
            // Table container bounds, written back each frame.
            .child(canvas(
                {
                    let state = cx.entity();
                    move |bounds, _, cx| state.update(cx, |state, _| state.bounds = bounds)
                },
                |_, _, _, _| {},
            ));

        // Scrollbars (our own, driven through the ScrollbarHandle impls).
        root = root.child(
            div()
                .absolute()
                .top_0()
                .size_full()
                .when(self.options.scrollbar_visible.bottom, |this| {
                    this.child(self.render_horizontal_scrollbar())
                })
                .when(
                    self.options.scrollbar_visible.right && rows_count > 0,
                    |this| this.children(self.render_vertical_scrollbar()),
                ),
        );

        // Window-level MouseUp fallback for column resize (附录 A-10): the
        // paint-stage canvas registers a per-frame listener that finishes a
        // resize even when the pointer was released outside the window.
        {
            let state = cx.entity();
            root = root.child(canvas(
                |_, _, _| {},
                move |_, _, _window, _cx| {
                    _window.on_mouse_event(move |_event: &MouseUpEvent, phase, _w, cx| {
                        if phase.bubble() {
                            state.update(cx, |state, cx| state.finish_col_resize(cx));
                        }
                    });
                },
            ));
        }

        root
    }
}

/// The table element: binds the state to a key context, focus, and actions.
#[derive(IntoElement)]
pub struct Table<D: TableDelegate> {
    state: Entity<TableState<D>>,
    palette: Palette,
    options: TableOptions,
}

impl<D: TableDelegate> Table<D> {
    /// Build a table for `state` with the given color contract.
    pub fn new(state: &Entity<TableState<D>>, palette: Palette) -> Self {
        Self {
            state: state.clone(),
            palette,
            options: TableOptions::default(),
        }
    }

    /// Zebra striping (default false).
    #[must_use]
    pub fn stripe(mut self, stripe: bool) -> Self {
        self.options.stripe = stripe;
        self
    }

    /// Rounded, bordered container (default true).
    #[must_use]
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.options.bordered = bordered;
        self
    }

    /// Scrollbar visibility (default both visible).
    #[must_use]
    pub fn scrollbar_visible(mut self, vertical: bool, horizontal: bool) -> Self {
        self.options.scrollbar_visible = Edges {
            right: vertical,
            bottom: horizontal,
            ..Default::default()
        };
        self
    }

    /// Uniform row height (default 28px).
    #[must_use]
    pub fn row_height(mut self, row_height: impl Into<Pixels>) -> Self {
        self.options.row_height = row_height.into();
        self
    }
}

impl<D: TableDelegate> RenderOnce for Table<D> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_handle = self
            .state
            .read_with(cx, |state, _| state.focus_handle.clone());
        let bordered = self.options.bordered;
        let palette = self.palette;
        self.state.update(cx, |state, _| {
            state.options = self.options;
            state.palette = palette;
        });

        div()
            .id(ElementId::named_usize(
                "tm-table",
                self.state.entity_id().as_non_zero_u64().get() as usize,
            ))
            .debug_selector(|| "tm-table".into())
            .size_full()
            .key_context(TABLE_CONTEXT)
            .track_focus(&focus_handle)
            .on_action(window.listener_for(&self.state, TableState::action_select_up))
            .on_action(window.listener_for(&self.state, TableState::action_select_down))
            .on_action(window.listener_for(&self.state, TableState::action_select_prev_col))
            .on_action(window.listener_for(&self.state, TableState::action_select_next_col))
            .on_action(window.listener_for(&self.state, TableState::action_select_home))
            .on_action(window.listener_for(&self.state, TableState::action_select_end))
            .on_action(window.listener_for(&self.state, TableState::action_activate))
            .on_action(window.listener_for(&self.state, TableState::action_cancel))
            .bg(palette.surface)
            .when(bordered, |this| {
                this.rounded(palette.panel_radius)
                    .border_1()
                    .border_color(palette.border)
            })
            .child(self.state)
    }
}

#[cfg(test)]
#[path = "../../tests/gui/data/table.rs"]
mod tests;
