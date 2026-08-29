//! Pure-logic tests for the public Processes-column metadata in
//! `gpui_app::processes_view`: `rows::columns` (canonical column order +
//! count), `rows::header_label` / `is_hideable` / `is_numeric`, plus the stable
//! element-id mapper `sort_id(SortCol)`.
//!
//! These ids double as `Hover::Static` discriminators and gpui `ElementId`s in the
//! render path, so pinning them guards against an accidental id rename silently
//! breaking click/hover wiring. The sort-direction and row-ordering logic behind
//! these columns is covered separately in `processes_view_test.rs`.

use std::collections::HashSet;

use taskmanager_gpui::gpui_app::processes_view::rows::{
    columns, header_label, is_hideable, is_numeric,
};
use taskmanager_gpui::gpui_app::processes_view::sort_id;
use taskmanager_shell::SortCol;

/// `columns()` is the canonical Win11-TM column order (Name → User → PID →
/// Threads → Start → Status → CPU → Memory → Swap → Disk read → Disk write → CPU time →
/// FDs → Nice) with no duplicates and no missing variants — the "Choose columns"
/// picker iterates this, so a missing/duplicate entry would break the picker.
#[test]
fn sortcol_all_has_fourteen_columns_in_canonical_order_no_dups() {
    let all = columns();
    assert_eq!(all.len(), 14, "expected exactly 14 sortable columns");
    // Canonical order.
    assert_eq!(
        all,
        &[
            SortCol::Name,
            SortCol::User,
            SortCol::Pid,
            SortCol::Threads,
            SortCol::StartTime,
            SortCol::State,
            SortCol::Cpu,
            SortCol::Memory,
            SortCol::Swap,
            SortCol::DiskRead,
            SortCol::DiskWrite,
            SortCol::CpuTime,
            SortCol::Fds,
            SortCol::Nice,
        ]
    );
    // No duplicates (defensive — the order check above implies this, but a future
    // array edit could re-introduce one).
    let mut seen = HashSet::new();
    for &c in all {
        assert!(seen.insert(c), "duplicate column {c:?} in columns()");
    }
}

/// `header_label` is the exact string rendered in the table header + picker row.
/// The labels are localized via `i18n::t`, so pin the language to English for a
/// deterministic canonical-label contract (the labels' EXISTENCE + canonical
/// mapping is the real assertion; wording is locale-dependent by design).
#[test]
fn sortcol_header_labels_match_header_render() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let cases = [
        (SortCol::Name, "Name"),
        (SortCol::User, "User"),
        (SortCol::Pid, "PID"),
        (SortCol::Threads, "Threads"),
        (SortCol::StartTime, "Start"),
        (SortCol::State, "Status"),
        (SortCol::Cpu, "CPU"),
        (SortCol::Memory, "Memory"),
        (SortCol::Swap, "Swap"),
        (SortCol::DiskRead, "Disk read"),
        (SortCol::DiskWrite, "Disk write"),
        (SortCol::CpuTime, "CPU time"),
        (SortCol::Fds, "FDs"),
        (SortCol::Nice, "Nice"),
    ];
    for (col, want) in cases {
        assert_eq!(
            header_label(col),
            want,
            "header label for {col:?} should be {want:?}"
        );
    }
}

/// `is_hideable`: only the identity column (Name) is NOT hideable — every other
/// column may be toggled off via the "Choose columns" picker. A regression here
/// would either let users hide Name (orphaned rows) or forbid hiding a real column.
#[test]
fn sortcol_only_name_is_not_hideable() {
    for &col in columns() {
        let expected = col != SortCol::Name;
        assert_eq!(
            is_hideable(col),
            expected,
            "is_hideable({col:?}) should be {expected}"
        );
    }
}

/// `is_numeric`: the numeric columns (right-aligned, monospace) are exactly
/// Pid / Threads / CPU / Memory / Swap / Disk read / Disk write / CPU time / FDs / Nice.
/// The text columns (Name / User / Start / Status) are left-aligned in the UI font.
#[test]
fn sortcol_numeric_columns_classified_correctly() {
    let numeric = [
        SortCol::Pid,
        SortCol::Threads,
        SortCol::Cpu,
        SortCol::Memory,
        SortCol::Swap,
        SortCol::DiskRead,
        SortCol::DiskWrite,
        SortCol::CpuTime,
        SortCol::Fds,
        SortCol::Nice,
    ];
    for &col in columns() {
        let expected = numeric.contains(&col);
        assert_eq!(
            is_numeric(col),
            expected,
            "is_numeric({col:?}) should be {expected}"
        );
    }
}

/// `sort_id` returns a distinct, stable, non-empty id per column — these ids are
/// used as `ElementId`s on the sort-header cells, so a collision would make gpui
/// merge two headers' interactive state.
#[test]
fn sort_id_distinct_nonempty_per_column() {
    let mut ids = HashSet::new();
    for &col in columns() {
        let id = sort_id(col);
        assert!(!id.is_empty(), "sort_id({col:?}) is empty");
        assert!(ids.insert(id), "duplicate sort_id {id:?} for {col:?}");
    }
}
