//! test-intent: behavior
//!
//! Pure-core tests for the table projection: measured viewport capacity, the
//! hidden-column projection's identity invariant, and the ui-contract wiring.
//! Row-window edge and clamping contracts belong to `zhuge-ui-core::row_page`
//! and are proven there; this crate proves the wiring that consumes them.
//!
//! The oracle is the contract table itself (`taskmanager_ui_contract`) — no
//! bevy types, no world.

use super::{SortProjection, header_label, rows_in_viewport, sorted_direction, visible_columns};
use taskmanager_ui_contract::PROCESS_COLUMNS;

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
    let columns = visible_columns(&[]);
    assert_eq!(columns.len(), 16, "the contract's canonical column count");
    assert_eq!(columns[0].id, "Name", "the identity column leads");
    // Spot-prove the wiring is the live contract table, not a copy: the
    // numeric resource columns carry their contract widths.
    for (id, width) in [
        ("CPU", 70.0),
        ("Memory", 100.0),
        ("Swap", 100.0),
        ("MemoryPss", 100.0),
        ("Network", 90.0),
    ] {
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
    assert_eq!(visible.len(), 14);
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
    assert_eq!(
        unchanged.len(),
        PROCESS_COLUMNS.len(),
        "an empty hidden set preserves the complete contract column set"
    );
}

#[test]
fn the_sort_indicator_rests_on_exactly_one_column_with_a_typed_direction() {
    let columns = visible_columns(&[]);
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

#[test]
fn table_column_width_distribution_balances_surplus_to_name() {
    let columns = visible_columns(&[]);
    let total_default: f32 = columns.iter().map(|c| c.default_width).sum();
    let surplus = 200.0;
    let available = total_default + surplus;
    let distributed = super::distribute_column_widths(&columns, Some(available));
    assert_eq!(distributed.len(), columns.len());
    let name_idx = columns.iter().position(|c| c.id == "Name").unwrap();
    assert_eq!(
        distributed[name_idx],
        columns[name_idx].default_width + surplus
    );
    for (i, spec) in columns.iter().enumerate() {
        if i != name_idx {
            assert_eq!(distributed[i], spec.default_width);
        }
    }
}

#[test]
fn table_column_width_distribution_shrinks_gracefully_when_constrained() {
    let columns = visible_columns(&[]);
    let distributed = super::distribute_column_widths(&columns, Some(800.0));
    assert_eq!(distributed.len(), columns.len());
    let total_dist: f32 = distributed.iter().sum();
    assert!((total_dist - 800.0).abs() < 1.0);
    for w in &distributed {
        assert!(*w >= super::LAYOUT_MIN_COLUMN_WIDTH_PX);
    }
}

#[test]
fn table_column_width_distribution_respects_overrides() {
    let columns = visible_columns(&[]);
    let mut overrides = std::collections::HashMap::new();
    overrides.insert("CPU", 140.0);
    overrides.insert("Memory", 180.0);
    let distributed =
        super::distribute_column_widths_with_overrides(&columns, Some(&overrides), None);
    let cpu_idx = columns.iter().position(|c| c.id == "CPU").unwrap();
    let mem_idx = columns.iter().position(|c| c.id == "Memory").unwrap();
    assert_eq!(distributed[cpu_idx], 140.0);
    assert_eq!(distributed[mem_idx], 180.0);
}
