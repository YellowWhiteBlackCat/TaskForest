use super::*;
use gpui::px;
use std::collections::{HashMap, HashSet};
use taskmanager_ui_contract::{PROCESS_COLUMNS, find};

/// Contract gate (defect #10): every `SortCol` variant must delegate its
/// default width, numeric alignment, hideability, and canonical position to
/// the neutral `PROCESS_COLUMNS` inventory — position for position, with no
/// orphan rows on either side.
///
/// Variant coverage is compiler-anchored: the exhaustive
/// [`SortCol::contract_id`] match makes adding a variant without a token a
/// build error, this test iterates [`columns()`] and asserts each
/// token resolves to exactly one contract row, and the position-wise id
/// equality plus length/distinctness checks make an orphan contract row
/// (or a variant missing from the contract) impossible to miss.
#[test]
fn sort_col_semantics_delegate_to_the_process_column_contract() {
    assert_eq!(
        columns().len(),
        PROCESS_COLUMNS.len(),
        "columns() and PROCESS_COLUMNS must describe the same columns"
    );
    let mut seen: HashSet<&str> = HashSet::new();
    for (position, &col) in columns().iter().enumerate() {
        let id = contract_id(col);
        assert_eq!(
            PROCESS_COLUMNS[position].id, id,
            "canonical order broke at position {position}"
        );
        assert!(seen.insert(id), "duplicate SortCol token {id}");
        let spec =
            find(id).unwrap_or_else(|| panic!("SortCol token {id} has no PROCESS_COLUMNS row"));
        assert_eq!(
            f32::from(default_width(col)),
            spec.default_width,
            "{id} default width must come from the contract"
        );
        assert_eq!(is_numeric(col), spec.numeric, "{id} numeric flag");
        assert_eq!(is_hideable(col), spec.hideable, "{id} hideable flag");
        assert_eq!(
            column_index(col),
            position,
            "{id} index must match the contract canonical order"
        );
    }
    assert_eq!(
        seen.len(),
        PROCESS_COLUMNS.len(),
        "PROCESS_COLUMNS carries orphan rows no SortCol maps to"
    );
}

#[test]
fn process_column_band_keeps_identity_and_active_column_visible() {
    let hidden = HashSet::new();
    let band = process_column_band(&hidden, SortCol::Cpu, px(600.0), &HashMap::new());
    assert_eq!(band.first(), Some(&SortCol::Name));
    assert!(band.contains(&SortCol::Cpu));
    assert!(
        band.len() < columns().len(),
        "a compact viewport must not materialize every process column"
    );
}

#[test]
fn process_column_step_skips_hidden_columns_without_wrapping() {
    let hidden = HashSet::from([SortCol::User, SortCol::Threads]);
    assert_eq!(
        process_column_step(SortCol::Name, true, &hidden),
        SortCol::Pid
    );
    assert_eq!(
        process_column_step(SortCol::Pid, false, &hidden),
        SortCol::Name
    );
    assert_eq!(
        process_column_step(SortCol::Nice, true, &hidden),
        SortCol::Nice
    );
}
