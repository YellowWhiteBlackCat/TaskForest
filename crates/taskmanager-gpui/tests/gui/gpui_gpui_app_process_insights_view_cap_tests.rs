use super::{MAX_INSIGHT_CARD_ROWS, capped_card_rows};

/// The bounded-materialization contract for the scrollable insight cards:
/// lists at or below the cap materialize fully; larger lists materialize
/// exactly the cap and report the remainder (rendered by
/// `elements::more_rows_hint`), never a silent truncation.
#[test]
fn capped_card_rows_bounds_the_window_and_reports_the_remainder() {
    assert_eq!(capped_card_rows(0), (0, 0));
    assert_eq!(capped_card_rows(7), (7, 0));
    assert_eq!(
        capped_card_rows(MAX_INSIGHT_CARD_ROWS),
        (MAX_INSIGHT_CARD_ROWS, 0)
    );
    // A browser-class process: hundreds of threads/fds beyond the cap.
    assert_eq!(
        capped_card_rows(MAX_INSIGHT_CARD_ROWS + 804),
        (MAX_INSIGHT_CARD_ROWS, 804)
    );
}
