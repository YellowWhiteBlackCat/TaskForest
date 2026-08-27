use super::VirtualWindow;

#[test]
fn initial_window_is_bounded_for_a_ten_thousand_row_table() {
    let window = VirtualWindow::for_rows(10_000, 0.0, 320.0, 24.0, 32.0);

    assert_eq!(window.start, 0);
    assert!(window.end < 10_000);
    assert!(window.materialized_len() <= 40);
    assert_eq!(window.top, 0.0);
    assert_eq!(window.bottom, (10_000 - window.end) as f32 * 24.0);
}

#[test]
fn scrolling_moves_the_window_and_preserves_total_content_height() {
    let first = VirtualWindow::for_rows(1_000, 0.0, 240.0, 30.0, 30.0);
    let middle = VirtualWindow::for_rows(1_000, 4_530.0, 240.0, 30.0, 30.0);

    assert!(middle.start > first.start);
    assert!(middle.end > middle.start);
    assert!(middle.top > first.top);
    assert!(middle.bottom < first.bottom);
    assert_eq!(middle.key(), (middle.start, middle.end));
}

#[test]
fn stale_offset_is_clamped_after_a_filter_shrinks_the_list() {
    let window = VirtualWindow::for_rows(8, f32::MAX, 200.0, 24.0, 32.0);

    assert_eq!(window.end, 8);
    assert_eq!(window.bottom, 0.0);
    assert!(window.start < window.end);
}

#[test]
fn prefix_height_does_not_make_the_first_window_unbounded() {
    let window = VirtualWindow::for_rows(10_000, 0.0, 480.0, 32.0, 48.0);

    assert_eq!(window.start, 0);
    assert!(window.materialized_len() <= 40);
}

#[test]
fn empty_and_invalid_inputs_degrade_to_a_safe_empty_or_single_extent() {
    let empty = VirtualWindow::for_rows(0, 0.0, 0.0, 0.0, f32::NAN);
    let invalid = VirtualWindow::for_rows(2, f32::NAN, f32::NAN, f32::NAN, -4.0);

    assert_eq!(empty.materialized_len(), 0);
    assert_eq!(invalid.start, 0);
    assert!(invalid.end > invalid.start);
    assert!(invalid.top.is_finite() && invalid.bottom.is_finite());
}

#[test]
fn generic_body_factory_receives_only_the_materialized_range() {
    use std::cell::Cell;
    use std::rc::Rc;

    let window = VirtualWindow::for_rows(10_000, 2_400.0, 240.0, 30.0, 32.0);
    let seen = Rc::new(Cell::new((usize::MAX, usize::MAX)));
    let writer = Rc::clone(&seen);
    let _body = super::virtual_table_body(window, iced::Length::Fill, move |start, end| {
        writer.set((start, end));
        (start..end)
            .map(|_| iced::widget::text("").into())
            .collect()
    });

    assert_eq!(seen.get(), (window.start, window.end));
    assert!(window.end - window.start < 10_000);
}

impl VirtualWindow {
    /// Number of row widgets that this window asks the renderer to build.
    #[must_use]
    pub(crate) const fn materialized_len(self) -> usize {
        self.end.saturating_sub(self.start)
    }
}
