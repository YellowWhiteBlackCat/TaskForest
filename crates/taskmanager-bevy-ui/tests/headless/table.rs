//! test-intent: behavior
//!
//! Pure-core tests for the table projection: virtual window math (edge and
//! clamping contracts the M1 scroller depends on), viewport capacity, the
//! hidden-column projection's identity invariant, and the ui-contract wiring.
//!
//! The oracle is the contract table itself (`taskmanager_ui_contract`) plus
//! hand-computed window arithmetic — no bevy types, no world.

use super::{
    RowWindow, SortProjection, header_label, process_columns, row_window, rows_in_viewport,
    sorted_direction, visible_columns,
};

#[test]
fn empty_table_or_viewport_renders_an_empty_window() {
    assert_eq!(row_window(0, 10, 0), RowWindow::default());
    assert_eq!(row_window(5, 0, 3), RowWindow::default());
    assert!(row_window(0, 10, 0).is_empty());
}

#[test]
fn window_covers_the_viewport_from_the_requested_offset() {
    assert_eq!(row_window(100, 10, 0).len(), 10);
    assert_eq!(row_window(100, 10, 0).first, 0);
    assert_eq!(
        row_window(100, 10, 42),
        RowWindow {
            first: 42,
            last: 52
        }
    );
}

#[test]
fn scroll_clamps_to_the_last_full_page_never_an_empty_tail() {
    // Asking past the end pins the window so the last rows stay visible.
    assert_eq!(
        row_window(100, 10, 999),
        RowWindow {
            first: 90,
            last: 100
        }
    );
    assert_eq!(
        row_window(100, 10, 90),
        RowWindow {
            first: 90,
            last: 100
        }
    );
    assert_eq!(
        row_window(7, 3, 6),
        RowWindow { first: 4, last: 7 },
        "a short table pins to its own tail"
    );
}

#[test]
fn viewport_larger_than_total_shows_everything_and_ignores_scroll() {
    assert_eq!(row_window(7, 50, 0), RowWindow { first: 0, last: 7 });
    assert_eq!(row_window(7, 50, 30), RowWindow { first: 0, last: 7 });
}

#[test]
fn viewport_capacity_floors_and_guards_degenerate_heights() {
    assert_eq!(rows_in_viewport(800.0, 32.0), 25);
    assert_eq!(rows_in_viewport(99.0, 33.0), 3);
    assert_eq!(rows_in_viewport(0.0, 32.0), 0);
    assert_eq!(rows_in_viewport(800.0, 0.0), 0);
    assert_eq!(rows_in_viewport(800.0, -4.0), 0);
}

#[test]
fn column_vocabulary_is_the_contract_single_source() {
    let columns = process_columns();
    assert_eq!(columns.len(), 14, "the contract's canonical column count");
    assert_eq!(columns[0].id, "Name", "the identity column leads");
    // Spot-prove the wiring is the live contract table, not a copy: the
    // numeric resource columns carry their contract widths.
    for (id, width) in [("CPU", 70.0), ("Memory", 100.0), ("Swap", 100.0)] {
        let spec = columns
            .iter()
            .find(|spec| spec.id == id)
            .unwrap_or_else(|| panic!("contract column {id} missing"));
        assert_eq!(spec.default_width, width, "contract width for {id}");
        assert!(spec.numeric, "{id} is numeric in the contract");
    }
}

#[test]
fn hidden_columns_drop_but_the_identity_column_stays() {
    let visible = visible_columns(&["CPU", "Memory"]);
    assert_eq!(visible.len(), 12);
    assert!(
        visible
            .iter()
            .all(|spec| spec.id != "CPU" && spec.id != "Memory"),
        "hidden columns are gone"
    );
    assert!(
        visible.iter().any(|spec| spec.id == "Name"),
        "hiding the identity column is refused (it is not hideable)"
    );
    let unchanged = visible_columns(&[]);
    assert_eq!(unchanged.len(), process_columns().len());
}

#[test]
fn the_sort_indicator_rests_on_exactly_one_column_with_a_typed_direction() {
    let columns = process_columns();
    let cpu = columns.iter().find(|spec| spec.id == "CPU").expect("CPU");
    let name = columns.iter().find(|spec| spec.id == "Name").expect("Name");
    let ascending = SortProjection {
        column: "CPU",
        descending: false,
    };
    // Labels stay the pure column word — sort state is never spliced into
    // identity text (the old "CPU ▲" spelling shipped tofu glyphs).
    assert_eq!(header_label(cpu), "CPU");
    assert_eq!(header_label(name), "Name");
    // The sorted column answers with its direction; every other column
    // answers "no indicator".
    assert_eq!(sorted_direction(cpu, Some(ascending)), Some(false));
    assert_eq!(sorted_direction(name, Some(ascending)), None);
    let descending = SortProjection {
        column: "CPU",
        descending: true,
    };
    assert_eq!(sorted_direction(cpu, Some(descending)), Some(true));
    assert_eq!(sorted_direction(cpu, None), None);
}
