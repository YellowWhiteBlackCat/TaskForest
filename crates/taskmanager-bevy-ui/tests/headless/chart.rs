use super::*;
use crate::widgets::chart::geometry::{ChartScale, ChartVertex, chart_runs, chart_runs_in};
use crate::widgets::chart::painter::slide_offset;
use taskmanager_shell::presentation::ChartSlots;
use taskmanager_ui_contract::presentation::motion::chart_slide_ramp;

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
    // The slot grid is the shared owner's, and the owner keeps the release
    // surface's evaluation order: the slot ratio is measured from the right
    // (`1 - from_right / denominator`), which rounds once more than the
    // mathematically equivalent form. The gap's first sample therefore lands
    // within one f32 ULP of 60.0 (4e-6 px at this scale) instead of exactly
    // on it — the release surface's pixels stay bit-identical.
    assert!(
        (segments[0].start.x - 60.0).abs() < 1e-3,
        "the run starts at the gap's first slot: {}",
        segments[0].start.x
    );
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

// ---- the right-anchored capacity window: the curve never claims a history
// ---- the product does not have

#[test]
fn a_partially_filled_window_grows_from_the_right_edge() {
    // 25 samples inside a 60-slot window: every sample owns one 60th-of-the
    // frame slot, the newest pins the right edge, and the curve occupies the
    // rightmost 24/59 of the frame instead of pretending to be a full window.
    let slots = ChartSlots::new(25, 60, 600.0);
    assert_eq!(slots.x(24), 600.0, "the newest sample owns the right edge");
    let slot_width = 600.0 / 59.0;
    assert!(
        (slots.x(23) - (600.0 - slot_width)).abs() < 1e-4,
        "one slot back is one slot width (x {})",
        slots.x(23)
    );
    assert!(
        (slots.x(0) - (600.0 - 24.0 * slot_width)).abs() < 1e-3,
        "the oldest sample is 24 slots from the right (x {})",
        slots.x(0)
    );

    let samples = (0..25).map(|value| value as f32).collect::<Vec<_>>();
    let runs = chart_runs_in(
        &samples,
        600.0,
        40.0,
        MAX_CHART_POINTS,
        ChartScale::Window,
        60,
    );
    assert_eq!(runs.len(), 1);
    let vertices = &runs[0].vertices;
    assert_eq!(vertices[24].x, 600.0);
    assert!(
        vertices[0].x > 300.0,
        "a quarter-filled window must not span the frame (x {})",
        vertices[0].x
    );
}

#[test]
fn a_full_or_over_full_window_occupies_the_whole_frame() {
    // A settled window (samples == capacity) resolves to the plain full-span
    // grid, bit for bit: the capacity rule adds no drift once the history is
    // warm.
    let settled = ChartSlots::new(60, 60, 600.0);
    assert_eq!(settled.x(0), 0.0);
    assert_eq!(settled.x(59), 600.0);
    assert_eq!(settled.x(30), 600.0 * 30.0 / 59.0);

    // The renderer's drawing budget is not extra history: a window truncated
    // to the budget still spans the frame rather than floating to the right.
    let samples = (0..(MAX_CHART_POINTS + 17))
        .map(|value| value as f32)
        .collect::<Vec<_>>();
    let runs = chart_runs_in(
        &samples,
        600.0,
        40.0,
        MAX_CHART_POINTS,
        ChartScale::Window,
        600,
    );
    let vertices = &runs[0].vertices;
    assert_eq!(vertices.len(), MAX_CHART_POINTS);
    assert_eq!(
        vertices[0].x, 0.0,
        "the drawn window starts at the left edge"
    );
    assert_eq!(vertices[vertices.len() - 1].x, 600.0);
}

#[test]
fn the_pointer_resolves_the_slot_the_curve_was_drawn_in() {
    // The drawing x and the hover slot share one denominator, so the sample
    // under the pointer is always the sample whose vertex is under the pointer.
    let slots = ChartSlots::new(10, 10, 100.0);
    for index in 0..10 {
        let drawn = slots.x(index);
        assert_eq!(
            slots.index_at(drawn),
            Some(index),
            "the pointer on slot {index} (x {drawn}) resolves to slot {index}"
        );
    }
    // The newest sample owns the edge: a cursor AT or past the right edge is
    // the newest slot, never nothing.
    assert_eq!(slots.index_at(100.0), Some(9));
    assert_eq!(slots.index_at(140.0), Some(9));
    assert_eq!(
        slots.index_at(-1.0),
        None,
        "before the frame there is no slot"
    );

    // A part-filled window resolves in its own slots, not the whole frame.
    let warming = ChartSlots::new(3, 60, 600.0);
    assert_eq!(
        warming.index_at(600.0),
        Some(2),
        "the right edge is the newest"
    );
    assert_eq!(
        warming.index_at(0.0),
        Some(0),
        "before the curve is the oldest"
    );
}

#[test]
fn the_chart_slide_rides_the_shared_ramp() {
    // The slide's curve is the shared chart-slide ramp, the same one the GPUI
    // graph and the Iced chart slide on. The toolkit's cubic smoothstep
    // answered 0.15625 at progress 0.25 against the shared ramp's 0.125;
    // this surface now answers the shared value.
    let width = 400.0;
    let points = 5;
    let slot = width / (points - 1) as f32;
    for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let eased = 1.0 + slide_offset(progress, width, points) / slot;
        assert_eq!(
            eased,
            chart_slide_ramp(progress),
            "the slide at progress {progress} must ride the shared ramp"
        );
    }
    assert_eq!(
        chart_slide_ramp(0.25),
        0.125,
        "the shared ramp's quarter point"
    );
}

#[test]
fn the_chart_slide_never_overshoots_its_slot() {
    // The slide starts one slot off the right edge and settles exactly on
    // it: an out-of-range progress can never push the curve past its slot.
    let width = 400.0;
    let points = 5;
    let slot = width / (points - 1) as f32;
    assert_eq!(slide_offset(0.0, width, points), -slot);
    assert_eq!(slide_offset(1.0, width, points), 0.0);
    assert_eq!(slide_offset(-1.0, width, points), -slot);
    assert_eq!(slide_offset(2.0, width, points), 0.0);
    assert_eq!(
        slide_offset(0.5, width, 1),
        0.0,
        "a window with no slot does not slide"
    );
}
