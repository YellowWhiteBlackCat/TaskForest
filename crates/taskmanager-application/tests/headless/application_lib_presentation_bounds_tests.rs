use super::MAX_CONTAINER_ROWS;
use zhuge_ui_core::row_page::row_budget;

/// The product owns the cap; the shared row-page owner owns the split. This
/// proves the wiring: a container inventory is never silently truncated, and
/// the domain rollup stays whole behind the bounded presentation list.
#[test]
fn the_container_cap_bounds_presentation_and_reports_the_omission() {
    let empty = row_budget(0, MAX_CONTAINER_ROWS);
    assert_eq!((empty.shown(), empty.hidden()), (0, 0));
    assert!(!empty.truncated());
    assert!(!row_budget(MAX_CONTAINER_ROWS - 1, MAX_CONTAINER_ROWS).truncated());
    assert_eq!(
        row_budget(MAX_CONTAINER_ROWS, MAX_CONTAINER_ROWS).shown(),
        MAX_CONTAINER_ROWS
    );
    let cut = row_budget(MAX_CONTAINER_ROWS + 3, MAX_CONTAINER_ROWS);
    assert_eq!((cut.shown(), cut.hidden()), (MAX_CONTAINER_ROWS, 3));
    assert!(cut.truncated());
    assert_eq!(
        cut.total(),
        MAX_CONTAINER_ROWS + 3,
        "the budget still knows the complete domain rollup"
    );
}
