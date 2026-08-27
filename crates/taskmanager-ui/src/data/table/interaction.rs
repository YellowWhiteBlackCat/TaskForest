//! Selection, click, sort/move, load-more and keyboard action methods of
//! `TableState` (absorption §4.5-4.8).

use std::ops::Range;

use gpui::{
    Axis, Bounds, ClickEvent, Context, DismissEvent, MouseDownEvent, Pixels, ScrollStrategy, Window,
};

use super::{
    ContextMenuOpen, PopupMenuState, SortState, TableActivate, TableCancel, TableDelegate,
    TableEvent, TableSelectDown, TableSelectEnd, TableSelectHome, TableSelectNextColumn,
    TableSelectPrevColumn, TableSelectUp, TableSelection, TableState, TableVisibleRange,
    validate_leading_fixed,
};

impl<D: TableDelegate> TableState<D> {
    /// Select `row_ix`, scrolling it into view (direction-aware strategy:
    /// moving down uses Bottom, moving up uses Top, absorption 4.6-8).
    pub fn set_selected_row(&mut self, row_ix: usize, cx: &mut Context<Self>) {
        if !self.row_selectable {
            return;
        }
        let is_down = match self.selection {
            TableSelection::Row(selected) => row_ix > selected,
            _ => true,
        };
        self.selection = TableSelection::Row(row_ix);
        self.right_clicked_row = None;
        self.vertical_scroll_handle.scroll_to_item(
            row_ix,
            if is_down {
                ScrollStrategy::Bottom
            } else {
                ScrollStrategy::Top
            },
        );
        cx.emit(TableEvent::SelectRow(row_ix));
        cx.notify();
    }

    /// Select `col_ix`, scrolling it into view.
    pub fn set_selected_col(&mut self, col_ix: usize, cx: &mut Context<Self>) {
        if !self.col_selectable {
            return;
        }
        self.selection = TableSelection::Column(col_ix);
        self.scroll_to_col(col_ix, cx);
        cx.emit(TableEvent::SelectColumn(col_ix));
        cx.notify();
    }

