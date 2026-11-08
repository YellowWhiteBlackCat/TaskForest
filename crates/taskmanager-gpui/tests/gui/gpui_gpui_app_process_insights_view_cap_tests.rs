use super::MAX_INSIGHT_CARD_ROWS;
use zhuge_ui_core::row_page::row_budget;

/// The bounded-materialization contract for the scrollable insight cards:
/// lists at or below the cap materialize fully; larger lists materialize
/// exactly the cap and report the remainder (rendered by
/// `elements::more_rows_hint`), never a silent truncation. The split itself is
/// owned by `zhuge-ui-core::row_page`; this proves the card cap is wired to it.
#[test]
fn capped_card_rows_bounds_the_window_and_reports_the_remainder() {
    let empty = row_budget(0, MAX_INSIGHT_CARD_ROWS);
    assert_eq!((empty.shown(), empty.hidden()), (0, 0));
    let small = row_budget(7, MAX_INSIGHT_CARD_ROWS);
    assert_eq!((small.shown(), small.hidden()), (7, 0));
    assert!(!small.truncated());
    let exact = row_budget(MAX_INSIGHT_CARD_ROWS, MAX_INSIGHT_CARD_ROWS);
    assert_eq!((exact.shown(), exact.hidden()), (MAX_INSIGHT_CARD_ROWS, 0));
    // A browser-class process: hundreds of threads/fds beyond the cap.
    let cut = row_budget(MAX_INSIGHT_CARD_ROWS + 804, MAX_INSIGHT_CARD_ROWS);
    assert_eq!((cut.shown(), cut.hidden()), (MAX_INSIGHT_CARD_ROWS, 804));
    assert!(cut.truncated());
    assert_eq!(cut.total(), MAX_INSIGHT_CARD_ROWS + 804);
}
