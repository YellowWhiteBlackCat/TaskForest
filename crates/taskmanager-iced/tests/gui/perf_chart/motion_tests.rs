//! Headless proofs for the Performance chart's motion and raster
//! primitives: the device-grid (sub-pixel) alignment, the sample-window
//! slide transition, the damped dynamic value range, the hover pill's
//! edge-collision flip, and the y-axis scale-text tiering.
//!
//! Every assertion reads a TYPED state (a resolved coordinate, a phase, an
//! axis value, a pill side) — never a wall-clock sleep and never a
//! screenshot.

use std::rc::Rc;
use std::time::{Duration, Instant};

use super::motion::{DampedRange, SeriesSlide, SlidePhase, slide_positions, slide_progress};
use super::raster::{
    PILL_FLIP_ZONE, PillSide, pill_x_for_side, resolve_pill_side, snap_to_device_grid,
    y_axis_scale_ticks,
};
use super::*;

/// The render scales the alignment is proven at: the 1:1 grid, the app's own
/// zoom (the 16/14 body scale), and the 125 % / 150 % desktop scales.
const SCALES: [f32; 5] = [1.0, 16.0 / 14.0, 1.25, 1.5, 2.0];

/// The coordinates the alignment is proven at.
const COORDINATES: [f32; 7] = [0.0, 1.0, 3.25, 10.1, 47.5, 120.75, 199.9];

#[test]
fn subpixel_alignment_lands_on_a_device_pixel_center_at_fractional_scales() {
    for scale in SCALES {
        for value in COORDINATES {
            let snapped = snap_to_device_grid(value, scale);
            let device = snapped * scale;
            // The aligned coordinate sits on a device-pixel center: its
            // fractional part is exactly one half, so a stroke through it
            // keeps a constant sub-pixel phase at every scale factor.
            let fraction = device - device.floor();
            assert!(
                (fraction - 0.5).abs() < 1e-3,
                "scale {scale}: {device} is not on a device-pixel center"
            );
            // The alignment never moves a vertex further than half a device
            // pixel, so the curve keeps its shape.
            assert!(
                (snapped - value).abs() <= 0.5 / scale + 1e-3,
                "scale {scale}: the alignment moved {value} too far"
            );
        }
    }
}

#[test]
fn subpixel_alignment_is_idempotent_and_never_reorders_a_curve() {
    for scale in SCALES {
        let mut previous = f32::NEG_INFINITY;
        for value in COORDINATES {
            let snapped = snap_to_device_grid(value, scale);
            assert_eq!(
                snap_to_device_grid(snapped, scale),
                snapped,
                "scale {scale}: the alignment must be idempotent"
            );
            assert!(
                snapped >= previous,
                "scale {scale}: the alignment must not reorder the curve"
            );
            previous = snapped;
        }
    }
}

#[test]
fn subpixel_alignment_falls_back_to_the_unit_grid_for_an_unusable_scale() {
    // A non-positive or non-finite scale factor is the honest "unknown
    // device grid" state: the unit grid keeps the curve on screen instead
    // of projecting it away.
    assert_eq!(
        snap_to_device_grid(10.0, 0.0),
        snap_to_device_grid(10.0, 1.0)
    );
    assert_eq!(
        snap_to_device_grid(10.0, f32::NAN),
        snap_to_device_grid(10.0, 1.0)
    );
    assert!(snap_to_device_grid(f32::NAN, 1.25).is_nan());
}

#[test]
fn slide_reproduces_the_outgoing_frame_at_zero_and_the_incoming_one_at_one() {
    let outgoing: Rc<[f32]> = Rc::from([10.0_f32, 20.0, 30.0, 40.0]);
    let incoming: Rc<[f32]> = Rc::from([20.0_f32, 30.0, 40.0, 50.0]);
    let mut slide = SeriesSlide::default();
    let start = Instant::now();
    assert_eq!(
        slide.advance(&outgoing, start),
        SlidePhase::Settled,
        "the first window has no outgoing frame to slide from"
    );
    // The next window restarts the slide.
    assert_eq!(
        slide.advance(&incoming, start),
        SlidePhase::Sliding { progress: 0.0 }
    );

    // The slide base is the outgoing window plus the incoming sample.
    let base = slide.base().expect("the slide base");
    assert_eq!(base, vec![10.0, 20.0, 30.0, 40.0, 50.0]);

    // At progress 0 the outgoing frame is reproduced exactly: the four
    // outgoing samples keep their own slots and the incoming one waits one
    // slot off the right edge.
    let outgoing_positions = slide_positions(4, 4, 300.0, 0.0);
    assert_eq!(outgoing_positions.len(), 5);
    assert_eq!(outgoing_positions[0], 0.0);
    assert_eq!(outgoing_positions[3], 300.0);
    assert_eq!(
        outgoing_positions[4], 400.0,
        "the incoming sample waits off-frame"
    );

    // At progress 1 the incoming frame is reached: the oldest sample has
    // left past the left edge and the incoming one owns the right edge.
    let incoming_positions = slide_positions(4, 4, 300.0, 1.0);
    assert_eq!(incoming_positions[1], 0.0);
    assert_eq!(incoming_positions[4], 300.0);
    assert_eq!(
        incoming_positions[0], -100.0,
        "the oldest sample leaves the frame"
    );
}

