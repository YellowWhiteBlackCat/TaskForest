use super::*;
use crate::widgets::chart::geometry::{ChartScale, ChartVertex, chart_runs};

#[test]
fn chart_projection_maps_extrema_and_keeps_newest_at_right_edge() {
    let segments = line_segments(&[1.0, 3.0, 2.0], 100.0, 40.0, MAX_CHART_POINTS);
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].start, ChartVertex { x: 0.0, y: 40.0 });
    assert_eq!(segments[0].end, ChartVertex { x: 50.0, y: 0.0 });
    assert_eq!(segments[1].end, ChartVertex { x: 100.0, y: 20.0 });
}

#[test]
fn unavailable_samples_break_the_line_instead_of_becoming_zero() {
    let segments = line_segments(&[1.0, f32::NAN, 3.0, 4.0], 90.0, 30.0, MAX_CHART_POINTS);
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].start.x, 60.0);
    assert!((segments[0].start.y - 10.0).abs() < 1e-4);
    assert_eq!(segments[0].end, ChartVertex { x: 90.0, y: 0.0 });
}

#[test]
fn constant_and_all_unavailable_windows_stay_honest_and_bounded() {
    let constant = line_segments(&[2.0, 2.0, 2.0], 60.0, 20.0, MAX_CHART_POINTS);
    assert!(
        constant
            .iter()
            .all(|segment| { segment.start.y == 10.0 && segment.end.y == 10.0 })
    );
    assert!(line_segments(&[f32::NAN, f32::INFINITY], 60.0, 20.0, MAX_CHART_POINTS).is_empty());

    let samples = (0..(MAX_CHART_POINTS + 17))
        .map(|value| value as f32)
        .collect::<Vec<_>>();
    let bounded = line_segments(&samples, 60.0, 20.0, MAX_CHART_POINTS);
    assert_eq!(bounded.len(), MAX_CHART_POINTS - 1);
    assert_eq!(bounded[0].start.x, 0.0);
    assert_eq!(bounded[0].start.y, 20.0);
}

#[test]
fn zero_budget_does_not_accidentally_render_a_sample() {
    assert!(line_segments(&[1.0], 10.0, 10.0, 0).is_empty());
}

// ---- the curve's own geometry: one continuous path, not a stack of quads --

#[test]
fn a_gap_free_window_is_one_continuous_path() {
    // The projection's truth is the RUN: one unbroken path per measured
    // stretch. The segment list is derived from it, so a renderer can never
    // mistake one rectangle per segment for the curve.
    let runs = chart_runs(
        &[1.0, 3.0, 2.0, 5.0],
        100.0,
        40.0,
        MAX_CHART_POINTS,
        ChartScale::Window,
    );
    assert_eq!(runs.len(), 1, "a gap-free window is one path");
    assert_eq!(runs[0].vertices.len(), 4);
    assert_eq!(runs[0].vertices[0], ChartVertex { x: 0.0, y: 40.0 });
    assert_eq!(runs[0].vertices[3], ChartVertex { x: 100.0, y: 0.0 });
}

#[test]
fn a_missing_observation_breaks_the_path_instead_of_joining_across_it() {
    let runs = chart_runs(
        &[1.0, f32::NAN, 3.0, 4.0],
        90.0,
        30.0,
        MAX_CHART_POINTS,
        ChartScale::Window,
    );
    // The lone sample before the gap cannot form a path of its own: the
    // measured stretch after the gap is the one continuous path.
    assert_eq!(runs.len(), 1, "the gap opens the next path");
    assert_eq!(runs[0].vertices.len(), 2);
    // The gap keeps its slot: the path starts where the sample index says,
    // not where the gap was.
    assert!(
        (runs[0].vertices[0].x - 90.0 * 2.0 / 3.0).abs() < 1e-3,
        "the path starts where the sample index says (x {})",
        runs[0].vertices[0].x
    );
}

#[test]
fn the_scaled_projection_anchors_the_curve_to_its_ceiling() {
    let runs = chart_runs(
        &[0.0, 50.0, 100.0],
        100.0,
        40.0,
        MAX_CHART_POINTS,
        ChartScale::Ceiling(100.0),
    );
    assert_eq!(runs.len(), 1);
    let vertices = &runs[0].vertices;
    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[0].y, 40.0, "zero sits on the floor");
    assert_eq!(vertices[1].y, 20.0, "half the ceiling sits halfway up");
    assert_eq!(vertices[2].y, 0.0, "the ceiling sits on the top");
}
