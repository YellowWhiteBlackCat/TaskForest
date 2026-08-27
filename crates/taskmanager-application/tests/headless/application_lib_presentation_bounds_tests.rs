use super::{MAX_CONTAINER_ROWS, container_row_window};

#[test]
fn container_row_window_keeps_the_domain_complete_and_presentation_bounded() {
    assert_eq!(container_row_window(0), (0, 0));
    assert_eq!(
        container_row_window(MAX_CONTAINER_ROWS - 1),
        (MAX_CONTAINER_ROWS - 1, 0)
    );
    assert_eq!(
        container_row_window(MAX_CONTAINER_ROWS),
        (MAX_CONTAINER_ROWS, 0)
    );
    assert_eq!(
        container_row_window(MAX_CONTAINER_ROWS + 3),
        (MAX_CONTAINER_ROWS, 3)
    );
}