    /// Clear the selection.
    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selection = TableSelection::None;
        cx.notify();
    }

    /// Scroll the given row to the top of the viewport.
    pub fn scroll_to_row(&mut self, row_ix: usize, cx: &mut Context<Self>) {
        self.vertical_scroll_handle
            .scroll_to_item(row_ix, ScrollStrategy::Top);
        cx.notify();
    }

    /// Scroll the given column into view (fixed columns are never scrolled;
    /// the request is offset past them, absorption 4.6-7).
    pub fn scroll_to_col(&mut self, col_ix: usize, cx: &mut Context<Self>) {
        let fixed_count = self.fixed_left_cols_count();
        self.horizontal_scroll_handle
            .scroll_to_item(col_ix.saturating_sub(fixed_count), ScrollStrategy::Top);
        cx.notify();
    }

    /// The visible row/column ranges.
    pub fn visible_range(&self) -> &TableVisibleRange {
        &self.visible_range
    }

    /// The table container bounds (written back by a canvas child).
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    pub(crate) fn on_row_left_click(
        &mut self,
        e: &ClickEvent,
        row_ix: usize,
        cx: &mut Context<Self>,
    ) {
        self.set_selected_row(row_ix, cx);
        if e.click_count() == 2 {
            cx.emit(TableEvent::DoubleClickedRow(row_ix));
        }
    }

    pub(crate) fn on_row_right_click(
        &mut self,
        e: &MouseDownEvent,
        row_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.right_clicked_row = Some(row_ix);

        // Open the delegate-filled context menu and focus it immediately
        // (absorption 2.6-2 / 3.6-8: open actions own the focus). The
        // delegate builds with the table's typed context; the component
        // owns presentation (palette/anchor) and self-renders once mounted.
        let mut menu = PopupMenuState::new(Vec::new(), cx);
        menu.set_action_context(self.focus_handle.clone());
        let menu = self.delegate.context_menu(row_ix, menu, cx);
        let menu_entity = menu.mount(self.palette, e.position, window, cx);
        let subscription = cx.subscribe(&menu_entity, |this, _menu, _: &DismissEvent, cx| {
            this.context_menu = None;
            cx.notify();
        });
        self._context_menu_subscription = Some(subscription);
        self.context_menu = Some(ContextMenuOpen { menu: menu_entity });
        cx.notify();
    }

    pub(crate) fn on_col_head_click(&mut self, col_ix: usize, cx: &mut Context<Self>) {
        if !self.col_selectable {
            return;
        }
        let Some(col_group) = self.col_groups.get(col_ix) else {
            return;
        };
        if !col_group.column.selectable {
            return;
        }
        self.set_selected_col(col_ix, cx);
    }

    /// Cycle the sort of `col_ix` (Unsorted → Descending → Ascending →
    /// Unsorted) and reset every other column (single-column activation,
    /// absorption §4.3-E). Emits [`TableEvent::SortChanged`].
    pub fn perform_sort(&mut self, col_ix: usize, cx: &mut Context<Self>) {
        if !self.sortable {
            return;
        }
        let Some(group) = self.col_groups.get(col_ix) else {
            return;
        };
        let Some(initial) = group.column.sort else {
            // Column is not sortable.
            return;
        };
        let base = if group.sort.is_active() {
            group.sort
        } else {
            initial
        };
        let next = base.cycle();
        for (ix, group) in self.col_groups.iter_mut().enumerate() {
            if ix == col_ix {
                group.sort = next;
            } else if group.column.sort.is_some() {
                group.sort = SortState::Unsorted;
            }
        }
        self.delegate_mut().perform_sort(col_ix, next, cx);
        cx.emit(TableEvent::SortChanged { col_ix, sort: next });
        cx.notify();
    }

    /// Move the column at `from` to `to` (same table only).
    pub fn move_column(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if from == to || from >= self.col_groups.len() || to >= self.col_groups.len() {
            return;
        }
        self.delegate_mut().move_column(from, to, cx);
        let col_group = self.col_groups.remove(from);
        self.col_groups.insert(to, col_group);
        let _ = validate_leading_fixed(&self.col_groups);
        cx.emit(TableEvent::MoveColumn(from, to));
        cx.notify();
    }

    /// Apply a new width to `ix` during a resize drag, clamped to
    /// `[MIN_COL_WIDTH, MAX_COL_WIDTH]` and ignoring <1px jitter.
    /// Dispatch `load_more` when the visible end is within the threshold of
    /// the bottom. Guarded by `is_eof` (true = more data may exist) and an
    /// in-flight lock (附录 A-8 fix: a delegate without its own lock cannot
    /// stack duplicate load tasks).
    pub(crate) fn load_more_if_need(
        &mut self,
        rows_count: usize,
        visible_end: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let threshold = self.delegate.load_more_threshold();
        if visible_end < rows_count.saturating_sub(threshold) {
            return;
        }
        if !self.delegate.is_eof(cx) {
            return;
        }
        if self.loading_more {
            return;
        }
        self.loading_more = true;
        self._load_more_task = cx.spawn_in(window, async move |view, window| {
            let _ = view.update_in(window, |view, _window, cx| {
                // Notify only when `load_more` actually grew the data: a
                // no-op `load_more` (static tables) must not queue a redraw,
                // otherwise the dispatch → task → notify → redraw → dispatch
                // cycle loops forever (P5 regression, found headlessly).
                // Delegates that load asynchronously notify themselves.
                let rows_before = view.delegate.rows_count(cx);
                view.delegate_mut().load_more(cx);
                let rows_after = view.delegate.rows_count(cx);
                view.loading_more = false;
                if rows_before != rows_after {
                    cx.notify();
                }
            });
        });
    }

    /// Update the visible range and notify the delegate. The `len <= 1`
    /// skip is kept on purpose (absorption 4.6-1): the first frame is the
    /// measurement frame and must not fire callbacks.
    pub(crate) fn update_visible_range_if_need(
        &mut self,
        visible_range: Range<usize>,
        axis: Axis,
        cx: &mut Context<Self>,
    ) {
        if visible_range.len() <= 1 {
            return;
        }
        if axis == Axis::Vertical {
            if self.visible_range.rows == visible_range {
                return;
            }
            self.delegate_mut()
                .visible_rows_changed(visible_range.clone(), cx);
            self.visible_range.rows = visible_range;
        } else {
            if self.visible_range.cols == visible_range {
                return;
            }
            self.delegate_mut()
                .visible_columns_changed(visible_range.clone(), cx);
            self.visible_range.cols = visible_range;
        }
    }

    /// Keyboard Up (row cursor; wraps).
    pub fn action_select_up(&mut self, _: &TableSelectUp, _: &mut Window, cx: &mut Context<Self>) {
        let rows_count = self.delegate.rows_count(cx);
        if rows_count < 1 {
            return;
        }
        let current = self.selection.row().unwrap_or(0);
        let next = if current > 0 {
            current.saturating_sub(1)
        } else if self.loop_selection {
            rows_count.saturating_sub(1)
        } else {
            current
        };
        self.set_selected_row(next, cx);
    }

    /// Keyboard Down (row cursor; wraps).
    pub fn action_select_down(
        &mut self,
        _: &TableSelectDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows_count = self.delegate.rows_count(cx);
        if rows_count < 1 {
            return;
        }
        let next = match self.selection.row() {
            // Entering from outside the table: land on the first row.
            None => 0,
            Some(current) if current < rows_count.saturating_sub(1) => current + 1,
            Some(_) if self.loop_selection => 0,
            Some(current) => current,
        };
        self.set_selected_row(next, cx);
    }

    /// Keyboard Left (column cursor; wraps).
    pub fn action_select_prev_col(
        &mut self,
        _: &TableSelectPrevColumn,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let columns_count = self.delegate.columns_count(cx);
        if columns_count < 1 {
            return;
        }
        let current = self.selection.column().unwrap_or(0);
        let next = if current > 0 {
            current.saturating_sub(1)
        } else if self.loop_selection {
            columns_count.saturating_sub(1)
        } else {
            current
        };
        self.set_selected_col(next, cx);
    }

    /// Keyboard Right (column cursor; wraps).
    pub fn action_select_next_col(
        &mut self,
        _: &TableSelectNextColumn,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let columns_count = self.delegate.columns_count(cx);
        if columns_count < 1 {
            return;
        }
        let next = match self.selection.column() {
            // Entering from outside the table: land on the first column.
            None => 0,
            Some(current) if current < columns_count.saturating_sub(1) => current + 1,
            Some(_) if self.loop_selection => 0,
            Some(current) => current,
        };
        self.set_selected_col(next, cx);
    }

    /// Keyboard Home: select the first row.
    pub fn action_select_home(
        &mut self,
        _: &TableSelectHome,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.delegate.rows_count(cx) < 1 {
            return;
        }
        self.set_selected_row(0, cx);
    }

    /// Keyboard End: select the last row.
    pub fn action_select_end(
        &mut self,
        _: &TableSelectEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows_count = self.delegate.rows_count(cx);
        if rows_count < 1 {
            return;
        }
        self.set_selected_row(rows_count.saturating_sub(1), cx);
    }

    /// Keyboard Enter: activate the selected row.
    pub fn action_activate(&mut self, _: &TableActivate, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.selection.row() {
            cx.emit(TableEvent::ActivateRow(row));
        }
    }

    /// Keyboard Escape: clear the selection, or propagate when there is
    /// nothing to clear (absorption §4.3-H).
    pub fn action_cancel(&mut self, _: &TableCancel, _: &mut Window, cx: &mut Context<Self>) {
        if self.has_selection() {
            self.clear_selection(cx);
        } else {
            cx.propagate();
        }
    }
}
