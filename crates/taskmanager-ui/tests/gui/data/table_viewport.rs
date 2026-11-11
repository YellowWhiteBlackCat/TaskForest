//! Whole-row viewport sizing and bounds for the shared table component.

use gpui::{
    App, AppContext, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    Stateful, Styled, TestAppContext, VisualTestContext, Window, div, px, size,
};

use taskmanager_theme::Theme;

use super::{Table, TableColumn, TableDelegate, TableState, whole_row_viewport_height};
use crate::primitives::scrollbar::SCROLLBAR_HEIGHT;

struct ViewportDelegate {
    column: TableColumn,
    row_count: usize,
}

impl TableDelegate for ViewportDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        1
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.row_count
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

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        div()
            .id(("table-viewport-test-row", row_ix))
            .debug_selector(move || format!("tm-table-viewport-test-row:{row_ix}"))
    }
}

struct TableWithSiblingScrollbar {
    state: Entity<TableState<ViewportDelegate>>,
}

impl Render for TableWithSiblingScrollbar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex_col()
            .child(div().w_full().h(px(267.0)).flex_shrink_0().child(
                Table::new(&self.state, Theme::dark().palette()).scrollbar_visible(true, false),
            ))
            .child(
                div()
                    .debug_selector(|| "tm-table-viewport-test-scrollbar-band".into())
                    .w_full()
                    .h(px(SCROLLBAR_HEIGHT))
                    .flex_shrink_0(),
            )
    }
}

#[test]
fn whole_row_viewport_rounds_down_without_exceeding_available_height() {
    for row_height in [px(20.0), px(28.0), px(31.0)] {
        let row_extent = f32::from(row_height);
        for tenths in 0..=280 {
            let available = px(tenths as f32 / 10.0);
            let viewport = whole_row_viewport_height(available, row_height);
            let height = f32::from(viewport);
            let remainder = f32::from(available) - height;

            assert!(viewport <= available, "{viewport:?} exceeds {available:?}");
            assert!(
                (height / row_extent - (height / row_extent).round()).abs() < 0.001,
                "{height} must be a whole number of {row_extent}px rows"
            );
            assert!(
                (-0.001..row_extent).contains(&remainder),
                "at most one partial row may remain: {available:?} -> {viewport:?}"
            );
        }
    }

    assert_eq!(whole_row_viewport_height(px(100.0), px(28.0)), px(84.0));
}

#[gpui::test]
async fn compact_table_admits_whole_rows_and_keeps_the_final_row_reachable(
    cx: &mut TestAppContext,
) {
    let state = cx.new(|_cx| {
        TableState::new(
            ViewportDelegate {
                column: TableColumn::new("row", "Row"),
                row_count: 50,
            },
            _cx,
        )
    });
    let window = cx.add_window(|_window, _cx| TableWithSiblingScrollbar {
        state: state.clone(),
    });
    cx.simulate_window_resize(window.into(), size(px(720.0), px(480.0)));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .expect("compact table should render");

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let table = visual.debug_bounds("tm-table").expect("table bounds");
    let rows = visual
        .debug_bounds("tm-table-rows-viewport")
        .expect("whole-row viewport bounds");
    let scrollbar = visual
        .debug_bounds("tm-table-viewport-test-scrollbar-band")
        .expect("sibling scrollbar band");
    let row_height = state.read_with(&visual, |table, _cx| table.options.row_height);
    let available_height = table.bottom() - rows.origin.y;
    let expected_height = whole_row_viewport_height(available_height, row_height);
    let row_count = (f32::from(rows.size.height) / f32::from(row_height)).round() as usize;
    let last_visible_row = visual
        .debug_bounds("tm-table-viewport-test-row:7")
        .expect("the eighth row should fit in the measured table slot");

    assert_eq!(table.size.height, px(267.0));
    assert_eq!(row_count, 8);
    assert_eq!(
        rows.size.height, expected_height,
        "table={table:?}, rows={rows:?}, row_height={row_height:?}, available={available_height:?}"
    );
    assert!(
        last_visible_row.bottom() <= rows.bottom() + px(0.5),
        "the last visible row must fit inside the row viewport: row={last_visible_row:?}, rows={rows:?}"
    );
    assert!(
        rows.bottom() <= scrollbar.origin.y + px(0.5),
        "the row viewport must end above the sibling scrollbar: rows={rows:?}, scrollbar={scrollbar:?}"
    );
    assert!(
        visual
            .debug_bounds("tm-table-viewport-test-row:8")
            .is_none(),
        "the row after the admitted range must not be partially rendered"
    );

    visual.update(|_window, cx| {
        state.update(cx, |table, cx| table.set_selected_row(49, cx));
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let rows = visual
        .debug_bounds("tm-table-rows-viewport")
        .expect("whole-row viewport remains mounted after scrolling");
    let final_data_row = visual
        .debug_bounds("tm-table-viewport-test-row:49")
        .expect("scrolling to the final row must keep it reachable");
    assert!(
        final_data_row.origin.y >= rows.origin.y - px(0.5)
            && final_data_row.bottom() <= rows.bottom() + px(0.5),
        "the final row must scroll fully into the snapped viewport: row={final_data_row:?}, rows={rows:?}"
    );
}