#[test]
fn slide_projects_the_base_runs_at_the_interpolated_positions() {
    let outgoing: Rc<[f32]> = Rc::from([10.0_f32, 20.0, 30.0, 40.0]);
    let incoming: Rc<[f32]> = Rc::from([20.0_f32, 30.0, 40.0, 50.0]);
    let mut slide = SeriesSlide::default();
    let start = Instant::now();
    slide.advance(&outgoing, start);
    slide.advance(&incoming, start);
    let size = Size::new(300.0, 100.0);

    let runs = slide
        .points(4, size, 0.0)
        .expect("the slide projects points");
    assert_eq!(runs.len(), 1, "one finite run");
    let points = &runs[0];
    assert_eq!(
        points.len(),
        5,
        "the base carries the outgoing window and the sample"
    );
    assert_eq!(points[0].x, 0.0);
    assert_eq!(points[4].x, 400.0);
    // The y values keep the shared percentage projection.
    assert_eq!(points[0].y, 90.0, "10 % of a 100 px frame sits 90 px down");

    let settled = slide
        .points(4, size, 1.0)
        .expect("the slide projects points");
    assert_eq!(
        settled[0][1].x, 0.0,
        "the incoming window's oldest sample owns the left edge"
    );
    assert_eq!(
        settled[0][4].x, 300.0,
        "the incoming sample owns the right edge"
    );
}

#[test]
fn slide_progress_eases_in_and_out_over_the_shared_duration() {
    assert_eq!(slide_progress(Duration::ZERO), 0.0);
    assert_eq!(slide_progress(super::motion::SLIDE_DURATION), 1.0);
    assert_eq!(slide_progress(super::motion::SLIDE_DURATION * 2), 1.0);
    let middle = slide_progress(super::motion::SLIDE_DURATION / 2);
    assert!(
        (0.0..1.0).contains(&middle),
        "the eased progress stays inside the transition: {middle}"
    );
    // The ramp is symmetric: the first and the second half take the same
    // time, so the curve neither lurches nor stalls at the midpoint.
    let quarter = slide_progress(super::motion::SLIDE_DURATION / 4);
    let three_quarters = slide_progress(super::motion::SLIDE_DURATION * 3 / 4);
    assert!(
        (quarter + three_quarters - 1.0).abs() < 1e-3,
        "the eased ramp is symmetric: {quarter} / {three_quarters}"
    );
}

#[test]
fn damped_range_eases_the_ceiling_instead_of_snapping_it() {
    let start = Instant::now();
    let mut range = DampedRange::new(100.0, start);
    // A rate spike raises the target: the ceiling starts at the range that
    // is on screen.
    assert_eq!(range.advance(400.0, start), 100.0);
    // Halfway through the 120 ms transition the ceiling has moved, but it
    // has not jumped.
    let middle = range.advance(400.0, start + Duration::from_millis(60));
    assert!(
        middle > 100.0 && middle < 400.0,
        "the ceiling must ease into the new scale: {middle}"
    );
    // The transition completes after the shared duration.
    assert_eq!(
        range.advance(400.0, start + Duration::from_millis(120)),
        400.0
    );
}

#[test]
fn hover_pill_flips_to_the_left_inside_the_right_edge_zone() {
    let width = 200.0;
    let anchor = width * (1.0 - PILL_FLIP_ZONE / 2.0);
    assert_eq!(
        resolve_pill_side(anchor, width),
        PillSide::Left,
        "the right-most tenth flips the pill to the cursor's left"
    );
    assert_eq!(
        resolve_pill_side(width * 0.5, width),
        PillSide::Centered,
        "away from the edge the pill stays centered on the crosshair"
    );

    let pill_width = 60.0;
    let flipped = pill_x_for_side(PillSide::Left, anchor, pill_width, width);
    assert!(
        flipped + pill_width <= anchor,
        "the flipped pill must stay left of the crosshair: {flipped}"
    );
    assert!(
        flipped >= 0.0 && flipped + pill_width <= width,
        "the flipped pill must stay inside the frame: {flipped}"
    );
    let centered = pill_x_for_side(PillSide::Centered, width * 0.5, pill_width, width);
    assert_eq!(centered, width * 0.5 - pill_width / 2.0);
}

#[test]
fn scale_text_tier_drops_the_dense_subdivision_on_a_compact_gauge() {
    let ladder = [2400.0_f32, 1200.0, 0.0];
    // A tall frame keeps the full ladder.
    assert_eq!(y_axis_scale_ticks(&ladder, 260.0), ladder.to_vec());
    // The shared secondary-chart height still carries the full ladder.
    assert_eq!(y_axis_scale_ticks(&ladder, 128.0), ladder.to_vec());
    // A compact gauge keeps the MAIN ticks only: the dense midpoint
    // subdivision is dropped so the numbers never stack.
    assert_eq!(y_axis_scale_ticks(&ladder, 120.0), vec![2400.0, 0.0]);
    // Below the readability floor no scale text is drawn at all.
    assert!(y_axis_scale_ticks(&ladder, 72.0).is_empty());
    // A non-finite frame height carries no scale text.
    assert!(y_axis_scale_ticks(&ladder, f32::NAN).is_empty());
}
