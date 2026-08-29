use super::*;

#[test]
fn cell_width_counts_wide_and_combining_graphemes() {
    assert_eq!(cell_width("CPU"), 3);
    assert_eq!(cell_width("中文"), 4);
    assert_eq!(cell_width("e\u{301}"), 1);
}

#[test]
fn head_truncation_preserves_graphemes_and_cell_bound() {
    let value = "中文e\u{301}tail";
    let truncated = truncate_cells(value, 6);
    assert_eq!(cell_width(&truncated), 6);
    assert!(truncated.ends_with('…'));
    assert!(!truncated.contains("e\u{301}tail"));
}

#[test]
fn tail_truncation_keeps_the_actionable_suffix() {
    let truncated = truncate_tail_cells("prefix 中文 error", 8);
    assert!(cell_width(&truncated) <= 8);
    assert!(truncated.ends_with("error"));
    assert!(truncated.starts_with('…'));
}

#[test]
fn padding_targets_cells_not_scalar_count() {
    let padded = pad_cells("中文", 6);
    assert_eq!(cell_width(&padded), 6);
    assert!(padded.ends_with("  "));
}
