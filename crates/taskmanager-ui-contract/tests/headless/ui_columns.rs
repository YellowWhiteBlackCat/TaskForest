use super::*;
use std::collections::HashSet;

#[test]
fn the_inventory_covers_exactly_fourteen_columns() {
    assert_eq!(PROCESS_COLUMNS.len(), 14);
    assert_eq!(
        PROCESS_COLUMNS.first().map(|spec| spec.id),
        Some("Name"),
        "Name stays the leading identity column"
    );
    assert_eq!(
        PROCESS_COLUMNS.last().map(|spec| spec.id),
        Some("Nice"),
        "Nice stays the trailing column"
    );
}

#[test]
fn ids_are_distinct_and_lookup_is_exact() {
    let mut seen = HashSet::new();
    for spec in PROCESS_COLUMNS {
        assert!(seen.insert(spec.id), "duplicate column id {}", spec.id);
        assert_eq!(
            find(spec.id),
            Some(spec),
            "lookup must be exact for {}",
            spec.id
        );
    }
    assert_eq!(seen.len(), PROCESS_COLUMNS.len());
    assert_eq!(
        find("not-a-process-column"),
        None,
        "unknown tokens must not resolve"
    );
}

#[test]
fn name_is_not_hideable_and_every_width_is_positive() {
    let name = find("Name").expect("Name column exists");
    assert!(!name.hideable, "the identity column must never be hideable");
    for spec in PROCESS_COLUMNS {
        assert!(
            spec.default_width > 0.0,
            "{} must have a positive default width",
            spec.id
        );
        if spec.id != "Name" {
            assert!(spec.hideable, "{} must stay hideable", spec.id);
        }
    }
}
