use super::StatRow;

/// Both variants expose the same label accessor.
#[test]
fn label_accessor_covers_all_variants() {
    assert_eq!(StatRow::text("Read", None).label(), "Read");
    assert_eq!(StatRow::pair("Swap", None).label(), "Swap");
    assert_eq!(
        StatRow::trend("Trend", "1", "2", "3", "1 · 2 · 3").label(),
        "Trend"
    );
}

/// A present value reads back exactly once, for all variants.
#[test]
fn value_accessor_returns_present_values() {
    let row = StatRow::text("Read", Some("12.3 MiB/s".to_string()));
    assert_eq!(row.value(), Some("12.3 MiB/s"));
    let row = StatRow::pair("VRAM", Some("1.0 / 4.0 GiB".to_string()));
    assert_eq!(row.value(), Some("1.0 / 4.0 GiB"));
    let row = StatRow::trend("Trend", "10", "15", "20", "10 · 15 · 20");
    assert_eq!(row.value(), Some("10 · 15 · 20"));
    assert_eq!(row.trend_parts(), Some(("10", "15", "20")));
}

/// `None` keeps the row: label still present, value absent — the
/// renderer's dash case, never a producer-side row omission.
#[test]
fn none_value_keeps_the_row_for_the_renderer_dash() {
    for row in [StatRow::text("Write", None), StatRow::pair("Memory", None)] {
        assert!(!row.label().is_empty());
        assert_eq!(row.value(), None);
    }
}

/// The variant is part of identity: a Text and a Pair with identical
/// label/value are distinct rows, and rows clone for snapshot reuse.
#[test]
fn variant_types_identity_and_rows_clone() {
    let row = StatRow::text("Status", Some("Healthy".to_string()));
    assert_eq!(
        row.clone(),
        StatRow::text("Status", Some("Healthy".to_string()))
    );
    assert_ne!(row, StatRow::pair("Status", Some("Healthy".to_string())));
}
