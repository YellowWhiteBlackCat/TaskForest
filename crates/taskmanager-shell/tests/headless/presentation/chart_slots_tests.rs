use super::*;

/// The right-anchored capacity window: the newest sample pins to the right
/// edge and every sample owns one fixed slot, so a growing window extends
/// right-to-left instead of stretching its first two samples across the frame.
#[test]
fn the_newest_sample_pins_right_and_the_window_grows_leftward() {
    let slots = ChartSlots::new(2, 5, 100.0);
    assert!((slots.x(1) - 100.0).abs() < 1e-4, "the newest pins right");
    assert!(
        (slots.x(0) - 75.0).abs() < 1e-4,
        "the older sample sits one fixed slot left: {}",
        slots.x(0)
    );
    let full = ChartSlots::new(5, 5, 100.0);
    for index in 0..5 {
        assert!(
            (full.x(index) - 100.0 * index as f32 / 4.0).abs() < 1e-4,
            "a settled window spreads evenly: index {index}"
        );
    }
    assert!(
        (ChartSlots::new(6, 2, 100.0).x(5) - 100.0).abs() < 1e-4,
        "a capacity below the sample count degrades to the full spread"
    );
}

/// One denominator drives both directions, so the drawn sample and the
/// resolved slot can never disagree.
#[test]
fn the_drawing_x_and_the_hover_resolution_share_one_denominator() {
    let slots = ChartSlots::new(10, 10, 100.0);
    assert_eq!(slots.denominator(), 9.0);
    for index in 0..10 {
        let drawn = slots.x(index);
        let probe = if drawn >= 100.0 { 100.0 - 1e-3 } else { drawn };
        assert_eq!(
            slots.index_at(probe),
            Some(index),
            "the pointer on slot {index} (x {drawn}) resolves to slot {index}"
        );
    }
}

/// Inside the frame the cursor clamps into the grid: the empty leading area of
/// a growing window is the oldest occupied slot, and the right edge is the
/// newest one.
#[test]
fn inside_the_frame_the_cursor_clamps_into_the_grid() {
    let warming = ChartSlots::new(3, 60, 600.0);
    assert_eq!(
        warming.index_at(0.0),
        Some(0),
        "before the curve is the oldest occupied slot"
    );
    assert_eq!(
        warming.index_at(600.0),
        Some(2),
        "the right edge is the newest slot"
    );
    assert_eq!(
        warming.index_at(900.0),
        Some(2),
        "past the right edge is still the newest slot"
    );
}

/// Outside the frame there is no slot: a cursor before the plot, a frame
/// without width, or a non-finite measurement resolves to nothing rather than
/// to a fabricated sample.
#[test]
fn outside_the_frame_there_is_no_slot() {
    let slots = ChartSlots::new(4, 8, 200.0);
    assert_eq!(slots.index_at(-0.5), None);
    assert_eq!(slots.index_at(f32::NAN), None);
    assert_eq!(slots.index_at(f32::INFINITY), None);
    assert_eq!(ChartSlots::new(4, 8, 0.0).index_at(50.0), None);
    assert_eq!(ChartSlots::new(4, 8, f32::NAN).index_at(50.0), None);
    assert_eq!(ChartSlots::new(0, 8, 200.0).index_at(50.0), None);
}

/// A window with a single sample has no line to draw, but the sample still
/// owns the right edge and stays hoverable — the same answer the release
/// surface has always given.
#[test]
fn a_single_sample_owns_the_right_edge_and_stays_hoverable() {
    let slots = ChartSlots::new(1, 8, 200.0);
    assert_eq!(slots.x(0), 200.0);
    assert_eq!(slots.index_at(50.0), Some(0));
    assert_eq!(slots.index_at(200.0), Some(0));
}

/// An empty window has no x at all, and a non-finite width never paints.
#[test]
fn an_empty_or_broken_window_has_no_x() {
    assert_eq!(ChartSlots::new(0, 8, 200.0).x(0), 0.0);
    assert_eq!(ChartSlots::new(3, 8, f32::NAN).x(0), 0.0);
    assert_eq!(ChartSlots::new(3, 8, -50.0).x(2), 0.0);
}
