//! Headless behavior tests for the table (absorption §4, 附录 A regressions).

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::time::Instant;

// Explicit imports only: the `gpui::*` glob re-exports gpui's `test` attribute
// macro, which shadows the built-in `#[test]` and recursively expands until
// rustc SIGSEGVs. Same workaround as the other crate test modules.
use gpui::{
    App, AppContext, Axis, Bounds, ClickEvent, Context, Entity, IntoElement, Modifiers,
    MouseButton, MouseClickEvent, MouseDownEvent, MouseUpEvent, ParentElement, Render, Styled,
    TestAppContext, Window, div, point, px,
};

use taskmanager_theme::Theme;

use crate::overlays::popup::{MenuEntry, MenuItem, PopupMenuState};

use super::{
    ColGroup, ColumnResizeDrag, SortState, Table, TableColumn, TableDelegate, TableEvent,
    TableSelection, TableState, horizontal_scroll_widths, leading_fixed_cols_count,
    render::DragColumn, validate_leading_fixed, width_from_drag,
};

/// A scriptable delegate recording every callback.
#[derive(Clone)]
struct TestDelegate {
    columns: Vec<TableColumn>,
    rows: usize,
    td_calls: usize,
    sort_calls: Vec<(usize, SortState)>,
    moves: Vec<(usize, usize)>,
    eof: bool,
    load_more_calls: usize,
    visible_rows: Vec<Range<usize>>,
    visible_cols: Vec<Range<usize>>,
    grow_on_load: bool,
}

impl TestDelegate {
    fn new(columns: Vec<TableColumn>, rows: usize) -> Self {
        Self {
            columns,
            rows,
            td_calls: 0,
            sort_calls: Vec::new(),
            moves: Vec::new(),
            eof: true,
            load_more_calls: 0,
            visible_rows: Vec::new(),
            visible_cols: Vec::new(),
            grow_on_load: false,
        }
    }

    fn standard(rows: usize) -> Self {
        Self::new(
            vec![
                TableColumn::new("pid", "PID").sortable(),
                TableColumn::new("name", "Name").sortable().width(px(200.0)),
                TableColumn::new("cpu", "CPU %").sortable(),
                TableColumn::new("mem", "MEM").sortable(),
            ],
            rows,
        )
    }
}

impl TableDelegate for TestDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows
    }

    fn column(&self, col_ix: usize, _cx: &App) -> &TableColumn {
        &self.columns[col_ix]
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        self.td_calls += 1;
        div().child(format!("{row_ix}:{col_ix}"))
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: SortState,
        _cx: &mut Context<TableState<Self>>,
    ) {
        self.sort_calls.push((col_ix, sort));
    }

    fn move_column(&mut self, from: usize, to: usize, _cx: &mut Context<TableState<Self>>) {
        self.moves.push((from, to));
    }

    fn is_eof(&self, _cx: &App) -> bool {
        self.eof
    }

    fn load_more(&mut self, _cx: &mut Context<TableState<Self>>) {
        self.load_more_calls += 1;
        if self.grow_on_load {
            self.rows += 1;
        }
    }

    fn visible_rows_changed(&mut self, range: Range<usize>, _cx: &mut Context<TableState<Self>>) {
        self.visible_rows.push(range);
    }

    fn visible_columns_changed(
        &mut self,
        range: Range<usize>,
        _cx: &mut Context<TableState<Self>>,
    ) {
        self.visible_cols.push(range);
    }
}

/// A minimal window root so action methods (which require `&mut Window`)
/// can run headlessly.
#[derive(Default)]
struct TestRoot;

