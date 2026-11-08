//! test-intent: behavior
//! The shared graph data-points window: one clamp (two samples make a shape,
//! the window never exceeds the bounded history ring) and one trailing slice
//! ("the newest N samples").

use super::*;

#[test]
fn the_window_clamps_to_two_samples_and_the_bounded_ring() {
    assert_eq!(graph_window_samples(0), MIN_GRAPH_WINDOW_SAMPLES);
    assert_eq!(graph_window_samples(1), MIN_GRAPH_WINDOW_SAMPLES);
    assert_eq!(graph_window_samples(2), 2);
    assert_eq!(graph_window_samples(60), 60);
    assert_eq!(
        graph_window_samples(usize::MAX),
        MAX_GRAPH_WINDOW_SAMPLES,
        "the render window never exceeds the bounded history ring"
    );
    assert_eq!(MAX_GRAPH_WINDOW_SAMPLES, 600);
}

#[test]
fn latest_samples_keeps_the_newest_capacity_samples() {
    let samples = [0.0, 1.0, 2.0, 3.0, 4.0];
    assert_eq!(latest_samples(&samples, 2), &[3.0, 4.0]);
    assert_eq!(latest_samples(&samples, 5), &[0.0, 1.0, 2.0, 3.0, 4.0]);
    // A window longer than the series keeps the whole series (no padding,
    // no fabricated leading gap).
    assert_eq!(latest_samples(&samples, 9), &[0.0, 1.0, 2.0, 3.0, 4.0]);
    assert_eq!(latest_samples(&[], 4), &[] as &[f32]);
}
