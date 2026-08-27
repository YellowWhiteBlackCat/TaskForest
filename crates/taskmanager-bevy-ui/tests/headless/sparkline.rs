//! test-intent: behavior
//!
//! Pure-core tests for the sparkline projection: degenerate inputs (empty,
//! single point, constant series, non-finite samples) and exact min/max
//! mapping. The oracle is hand-computed geometry — no bevy, no world.

use super::{bar_fractions, polyline};

const WIDTH: f32 = 100.0;
const HEIGHT: f32 = 40.0;
const MID: f32 = 20.0;

fn x_of(samples: &[f32]) -> Vec<f32> {
    polyline(samples, WIDTH, HEIGHT)
        .into_iter()
        .map(|v| v.x)
        .collect()
}

#[test]
fn empty_samples_project_to_an_empty_path() {
    assert!(polyline(&[], WIDTH, HEIGHT).is_empty());
}

#[test]
fn a_single_sample_pins_to_the_newest_edge_at_mid_height() {
    let path = polyline(&[7.0], WIDTH, HEIGHT);
    assert_eq!(path.len(), 1);
    assert_eq!(path[0].x, WIDTH, "the lone sample sits on the newest edge");
    assert_eq!(
        path[0].y, MID,
        "one point has no range; mid is the neutral y"
    );
}

#[test]
fn min_max_map_to_bottom_and_top_with_endpoint_spacing() {
    let path = polyline(&[0.0, 1.0], WIDTH, HEIGHT);
    assert_eq!(path.len(), 2);
    assert_eq!(path[0].x, 0.0, "oldest sample at the left edge");
    assert_eq!(path[1].x, WIDTH, "newest sample at the right edge");
    assert_eq!(path[0].y, HEIGHT, "min maps to the box bottom");
    assert_eq!(path[1].y, 0.0, "max maps to the box top");
    // A middle value maps proportionally: [0, 2, 1] → 40, 0, 20.
    let middle = polyline(&[0.0, 2.0, 1.0], WIDTH, HEIGHT);
    assert!(
        (middle[2].y - 20.0).abs() < 1e-4,
        "mid value maps to mid height"
    );
}

#[test]
fn a_constant_series_is_a_flat_mid_line() {
    let path = polyline(&[3.0, 3.0, 3.0, 3.0], WIDTH, HEIGHT);
    assert_eq!(path.len(), 4);
    for vertex in path {
        assert_eq!(vertex.y, MID, "flat trend reads as a flat trend at mid");
    }
}

#[test]
fn non_finite_samples_clamp_to_mid_never_fabricate_a_trend() {
    // One NaN among finite values: that vertex goes to mid, the finite
    // vertices keep their exact mapping.
    let path = polyline(&[0.0, f32::NAN, 1.0], WIDTH, HEIGHT);
    assert!((path[0].y - HEIGHT).abs() < 1e-4);
    assert!(
        (path[1].y - MID).abs() < 1e-4,
        "the NaN vertex clamps to mid"
    );
    assert!((path[2].y - 0.0).abs() < 1e-4);
    // All-NaN input: no finite range at all, still one mid vertex per sample.
    let all_nan = polyline(&[f32::NAN, f32::NAN], WIDTH, HEIGHT);
    assert_eq!(all_nan.len(), 2);
    for vertex in all_nan {
        assert_eq!(vertex.y, MID);
    }
    // +inf/-inf samples are equally non-finite observations.
    let infinities = polyline(&[f32::INFINITY, f32::NEG_INFINITY], WIDTH, HEIGHT);
    for vertex in infinities {
        assert_eq!(vertex.y, MID);
    }
}

#[test]
fn x_positions_are_evenly_spaced_with_endpoints_at_both_edges() {
    let x = x_of(&[0.0, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(x.len(), 5);
    assert_eq!(x[0], 0.0);
    assert_eq!(x[4], WIDTH);
    for pair in x.windows(2) {
        let step = pair[1] - pair[0];
        assert!((step - 25.0).abs() < 1e-4, "even spacing, got step {step}");
    }
}

#[test]
fn bar_fractions_share_the_polyline_semantics_bottom_aligned() {
    // min → 0-height bar, max → full bar, mirroring polyline's y mapping.
    assert_eq!(bar_fractions(&[0.0, 1.0]), vec![0.0, 1.0]);
    // Constant series and single samples mirror the mid-height projection.
    assert_eq!(bar_fractions(&[5.0, 5.0]), vec![0.5, 0.5]);
    assert_eq!(bar_fractions(&[9.0]), vec![0.5]);
    assert!(bar_fractions(&[]).is_empty());
}