impl Render for TestRoot {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn setup(cx: &mut TestAppContext) -> Entity<TableState<TestDelegate>> {
    cx.new(|cx| TableState::new(TestDelegate::standard(50), cx))
}

/// Run `f` with a real window so window-bearing action methods work.
fn with_window<R>(cx: &mut TestAppContext, f: impl FnOnce(&mut Window, &mut App) -> R) -> R {
    let window = cx.add_window(|_window, _cx| TestRoot);
    cx.update_window(window.into(), |_root, window, cx| f(window, cx))
        .expect("window update")
}

#[test]
fn sort_cycle_three_states_loops() {
    assert_eq!(SortState::Unsorted.cycle(), SortState::Descending);
    assert_eq!(SortState::Descending.cycle(), SortState::Ascending);
    assert_eq!(SortState::Ascending.cycle(), SortState::Unsorted);
    assert!(!SortState::Unsorted.is_active());
    assert!(SortState::Ascending.is_active());
    assert!(SortState::Descending.is_active());
}

#[gpui::test]
async fn sort_cycles_and_activates_single_column(cx: &mut TestAppContext) {
    let table = setup(cx);
    table.update(cx, |table, cx| {
        // First click: Descending on column 0 only.
        table.perform_sort(0, cx);
        assert_eq!(table.col_groups[0].sort, SortState::Descending);
        assert_eq!(table.col_groups[1].sort, SortState::Unsorted);
        assert_eq!(table.col_groups[2].sort, SortState::Unsorted);
        assert_eq!(
            table.delegate().sort_calls,
            vec![(0, SortState::Descending)]
        );

        // Second click: Ascending, still single column.
        table.perform_sort(0, cx);
        assert_eq!(table.col_groups[0].sort, SortState::Ascending);

        // Third click: back to Unsorted.
        table.perform_sort(0, cx);
        assert_eq!(table.col_groups[0].sort, SortState::Unsorted);

        // Sorting another column deactivates the first.
        table.perform_sort(2, cx);
        assert_eq!(table.col_groups[0].sort, SortState::Unsorted);
        assert_eq!(table.col_groups[2].sort, SortState::Descending);
        assert_eq!(table.col_groups[3].sort, SortState::Unsorted);
        assert_eq!(
            table.delegate().sort_calls.last(),
            Some(&(2, SortState::Descending))
        );
    });
}

#[gpui::test]
async fn non_sortable_column_never_cycles(cx: &mut TestAppContext) {
    let table = cx.new(|cx| {
        let delegate = TestDelegate::new(
            vec![
                TableColumn::new("a", "A"),
                TableColumn::new("b", "B").sortable(),
            ],
            5,
        );
        TableState::new(delegate, cx)
    });
    table.update(cx, |table, cx| {
        table.perform_sort(0, cx);
        assert_eq!(table.col_groups[0].sort, SortState::Unsorted);
        assert!(table.delegate().sort_calls.is_empty());
    });
}

#[gpui::test]
async fn sort_emits_typed_event(cx: &mut TestAppContext) {
    let table = setup(cx);
    let received = Rc::new(RefCell::new(None::<TableEvent>));
    let sink = received.clone();
    cx.update(|cx| {
        cx.subscribe(&table, move |_, event: &TableEvent, _| {
            *sink.borrow_mut() = Some(event.clone());
        })
        .detach();
    });
    table.update(cx, |table, cx| table.perform_sort(0, cx));
    cx.run_until_parked();
    assert!(matches!(
        *received.borrow(),
        Some(TableEvent::SortChanged {
            col_ix: 0,
            sort: SortState::Descending
        })
    ));
}

/// 附录 A-9 regression: refresh() must keep the runtime sort and width of
/// columns that still exist (matched by key) and reset new ones.
#[gpui::test]
async fn refresh_preserves_sort_and_width_by_key(cx: &mut TestAppContext) {
    let table = cx.new(|cx| {
        let mut delegate = TestDelegate::standard(10);
        delegate.columns[1].width = px(240.0);
        TableState::new(delegate, cx)
    });
    // Apply a runtime sort + resize.
    table.update(cx, |table, cx| {
        table.perform_sort(1, cx);
        assert_eq!(table.col_groups[1].sort, SortState::Descending);
        table.col_groups[1].width = px(300.0);
    });
    // Delegate rebuilds columns (same keys + one new column).
    table.update(cx, |table, cx| {
        table
            .delegate_mut()
            .columns
            .push(TableColumn::new("new", "New").sortable());
        table.delegate_mut().columns[0].width = px(999.0); // fresh width
        table.refresh(cx);
        assert_eq!(table.col_groups.len(), 5);
        // Column "name" (ix 1) keeps sort + runtime width.
        let name = &table.col_groups[1];
        assert_eq!(name.column.key, "name");
        assert_eq!(name.sort, SortState::Descending);
        assert_eq!(name.width, px(300.0));
        // New column starts from its configured state.
        let new = table.col_groups.last().unwrap();
        assert_eq!(new.column.key, "new");
        assert_eq!(new.sort, SortState::Unsorted);
        assert_eq!(new.width, px(100.0));
        // Column 0 keeps its runtime width (merge wins over fresh value).
        assert_eq!(table.col_groups[0].width, px(100.0));
    });
}

#[gpui::test]
async fn selection_is_mutually_exclusive(cx: &mut TestAppContext) {
    let table = setup(cx);
    table.update(cx, |table, cx| {
        table.set_selected_row(3, cx);
        assert_eq!(table.selection, TableSelection::Row(3));
        assert_eq!(table.selected_row(), Some(3));
        assert_eq!(table.selected_col(), None);
        // Column selection replaces the row selection (no illegal
        // "both selected" state).
        table.set_selected_col(1, cx);
        assert_eq!(table.selection, TableSelection::Column(1));
        assert_eq!(table.selected_row(), None);
        assert_eq!(table.selected_col(), Some(1));
        table.clear_selection(cx);
        assert_eq!(table.selection, TableSelection::None);
        assert!(!table.has_selection());
    });
}

#[gpui::test]
async fn keyboard_rows_move_and_wrap(cx: &mut TestAppContext) {
    let table = cx.new(|cx| TableState::new(TestDelegate::standard(3), cx));
    with_window(cx, |window, cx| {
        table.update(cx, |table, cx| {
            // None selected: Down lands on 0.
            table.action_select_down(&super::TableSelectDown, window, cx);
            assert_eq!(table.selected_row(), Some(0));
            table.action_select_down(&super::TableSelectDown, window, cx);
            table.action_select_down(&super::TableSelectDown, window, cx);
            assert_eq!(table.selected_row(), Some(2));
            // Wrap at the bottom.
            table.action_select_down(&super::TableSelectDown, window, cx);
            assert_eq!(table.selected_row(), Some(0));
            // Wrap at the top.
            table.action_select_up(&super::TableSelectUp, window, cx);
            assert_eq!(table.selected_row(), Some(2));
        });
    });
}

#[gpui::test]
async fn keyboard_home_end_and_activate(cx: &mut TestAppContext) {
    let table = cx.new(|cx| TableState::new(TestDelegate::standard(5), cx));
    let activated = Rc::new(RefCell::new(None::<usize>));
    let sink = activated.clone();
    cx.update(|cx| {
        cx.subscribe(&table, move |_, event: &TableEvent, _| {
            if let TableEvent::ActivateRow(ix) = event {
                *sink.borrow_mut() = Some(*ix);
            }
        })
        .detach();
    });
    with_window(cx, |window, cx| {
        table.update(cx, |table, cx| {
            table.action_select_end(&super::TableSelectEnd, window, cx);
            assert_eq!(table.selected_row(), Some(4));
            table.action_select_home(&super::TableSelectHome, window, cx);
            assert_eq!(table.selected_row(), Some(0));
            table.action_activate(&super::TableActivate, window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(*activated.borrow(), Some(0));
}

#[gpui::test]
async fn keyboard_columns_move_and_wrap(cx: &mut TestAppContext) {
    let table = setup(cx);
    with_window(cx, |window, cx| {
        table.update(cx, |table, cx| {
            table.action_select_next_col(&super::TableSelectNextColumn, window, cx);
            assert_eq!(table.selected_col(), Some(0));
            table.action_select_next_col(&super::TableSelectNextColumn, window, cx);
            assert_eq!(table.selected_col(), Some(1));
            // Wrap at the last column.
            table.action_select_next_col(&super::TableSelectNextColumn, window, cx);
            table.action_select_next_col(&super::TableSelectNextColumn, window, cx);
            table.action_select_next_col(&super::TableSelectNextColumn, window, cx);
            assert_eq!(table.selected_col(), Some(0));
            // Up/Down switch to row mode and start from 0.
            table.action_select_down(&super::TableSelectDown, window, cx);
            assert_eq!(table.selected_row(), Some(0));
        });
    });
}

#[gpui::test]
async fn keyboard_escape_clears_selection(cx: &mut TestAppContext) {
    let table = setup(cx);
    with_window(cx, |window, cx| {
        table.update(cx, |table, cx| {
            table.set_selected_row(4, cx);
            table.action_cancel(&super::TableCancel, window, cx);
            assert_eq!(table.selection, TableSelection::None);
            // Second escape with no selection must not panic (propagates).
            table.action_cancel(&super::TableCancel, window, cx);
        });
    });
}

#[gpui::test]
async fn row_double_click_emits_event(cx: &mut TestAppContext) {
    let table = setup(cx);
    let received = Rc::new(RefCell::new(None::<TableEvent>));
    let sink = received.clone();
    cx.update(|cx| {
        cx.subscribe(&table, move |_, event: &TableEvent, _| {
            *sink.borrow_mut() = Some(event.clone());
        })
        .detach();
    });
    table.update(cx, |table, cx| {
        table.set_selected_row(2, cx);
        let click = ClickEvent::Mouse(MouseClickEvent {
            down: MouseDownEvent {
                button: MouseButton::Left,
                position: point(px(0.0), px(0.0)),
                modifiers: Modifiers::default(),
                click_count: 2,
                first_mouse: false,
            },
            up: MouseUpEvent {
                button: MouseButton::Left,
                position: point(px(0.0), px(0.0)),
                modifiers: Modifiers::default(),
                click_count: 2,
            },
        });
        table.on_row_left_click(&click, 2, cx);
    });
    cx.run_until_parked();
    assert!(matches!(
        *received.borrow(),
        Some(TableEvent::DoubleClickedRow(2))
    ));
}

#[test]
fn leading_fixed_run_is_counted_and_validated() {
    let col = |ix: usize, on: bool| {
        let mut column = TableColumn::new(format!("c{ix}"), format!("C{ix}"));
        column.fixed_left = on;
        ColGroup {
            column,
            width: px(10.0),
            bounds: Bounds::default(),
            sort: SortState::Unsorted,
        }
    };
    // Leading run of 2.
    let groups = vec![col(0, true), col(1, true), col(2, false), col(3, false)];
    assert_eq!(leading_fixed_cols_count(&groups), 2);
    assert_eq!(validate_leading_fixed(&groups), 2);
    // Non-leading fixed column is not counted as fixed (A-1).
    let groups = vec![col(0, false), col(1, true), col(2, false)];
    assert_eq!(leading_fixed_cols_count(&groups), 0);
    assert_eq!(validate_leading_fixed(&groups), 0);
}

#[test]
fn horizontal_extent_includes_the_trailing_empty_column() {
    let groups = vec![
        ColGroup {
            column: TableColumn::new("fixed", "Fixed"),
            width: px(80.0),
            bounds: Bounds::default(),
            sort: SortState::Unsorted,
        },
        ColGroup {
            column: TableColumn::new("scroll", "Scroll"),
            width: px(220.0),
            bounds: Bounds::default(),
            sort: SortState::Unsorted,
        },
    ];

    assert_eq!(
        horizontal_scroll_widths(&groups, 1, px(12.0)),
        vec![px(220.0), px(12.0)],
        "the body virtual list must share the header's trailing extent"
    );
}

#[gpui::test]
async fn scroll_to_col_skips_fixed_columns(cx: &mut TestAppContext) {
    let table = cx.new(|cx| {
        let mut delegate = TestDelegate::standard(3);
        delegate.columns[0].fixed_left = true;
        delegate.columns[1].fixed_left = true;
        TableState::new(delegate, cx)
    });
    table.update(cx, |table, cx| {
        assert_eq!(table.fixed_left_cols_count(), 2);
        // Column 3 maps to deferred item 1 in the horizontal list (0-based
        // after the 2 fixed columns; absorption 4.6-7).
        table.scroll_to_col(3, cx);
        let deferred = table.horizontal_scroll_handle.deferred_scroll();
        assert_eq!(deferred.map(|d| d.item_index), Some(1));
    });
}

/// 附录 A-7 lock: the visible-range callback skips single-item ranges (the
/// measurement frame) and only fires on actual changes.
#[gpui::test]
async fn visible_rows_changed_skips_measurement_frame(cx: &mut TestAppContext) {
    let table = setup(cx);
    table.update(cx, |table, cx| {
        table.update_visible_range_if_need(0..1, Axis::Vertical, cx);
        assert!(
            table.delegate().visible_rows.is_empty(),
            "len<=1 must not fire"
        );
        table.update_visible_range_if_need(2..12, Axis::Vertical, cx);
        assert_eq!(table.visible_range().rows(), &(2..12));
        assert_eq!(table.delegate().visible_rows, vec![2..12]);
        // Same range again: no duplicate callback.
        table.update_visible_range_if_need(2..12, Axis::Vertical, cx);
        assert_eq!(table.delegate().visible_rows.len(), 1);
        // Columns follow the same rule.
        table.update_visible_range_if_need(1..2, Axis::Horizontal, cx);
        assert!(table.delegate().visible_cols.is_empty());
        table.update_visible_range_if_need(0..3, Axis::Horizontal, cx);
        assert_eq!(table.delegate().visible_cols, vec![0..3]);
    });
}

/// 附录 A-8 fix: while a load_more task is in flight, further triggers are
/// locked out; after completion the lock releases.
#[gpui::test]
async fn load_more_is_gated_by_inflight_lock(cx: &mut TestAppContext) {
    let table = cx.new(|cx| TableState::new(TestDelegate::standard(10), cx));
    with_window(cx, |window, cx| {
        table.update(cx, |table, cx| {
            // Near the bottom (rows 10, threshold 20 → any end triggers).
            table.load_more_if_need(10, 5, window, cx);
            assert!(table.loading_more, "in-flight lock must be armed");
            // Second trigger while in flight: blocked (A-8).
            table.load_more_if_need(10, 9, window, cx);
            assert!(
                table.loading_more,
                "in-flight lock must block duplicate dispatch"
            );
        });
    });
    // The spawned task completes: exactly one dispatch ran and the lock
    // releases (a second spawn would have made the count two).
    cx.run_until_parked();
    let (calls, locked) =
        table.read_with(cx, |t, _| (t.delegate().load_more_calls, t.loading_more));
    assert_eq!(calls, 1, "the in-flight spawn must run exactly once");
    assert!(!locked, "lock must release after the task completes");
}

#[gpui::test]
async fn load_more_respects_threshold_and_eof(cx: &mut TestAppContext) {
    let table = cx.new(|cx| TableState::new(TestDelegate::standard(100), cx));
    with_window(cx, |window, cx| {
        table.update(cx, |table, cx| {
            // Visible end far from the bottom: no dispatch.
            table.load_more_if_need(100, 10, window, cx);
            assert_eq!(table.delegate().load_more_calls, 0);
            // Near the bottom: dispatch (delegate is_eof = true).
            table.load_more_if_need(100, 95, window, cx);
            assert!(table.loading_more, "eof=true dispatch must arm the lock");
            // Exhausted delegate: no second dispatch while in flight.
            table.delegate_mut().eof = false;
            table.load_more_if_need(100, 99, window, cx);
            assert!(table.loading_more, "eof=false must not add a second task");
        });
    });
    // Only the single eof=true dispatch may have run; the eof=false
    // re-trigger (and the in-flight gate) kept the count at one.
    cx.run_until_parked();
    table.update(cx, |table, _cx| {
        assert_eq!(table.delegate().load_more_calls, 1);
        assert!(!table.loading_more);
    });
}

/// Width clamping: below MIN stays unchanged, above MAX clamps, sub-1px
/// jitter is ignored (absorption §4.3-F).
/// P5 regression: a no-op `load_more` (static table, default `is_eof`) must
/// not queue a redraw. The task's unconditional notify used to drive a
/// dispatch → task → notify → redraw → dispatch loop that spun forever in
/// headless tests (and spam-redrew on real desktops).
#[gpui::test]
async fn noop_load_more_does_not_notify(cx: &mut TestAppContext) {
    let table = cx.new(|cx| TableState::new(TestDelegate::standard(3), cx));
    let notified = Rc::new(RefCell::new(0));
    let sink = notified.clone();
    cx.update(|cx| {
        cx.observe(&table, move |_, cx: &mut App| {
            *sink.borrow_mut() += 1;
            let _ = cx;
        })
        .detach();
    });
    with_window(cx, |window, cx| {
        table.update(cx, |table, cx| {
            table.load_more_if_need(3, 2, window, cx);
            assert!(table.loading_more, "dispatch must arm the in-flight lock");
        });
    });
    cx.run_until_parked();
    assert_eq!(*notified.borrow(), 0, "no-op load_more must not notify");
    let calls = table.read_with(cx, |t, _| t.delegate().load_more_calls);
    assert_eq!(calls, 1, "the single dispatch must still run");
}

/// The infinite-scroll contract: a `load_more` that grows the rows does
/// notify, so the new rows are rendered.
#[gpui::test]
async fn growing_load_more_notifies(cx: &mut TestAppContext) {
    let mut delegate = TestDelegate::standard(3);
    delegate.grow_on_load = true;
    let table = cx.new(|cx| TableState::new(delegate, cx));
    let notified = Rc::new(RefCell::new(0));
    let sink = notified.clone();
    cx.update(|cx| {
        cx.observe(&table, move |_, cx: &mut App| {
            *sink.borrow_mut() += 1;
            let _ = cx;
        })
        .detach();
    });
    with_window(cx, |window, cx| {
        table.update(cx, |table, cx| {
            table.load_more_if_need(3, 2, window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(*notified.borrow(), 1, "growing load_more must notify once");
    let rows = table.read_with(cx, |t, _| t.delegate().rows);
    assert_eq!(rows, 4);
}

#[gpui::test]
async fn column_resize_clamps_width(cx: &mut TestAppContext) {
    let table = setup(cx);
    table.update(cx, |table, cx| {
        // Below the 10px minimum: rejected.
        table.resize_cols(0, px(3.0), cx);
        assert_eq!(table.col_groups[0].width, px(100.0));
        // Normal resize applies.
        table.resize_cols(0, px(150.0), cx);
        assert_eq!(table.col_groups[0].width, px(150.0));
        // Above the 1200px maximum: clamped.
        table.resize_cols(0, px(5000.0), cx);
        assert_eq!(table.col_groups[0].width, px(1200.0));
        // Sub-1px change: ignored.
        table.resize_cols(0, px(1200.4), cx);
        assert_eq!(table.col_groups[0].width, px(1200.0));
        // Non-resizable columns never change (the runtime column snapshot
        // is the resize source of truth; "name" starts at its configured
        // 200px width).
        table.col_groups[1].column.resizable = false;
        table.resize_cols(1, px(42.0), cx);
        assert_eq!(table.col_groups[1].width, px(200.0));
    });
}

#[test]
fn column_resize_anchors_the_first_pointer_sample() {
    let mut anchor = None;

    // The first motion establishes the drag origin and therefore keeps the
    // width at the captured start value.
    assert_eq!(
        width_from_drag(px(100.0), &mut anchor, px(300.0)),
        px(100.0)
    );
    assert_eq!(anchor, Some(px(300.0)));

    // Later motion is a stable delta from that origin, not a delta from the
    // last rendered column bounds.
    assert_eq!(
        width_from_drag(px(100.0), &mut anchor, px(365.0)),
        px(165.0)
    );
}

#[gpui::test]
async fn move_column_reorders_and_notifies_delegate(cx: &mut TestAppContext) {
    let table = setup(cx);
    table.update(cx, |table, cx| {
        table.move_column(0, 2, cx);
        assert_eq!(table.col_groups[2].column.key, "pid");
        assert_eq!(table.col_groups[0].column.key, "name");
        assert_eq!(table.delegate().moves, vec![(0, 2)]);
    });
}

#[gpui::test]
async fn drag_payloads_are_typed(cx: &mut TestAppContext) {
    let table = setup(cx);
    let entity_id = table.entity_id();
    let col_ix = table.read_with(cx, |_, _| 0);
    let payload = DragColumn {
        entity_id,
        name: "PID".into(),
        width: px(80.0),
        col_ix,
    };
    assert_eq!(payload.col_ix, 0);
    let resize = ColumnResizeDrag {
        owner: entity_id,
        column: col_ix,
        start_width: px(100.0),
    };
    assert_eq!(resize.owner, entity_id);
    assert_eq!(resize.column, 0);
    assert_eq!(resize.start_width, px(100.0));
}

/// Perf smoke: 10k rows render with virtualization — only the visible cells
/// are laid out, and the whole headless draw completes within a generous
/// budget.
#[gpui::test]
async fn table_virtualizes_10000_rows(cx: &mut TestAppContext) {
    struct Harness {
        state: Entity<TableState<TestDelegate>>,
    }
    impl Render for Harness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let palette = Theme::dark().palette();
            div().size_full().child(Table::new(&self.state, palette))
        }
    }

    let state = cx.new(|cx| TableState::new(TestDelegate::standard(10_000), cx));
    let window = cx.add_window(|_window, _cx| Harness {
        state: state.clone(),
    });
    let start = Instant::now();
    let _ = cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear());
    let elapsed = start.elapsed();

    // The headless test window is 1920x1080; at 28px rows only ~39 rows are
    // visible (4 columns each). Layout runs a couple of measurement passes,
    // so the real render path must stay far under 600 cells — never
    // anywhere near the 40k full render.
    let td_calls = state.read_with(cx, |s, _| s.delegate().td_calls);
    assert!(
        td_calls < 600,
        "virtualization leaked: {td_calls} cells rendered for 10k rows"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "10k-row draw took {elapsed:?}"
    );
}

struct TableHarness {
    state: Entity<TableState<TestDelegate>>,
}

impl Render for TableHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::dark().palette();
        div().size_full().child(Table::new(&self.state, palette))
    }
}

/// 附录 A-2 regression: after `refresh()` shrinks the column set, header
/// rendering must not panic on stale column indices — `render_th` falls
/// back to an empty cell instead of `expect()`ing (gc panicked on the
/// out-of-bounds column).
#[gpui::test]
async fn render_th_skips_stale_indices_after_refresh_shrinks(cx: &mut TestAppContext) {
    let state = cx.new(|cx| TableState::new(TestDelegate::standard(5), cx));
    let window = cx.add_window(|_window, _cx| TableHarness {
        state: state.clone(),
    });
    let draw = |cx: &mut TestAppContext| {
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
    };
    draw(cx);
    let cells_before = state.read_with(cx, |s, _| s.delegate().td_calls);

    // Refresh with a shrunk column set (4 -> 2): the runtime groups drop the
    // old columns.
    state.update(cx, |table, cx| {
        table.delegate_mut().columns.truncate(2);
        table.refresh(cx);
        assert_eq!(table.col_groups.len(), 2);
    });

    // A stale index (the old last column) must render an empty cell, never
    // panic (the A-2 `get()` guard).
    with_window(cx, |window, cx| {
        state.update(cx, |table, cx| {
            let cell = table.render_th(3, window, cx);
            let _ = cell;
        });
    });

    // The end-to-end draw after the shrink renders normally: no panic, and
    // only the two remaining columns produce cells.
    draw(cx);
    let cells_after = state.read_with(cx, |s, _| s.delegate().td_calls);
    let delta = cells_after - cells_before;
    assert!(
        delta < 200,
        "after the shrink only 2 columns x 5 rows may render ({delta} cells)"
    );
}

/// 附录 A-10 regression: releasing a column-resize drag outside the window
/// must still finish the resize — the paint-stage canvas registers a
/// window-level `MouseUp` listener that resets `resizing_col` and emits
/// `ColumnWidthsChanged` (gc could leave `resizing_col` stuck forever).
#[gpui::test]
async fn mouse_release_outside_window_finishes_column_resize(cx: &mut TestAppContext) {
    let state = cx.new(|cx| TableState::new(TestDelegate::standard(10), cx));
    let window = cx.add_window(|_window, _cx| TableHarness {
        state: state.clone(),
    });
    let widths_changed = Rc::new(RefCell::new(None::<Vec<gpui::Pixels>>));
    let sink = widths_changed.clone();
    cx.update(|cx| {
        cx.subscribe(&state, move |_, event: &TableEvent, _| {
            if let TableEvent::ColumnWidthsChanged(widths) = event {
                *sink.borrow_mut() = Some(widths.clone());
            }
        })
        .detach();
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    // Simulate an in-progress drag whose pointer has left the window.
    state.update(cx, |table, cx| {
        table.resizing_col = Some(0);
        table.resize_cols(0, px(150.0), cx);
        assert_eq!(table.col_groups[0].width, px(150.0));
    });

    // Release at a point beyond the headless 1920x1080 window bounds.
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_mouse_up(
        point(px(2500.0), px(1500.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    drop(vcx);

    state.update(cx, |table, _| {
        assert_eq!(
            table.resizing_col, None,
            "window-level MouseUp must reset the drag state"
        );
    });
    assert_eq!(
        *widths_changed.borrow(),
        Some(vec![px(150.0), px(200.0), px(100.0), px(100.0)]),
        "finishing the resize must emit all current column widths"
    );
}

/// Column-gutter lock: every cell (header, body, fixed columns — all render
/// through `render_cell`) carries the standard horizontal inner padding, so
/// table text never sits flush against the column edge or the table border
/// (Services/Users/Startup rendered edge-to-edge before this).
#[gpui::test]
async fn table_cells_carry_the_column_gutter_padding(cx: &mut TestAppContext) {
    use taskmanager_theme::tokens;
    cx.update(|cx| {
        let state = TableState::new(TestDelegate::standard(3), cx);
        assert!(!state.col_groups.is_empty());
        let expected = div()
            .pl(crate::theme_binding::definite_length(tokens::SPACE_8))
            .pr(crate::theme_binding::definite_length(tokens::SPACE_8))
            .style()
            .padding
            .clone();
        for col_ix in 0..state.col_groups.len() {
            let padding = state.render_cell(col_ix).style().padding.clone();
            assert_eq!(
                padding.left, expected.left,
                "column {col_ix} must keep the standard left gutter"
            );
            assert_eq!(
                padding.right, expected.right,
                "column {col_ix} must keep the standard right gutter"
            );
        }
    });
}

/// A one-column delegate whose row menu records item activations.
struct MenuDelegate {
    column: TableColumn,
    activations: Rc<RefCell<usize>>,
}

impl TableDelegate for MenuDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        1
    }

    fn rows_count(&self, _cx: &App) -> usize {
        20
    }

    fn column(&self, _col_ix: usize, _cx: &App) -> &TableColumn {
        &self.column
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        _col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div().child(format!("row {row_ix}"))
    }

    fn context_menu(
        &mut self,
        _row_ix: usize,
        _menu: PopupMenuState,
        cx: &mut Context<TableState<Self>>,
    ) -> PopupMenuState {
        let activations = self.activations.clone();
        let stop = self.activations.clone();
        PopupMenuState::new(
            vec![
                MenuEntry::Item(MenuItem::new("start", move |_, _| {
                    *activations.borrow_mut() += 1;
                })),
                MenuEntry::Item(MenuItem::new("stop", move |_, _| {
                    *stop.borrow_mut() += 10;
                })),
            ],
            cx,
        )
    }
}

struct MenuHarness {
    state: Entity<TableState<MenuDelegate>>,
}

impl Render for MenuHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::dark().palette();
        div().size_full().child(Table::new(&self.state, palette))
    }
}

/// End-to-end lock for the table context-menu host (the mount path used by
/// Services/Users/Startup): a right-click on a rendered row routes through
/// the delegate, the menu mounts as a self-rendering entity anchored at the
/// click position, item activation dispatches the delegate action, and the
/// host clears its open-menu field — a stale host would keep painting a
/// dead menu.
#[gpui::test]
async fn row_right_click_menu_mounts_dispatches_and_clears(cx: &mut TestAppContext) {
    let activations = Rc::new(RefCell::new(0usize));
    let state = cx.new(|cx| {
        TableState::new(
            MenuDelegate {
                column: TableColumn::new("name", "Name"),
                activations: activations.clone(),
            },
            cx,
        )
    });
    let window = cx.add_window(|_window, _cx| MenuHarness {
        state: state.clone(),
    });
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear());

    // Right-click a body row (below the header): the menu must mount,
    // anchored exactly at the click position (no entrance animation).
    let open_at = point(px(200.0), px(120.0));
    vcx.simulate_mouse_down(open_at, MouseButton::Right, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    let popup = vcx
        .debug_bounds("tm-popup")
        .expect("a row right-click must open the delegate menu");
    assert_eq!(popup.left(), px(200.0), "anchored at the click x");
    assert_eq!(popup.top(), px(120.0), "anchored at the click y");

    // Activate item 0 (body py SPACE_4 + half of the 26px row).
    let item_center = point(popup.left() + px(40.0), popup.top() + px(17.0));
    vcx.simulate_click(item_center, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    drop(vcx);

    assert_eq!(
        *activations.borrow(),
        1,
        "the item click must dispatch the delegate action"
    );
    state.read_with(cx, |table, _| {
        assert!(
            table.context_menu.is_none(),
            "activation dismissal must clear the host's open-menu field"
        );
    });

    // After dismissal the menu is gone: clicking the stale item position
    // must not dispatch again (debug_bounds is append-only across frames,
    // so re-clicking is the behavioral absence proof, same as the
    // ContextMenuExt host test).
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_click(item_center, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    drop(vcx);
    assert_eq!(
        *activations.borrow(),
        1,
        "a dismissed menu must not dispatch further actions"
    );
}
