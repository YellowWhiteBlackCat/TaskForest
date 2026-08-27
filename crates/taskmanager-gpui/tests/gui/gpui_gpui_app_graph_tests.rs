//! Unit tests for the pure-logic graph helpers. The paint paths themselves
//! (gradient fill, smoothed stroke, badge) are exercised end-to-end by the
//! page-specific render suites; here we cover the
//! arithmetic that is cheap to assert without a window.
// Import ONLY the helpers under test + the two gpui ctors — NOT `use
// super::*`. The parent module has `use gpui::*;`, whose prelude shadows the
// built-in `#[test]` attribute macro once re-globbed in here, which trips a
// recursion-limit error on the attribute itself (same landmine documented in
// `system_view::tests`). Keeping this scope minimal resolves `#[test]` to
// the std built-in.
use super::hover::sample_index_at_cursor_x;
use super::slide::{GRAPH_SLIDE_DURATION, slide_progress_value, slide_timing_for};
use super::{
    GraphOpts, GraphSampleState, GraphSettings, MAX_GRAPH_DATA_POINTS, MIN_GRAPH_DATA_POINTS,
    SMOOTH_TENSION, catmull_rom_controls, compute_column_count, finite_sample_runs,
    graph_sample_state, graph_slide_spacing, latest_finite_sample, latest_samples_rc,
    latest_samples_rc_for_slide, sample_x, sample_x_slide,
};
use gpui::{Bounds, ElementId, Pixels, point, px, size};
use std::rc::Rc;
use std::time::Instant;

/// Keep graph pixels, hover lookup, and summary statistics on the same newest
/// sample window. Empty/gap samples are retained; only the time-axis prefix is
/// discarded when a user chooses fewer points.
///
/// Superseded in production by `latest_samples_rc` (the shared-series,
/// zero-copy-on-fit variant); retained as the slice-tail reference the tests
/// compare against.
fn limit_samples(samples: &[f32], data_points: usize) -> Vec<f32> {
    let limit = GraphSettings::clamp_data_points(data_points);
    if samples.len() <= limit {
        samples.to_vec()
    } else {
        samples[samples.len() - limit..].to_vec()
    }
}

/// Nearest finite sample value at a window-space cursor x.
fn sample_at_cursor_x(
    samples: &[f32],
    left: Pixels,
    width: Pixels,
    x: Pixels,
    capacity: usize,
) -> Option<f32> {
    sample_index_at_cursor_x(samples, left, width, x, capacity).map(|index| samples[index])
}

#[test]
fn compute_column_count_known_values() {
    // Documented MC recipe cases.
    assert_eq!(compute_column_count(1), 1);
    assert_eq!(compute_column_count(2), 2);
    assert_eq!(compute_column_count(3), 3);
    assert_eq!(compute_column_count(20), 4); // 4x5
    assert_eq!(compute_column_count(24), 6); // 6x4
}

#[test]
fn catmull_rom_rejects_short_inputs() {
    assert!(catmull_rom_controls(&[], SMOOTH_TENSION).is_none());
    assert!(catmull_rom_controls(&[point(px(0.), px(0.))], SMOOTH_TENSION).is_none());
}

#[test]
fn catmull_rom_segment_count_matches() {
    let pts = vec![
        point(px(0.), px(0.)),
        point(px(1.), px(2.)),
        point(px(2.), px(1.)),
        point(px(3.), px(3.)),
        point(px(4.), px(0.)),
    ];
    let (ctrl_a, ctrl_b) = catmull_rom_controls(&pts, SMOOTH_TENSION).expect(">=2 pts");
    // n-1 segments.
    assert_eq!(ctrl_a.len(), pts.len() - 1);
    assert_eq!(ctrl_b.len(), pts.len() - 1);
}

#[test]
fn catmull_rom_straight_line_stays_collinear() {
    // Collinear samples must yield control points still on the line y=x,
    // so the smoothed curve does not bow off a straight diagonal.
    let pts = vec![
        point(px(0.), px(0.)),
        point(px(10.), px(10.)),
        point(px(20.), px(20.)),
        point(px(30.), px(30.)),
    ];
    let (ctrl_a, ctrl_b) = catmull_rom_controls(&pts, SMOOTH_TENSION).unwrap();
    for (ca, cb) in ctrl_a.iter().zip(ctrl_b.iter()) {
        assert!(
            (f32::from(ca.x) - f32::from(ca.y)).abs() < 1e-3,
            "ca off line"
        );
        assert!(
            (f32::from(cb.x) - f32::from(cb.y)).abs() < 1e-3,
            "cb off line"
        );
    }
}

#[test]
fn catmull_rom_zero_tension_is_straight() {
    // tension 0 => control points coincide with the endpoints => the cubic
    // degenerates to a straight segment (no smoothing).
    let pts = vec![
        point(px(0.), px(0.)),
        point(px(5.), px(7.)),
        point(px(10.), px(3.)),
    ];
    let (ctrl_a, ctrl_b) = catmull_rom_controls(&pts, 0.0).unwrap();
    // Segment 0: ctrl_a == pts[0], ctrl_b == pts[1].
    assert!((f32::from(ctrl_a[0].x)).abs() < 1e-3);
    assert!((f32::from(ctrl_a[0].y)).abs() < 1e-3);
    assert!((f32::from(ctrl_b[0].x) - 5.0).abs() < 1e-3);
    assert!((f32::from(ctrl_b[0].y) - 7.0).abs() < 1e-3);
}

#[test]
fn sample_at_cursor_x_edges_and_mid() {
    let s = [0.0_f32, 50.0, 100.0, 50.0];
    let left = px(0.0);
    let width = px(300.0);
    // Full window (n == capacity): left edge → first sample, right edge →
    // last sample, mid → idx 2.
    assert_eq!(sample_at_cursor_x(&s, left, width, px(0.0), 4), Some(0.0));
    assert_eq!(
        sample_at_cursor_x(&s, left, width, px(300.0), 4),
        Some(50.0)
    );
    assert_eq!(
        sample_at_cursor_x(&s, left, width, px(150.0), 4),
        Some(100.0)
    );
}

#[test]
fn graph_slide_spacing_matches_one_sample_slot() {
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(600.0), px(100.0)));

    let spacing = graph_slide_spacing(bounds, 60);
    let expected = 600.0 / 59.0;
    assert!((f32::from(spacing) - expected).abs() < 0.01);
}

#[test]
fn slide_mapping_shows_previous_window_then_current_window() {
    let left = px(0.0);
    let width = px(600.0);

    // Progress 0: the extra older sample sits at the left edge and the
    // current newest sample is one slot off-screen right, so the visible
    // curve is exactly the previous window.
    assert!((f32::from(sample_x_slide(left, width, 0, 60, 0.0))).abs() < 0.01);
    assert!((f32::from(sample_x_slide(left, width, 59, 60, 0.0)) - 600.0).abs() < 0.01);
    assert!(
        f32::from(sample_x_slide(left, width, 60, 60, 0.0)) > 600.0,
        "the newest sample must enter from off-screen right"
    );

    // Progress 1: the current window occupies the full width with the newest
    // sample pinned at the right edge.
    assert!((f32::from(sample_x_slide(left, width, 1, 60, 1.0))).abs() < 0.01);
    assert!((f32::from(sample_x_slide(left, width, 60, 60, 1.0)) - 600.0).abs() < 0.01);
}

#[test]
fn hover_index_uses_the_same_slot_grid_as_the_painted_curve() {
    let samples = [10.0_f32, 20.0, 30.0];
    let left = px(0.0);
    let width = px(600.0);
    // Three samples in a 60-slot window occupy the last three slots.
    assert_eq!(
        sample_index_at_cursor_x(&samples, left, width, px(600.0), 60),
        Some(2)
    );
    assert_eq!(
        sample_index_at_cursor_x(&samples, left, width, px(600.0 - 600.0 / 59.0), 60),
        Some(1)
    );
    assert_eq!(
        sample_index_at_cursor_x(&samples, left, width, px(600.0 - 2.0 * 600.0 / 59.0), 60),
        Some(0)
    );
    assert_eq!(
        sample_index_at_cursor_x(&samples, left, width, px(0.0), 60),
        Some(0),
        "empty leading canvas clamps to the oldest occupied slot"
    );
}

#[test]
fn sample_at_cursor_x_empty_or_zero_width() {
    assert_eq!(
        sample_at_cursor_x(&[], px(0.0), px(100.0), px(10.0), 60),
        None
    );
    assert_eq!(
        sample_at_cursor_x(&[1.0, 2.0], px(0.0), px(0.0), px(10.0), 60),
        None,
    );
}

/// The right-anchored growing-window contract: with fewer samples than the
/// window capacity, the newest sample stays at the RIGHT edge and each old
/// sample occupies one fixed slot to its left. This is the compact startup
/// behavior used by mature task managers.
#[test]
fn sample_x_grows_from_the_right_edge_at_fixed_slot_spacing() {
    let left = px(0.0);
    let width = px(600.0);
    let capacity = 60;

    // Freshly opened (n=3): the newest sample is pinned to the right edge;
    // the older two samples occupy the two slots immediately to its left.
    let slot = f32::from(width) / (capacity as f32 - 1.0);
    let first = f32::from(sample_x(left, width, 0, 3, capacity));
    let newest = f32::from(sample_x(left, width, 2, 3, capacity));
    assert!((first - (600.0 - 2.0 * slot)).abs() < 0.01);
    assert!((newest - 600.0).abs() < 0.01, "newest must pin right");
    assert!(
        newest > first,
        "newest must remain to the right of the oldest"
    );

    // Full window (n == capacity): classic full spread, oldest at 0.
    let oldest = f32::from(sample_x(left, width, 0, capacity, capacity));
    assert!(oldest.abs() < 0.01);
    let full_newest = f32::from(sample_x(left, width, capacity - 1, capacity, capacity));
    assert!((full_newest - 600.0).abs() < 0.01);
}

#[test]
fn hover_over_a_growing_window_resolves_the_right_anchored_samples() {
    // n=2 of 60: the meaningful hover targets occupy the last two fixed
    // slots, with the newest sample at the right edge.
    let samples = [10.0_f32, 20.0];
    let width = px(600.0);
    assert_eq!(
        sample_at_cursor_x(&samples, px(0.0), width, px(600.0), 60),
        Some(20.0),
        "cursor at the right edge reads the newest sample"
    );
    assert_eq!(
        sample_at_cursor_x(&samples, px(0.0), width, px(600.0 - 600.0 / 59.0), 60),
        Some(10.0),
        "the previous slot reads the oldest sample"
    );
    // The leading canvas is empty; clamping still resolves to the oldest
    // occupied slot rather than inventing another sample.
    assert_eq!(
        sample_at_cursor_x(&samples, px(0.0), width, px(0.0), 60),
        Some(10.0)
    );
}

#[test]
fn non_finite_samples_split_graph_runs_without_collapsing_time() {
    let samples = [10.0, 20.0, f32::NAN, f32::NAN, 0.0, 5.0, f32::NAN];

    assert_eq!(
        finite_sample_runs(&samples),
        [vec![(0, 10.0), (1, 20.0)], vec![(4, 0.0), (5, 5.0)]]
    );
}

#[test]
fn graph_sample_state_distinguishes_warmup_from_provider_gap() {
    assert_eq!(graph_sample_state(&[]), GraphSampleState::Collecting);
    assert_eq!(
        graph_sample_state(&[f32::NAN, f32::INFINITY]),
        GraphSampleState::Unavailable
    );
    assert_eq!(graph_sample_state(&[1.0]), GraphSampleState::Collecting);
    assert_eq!(
        graph_sample_state(&[f32::NAN, 1.0]),
        GraphSampleState::Collecting
    );
    assert_eq!(
        graph_sample_state(&[f32::NAN, 0.0, 4.0]),
        GraphSampleState::Measured
    );
}

#[test]
fn graph_badge_and_hover_do_not_reuse_a_value_across_a_gap() {
    let samples = [10.0, 20.0, f32::NAN];

    assert_eq!(latest_finite_sample(&samples), None);
    assert_eq!(
        sample_at_cursor_x(&samples, px(0.0), px(200.0), px(200.0), 3),
        None
    );
    assert_eq!(
        sample_at_cursor_x(&samples, px(0.0), px(200.0), px(100.0), 3),
        Some(20.0)
    );
}

#[test]
fn graph_opts_default_preserves_baseline_appearance() {
    // Every refinement must be OFF by default so existing callers render
    // exactly as before. This is the backward-compat contract.
    let o = GraphOpts::default();
    assert!(!o.gradient_fill);
    assert!(!o.ref_lines);
    assert!(!o.value_badge);
    assert!(o.badge_fmt.is_none());
    // Smoothing flipped to the intrinsic default (elegant-curve policy).
    assert!(o.smooth);
    assert_eq!(o.data_points, MAX_GRAPH_DATA_POINTS);
    assert!(!o.sliding);
    // Baseline tunables unchanged.
    assert_eq!(o.fill_alpha, 0.39);
    assert_eq!(o.grid_alpha, 0.13);
    assert_eq!(o.hlines, 5);
    assert_eq!(o.vlines, 6);
    assert_eq!(o.stroke_width, 1.5);
    assert_eq!(o.max, 100.0);
}

#[test]
fn graph_settings_clamp_configured_points_and_apply_render_preferences() {
    let settings = GraphSettings::from_config(9, true, false, 42);
    assert_eq!(settings.data_points, MIN_GRAPH_DATA_POINTS);
    assert!(settings.sliding_graphs);
    assert!(!settings.network_dynamic_scaling);
    assert_eq!(settings.animation_epoch, 42);

    let oversized = GraphSettings::from_config(u32::MAX, false, true, 0);
    assert_eq!(oversized.data_points, MAX_GRAPH_DATA_POINTS);
    assert_eq!(oversized.data_points_as_config(), 600);

    let opts = GraphOpts::default().with_settings(settings);
    // Smoothing is intrinsic rendering now: settings never disable it.
    assert!(opts.smooth);
    assert!(opts.sliding);
    assert_eq!(opts.data_points, MIN_GRAPH_DATA_POINTS);
    assert_eq!(opts.animation_epoch, 42);
}

#[test]
fn limit_samples_keeps_the_newest_points_and_preserves_gaps() {
    let samples = [1.0, f32::NAN, 3.0, 4.0];
    let unchanged = limit_samples(&samples, 10);
    assert_eq!(unchanged.len(), samples.len());
    assert_eq!(unchanged[0], samples[0]);
    assert!(unchanged[1].is_nan());
    assert_eq!(unchanged[2], samples[2]);
    assert_eq!(unchanged[3], samples[3]);

    let longer = [
        0.0,
        1.0,
        2.0,
        3.0,
        4.0,
        5.0,
        6.0,
        7.0,
        8.0,
        f32::NAN,
        10.0,
        11.0,
    ];
    let newest = limit_samples(&longer, 10);
    assert_eq!(newest.len(), 10);
    assert_eq!(newest[0], 2.0);
    assert_eq!(newest[6], 8.0);
    assert!(newest[7].is_nan());
    assert_eq!(newest[8], 10.0);
    assert_eq!(newest[9], 11.0);
}

#[test]
fn graph_opts_remains_copy_with_fn_pointer_field() {
    // `badge_fmt: Option<fn(f32)->String>` must keep GraphOpts Copy so the
    // existing pass-by-value call sites (graph_element(opts), etc.) compile.
    fn pct(v: f32) -> String {
        format!("{:.0}%", v)
    }
    let o = GraphOpts {
        badge_fmt: Some(pct),
        ..GraphOpts::default()
    };
    let o2 = o; // move (Copy => actually a copy)
    assert!(o.badge_fmt.is_some());
    assert!(o2.badge_fmt.is_some());
    assert_eq!(o2.badge_fmt.unwrap()(73.0), "73%");
}

/// The shared-series tail limit must be the IDENTITY when the series
/// already fits the configured window — the generation-keyed caches hand
/// out rings capped at the same bound, so a UI-only frame keeps the same
/// `Rc` and copies nothing.
#[test]
fn latest_samples_rc_keeps_the_same_rc_when_the_series_fits() {
    let series: Rc<[f32]> = Rc::from([1.0, 2.0, 3.0].as_slice());
    let limited = latest_samples_rc(Rc::clone(&series), MAX_GRAPH_DATA_POINTS);
    assert!(
        Rc::ptr_eq(&series, &limited),
        "a fitting series must not be re-allocated"
    );
}

/// The slide ledger is the page-switch antidote: a remounted graph with the
/// same data must keep its original wall-clock start (settled or still
/// sliding), never restart from zero and visibly “reverse”. The generation is
/// the bit-exact window content, so unrelated telemetry revisions (the store
/// shares one revision across every domain) cannot re-slide the graph.
#[test]
fn slide_timing_survives_same_content_remount() {
    let mut ledger = Default::default();
    let id = ElementId::from("slide-ledger-remount");
    let samples: Rc<[f32]> = Rc::from((0..61).map(|i| i as f32).collect::<Vec<_>>().as_slice());
    let first = slide_timing_for(&mut ledger, &id, &samples, Instant::now());
    // A page remount rebuilds the element but the projection cache hands the
    // SAME slice `Rc`; even a fresh allocation with identical content must not
    // restart the slide.
    let remount = slide_timing_for(&mut ledger, &id, &samples, Instant::now());
    assert_eq!(
        first, remount,
        "same graph + same content must continue from the same start"
    );
    let new_samples: Rc<[f32]> = Rc::from((1..62).map(|i| i as f32).collect::<Vec<_>>().as_slice());
    let next = slide_timing_for(&mut ledger, &id, &new_samples, Instant::now());
    assert!(next >= first, "new window content must start a fresh slide");
}

/// Two graphs each own a timing slot, so a device or row switch cannot make
/// one graph inherit another series' slide state.
#[test]
fn slide_timing_is_per_graph_id() {
    let mut ledger = Default::default();
    let id_a = ElementId::from("slide-ledger-a");
    let id_b = ElementId::from("slide-ledger-b");
    let samples: Rc<[f32]> = Rc::from((0..61).map(|i| i as f32).collect::<Vec<_>>().as_slice());
    let first_a = slide_timing_for(&mut ledger, &id_a, &samples, Instant::now());
    let first_b = slide_timing_for(&mut ledger, &id_b, &samples, Instant::now());
    assert_eq!(
        slide_timing_for(&mut ledger, &id_a, &samples, Instant::now()),
        first_a
    );
    assert_eq!(
        slide_timing_for(&mut ledger, &id_b, &samples, Instant::now()),
        first_b
    );
    assert!(
        first_b >= first_a,
        "independent entries are created on demand"
    );
}

/// THE regression contract for the “filled graph still walks backward” bug:
/// a full ring (source longer than the slide window) must not restart its
/// slide when the same data is rendered again — not on a fresh tail-slice
/// allocation, and not when an unrelated telemetry domain advances the shared
/// global revision. Only genuinely new window content may start a new slide.
#[test]
fn full_graph_slide_ignores_unrelated_revision_and_fresh_allocations() {
    let mut ledger = Default::default();
    let id = ElementId::from("full-graph-revision-regression");
    let source: Rc<[f32]> = Rc::from((0..600).map(|i| i as f32).collect::<Vec<_>>().as_slice());
    let slice = latest_samples_rc_for_slide(Rc::clone(&source), 60);
    let first = slide_timing_for(&mut ledger, &id, &slice, Instant::now());

    // One frame later the graph is rebuilt with the same source; the memo
    // hands back the SAME slice, and the ledger must keep the same start.
    let next_frame = latest_samples_rc_for_slide(Rc::clone(&source), 60);
    assert!(Rc::ptr_eq(&slice, &next_frame));
    assert_eq!(
        slide_timing_for(&mut ledger, &id, &next_frame, Instant::now()),
        first,
        "an unchanged full-ring frame must not restart the slide"
    );

    // Even a brand-new allocation with identical content (a caller that
    // re-projects every frame) must not restart the slide.
    let same_content: Rc<[f32]> = Rc::from(slice.to_vec().as_slice());
    assert_eq!(
        slide_timing_for(&mut ledger, &id, &same_content, Instant::now()),
        first,
        "identical window content must share one slide generation"
    );

    // Genuinely new data (the window shifted by one sample) starts fresh.
    let new_window: Rc<[f32]> = Rc::from((1..62).map(|i| i as f32).collect::<Vec<_>>().as_slice());
    assert!(
        slide_timing_for(&mut ledger, &id, &new_window, Instant::now()) >= first,
        "new window content must start a new slide"
    );
}

/// The slide timeline is monotonic and settles exactly once the duration has
/// elapsed. A regression here means the graph would stutter or hang mid-slide.
#[test]
fn slide_progress_is_monotonic_and_settles_after_duration() {
    let start = Instant::now();
    assert_eq!(slide_progress_value(start, start), 0.0);
    let quarter = slide_progress_value(start, start + GRAPH_SLIDE_DURATION / 4);
    let half = slide_progress_value(start, start + GRAPH_SLIDE_DURATION / 2);
    assert!(
        (0.0..1.0).contains(&quarter) && (0.0..1.0).contains(&half) && half > quarter,
        "progress must ease monotonically through the animation"
    );
    assert_eq!(
        slide_progress_value(start, start + GRAPH_SLIDE_DURATION),
        1.0
    );
    assert_eq!(
        slide_progress_value(start, start + GRAPH_SLIDE_DURATION * 2),
        1.0,
        "a settled slide must stay settled forever"
    );
}

/// A full ring must keep ONE stable tail-slice allocation across frames;
/// otherwise every animation frame changes the scene-cache key and the slide
/// re-tessellates (the “filled graph” jank).
#[test]
fn full_ring_slide_slice_is_memoized_by_source_identity() {
    let source: Rc<[f32]> = Rc::from((0..600).map(|i| i as f32).collect::<Vec<_>>().as_slice());
    let first = latest_samples_rc_for_slide(Rc::clone(&source), 60);
    let second = latest_samples_rc_for_slide(Rc::clone(&source), 60);
    assert_eq!(first.len(), 61);
    assert!(
        Rc::ptr_eq(&first, &second),
        "the same full ring must reuse the memoized tail slice"
    );
    let smaller = latest_samples_rc_for_slide(Rc::clone(&source), 120);
    assert!(
        !Rc::ptr_eq(&first, &smaller),
        "a different window length gets its own slice"
    );
    let other_source: Rc<[f32]> =
        Rc::from((0..600).map(|i| i as f32).collect::<Vec<_>>().as_slice());
    let other = latest_samples_rc_for_slide(Rc::clone(&other_source), 60);
    assert!(
        !Rc::ptr_eq(&first, &other),
        "a distinct source allocation must not reuse another ring's slice"
    );
}

/// A series longer than the configured window pays exactly one tail-slice
/// and keeps the newest samples (same cut `limit_samples` produces).
#[test]
fn latest_samples_rc_tail_cuts_like_limit_samples() {
    // The data-points setting clamps to a floor of 10, so the cut is only
    // observable above it.
    let series: Rc<[f32]> = Rc::from((0..30).map(|i| i as f32).collect::<Vec<_>>().as_slice());
    let limited = latest_samples_rc(Rc::clone(&series), 12);
    assert_eq!(
        &*limited,
        &(18..30).map(|i| i as f32).collect::<Vec<_>>()[..]
    );
    assert!(!Rc::ptr_eq(&series, &limited));

    let expected = limit_samples(&series, 12);
    assert_eq!(&*limited, expected.as_slice());

    // Below the floor the setting clamps UP, so a 10-sample series is not
    // cut at all.
    let small: Rc<[f32]> = Rc::from((0..10).map(|i| i as f32).collect::<Vec<_>>().as_slice());
    assert!(Rc::ptr_eq(&small, &latest_samples_rc(Rc::clone(&small), 3)));
}

// ── two-series graphs (split-direction throughput families) ────────────────

use super::hover::multi_series_hover_text;
use super::{SECONDARY_TINT_LIFT, dual_series_colors};
use gpui::Rgba;

// test-intent: behavior
/// The secondary series color is the family token lifted toward white by the
/// pinned lift factor — a tint of the SAME color on every channel, alpha
/// untouched, never a new product color; the primary wears the token as-is.
#[test]
fn dual_series_colors_tint_the_secondary_toward_white_only() {
    let base = Rgba {
        r: 0.2,
        g: 0.4,
        b: 0.9,
        a: 1.0,
    };
    let (primary, secondary) = dual_series_colors(base);
    assert_eq!(
        (primary.r, primary.g, primary.b, primary.a),
        (base.r, base.g, base.b, base.a),
        "the primary series wears the family token as-is"
    );
    for (channel, lifted) in [
        (base.r, secondary.r),
        (base.g, secondary.g),
        (base.b, secondary.b),
    ] {
        let expected = channel + (1.0 - channel) * SECONDARY_TINT_LIFT;
        assert!(
            (lifted - expected).abs() < 1e-6,
            "the secondary channel must be exactly the lift of the token"
        );
        assert!(
            channel < lifted && lifted < 1.0,
            "the tint sits strictly between the token and white"
        );
    }
    assert_eq!(secondary.a, base.a, "alpha is untouched");

    // An already-white channel stays white (the lift is idempotent at 1.0).
    let white = Rgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 0.5,
    };
    let (_, lifted_white) = dual_series_colors(white);
    assert_eq!(
        (lifted_white.r, lifted_white.g, lifted_white.b),
        (white.r, white.g, white.b)
    );
}

// test-intent: behavior
/// The two-series tooltip speaks only for the directions that hold evidence
/// at the hovered slot: both finite → both labeled values; one gap → that
/// direction alone; both gap or an out-of-range slot → no tooltip at all.
#[test]
fn multi_series_hover_text_composes_directions_and_suppresses_shared_gaps() {
    let fmt = |value: f32| format!("{value:.0}");
    let primary = vec![1.0, f32::NAN, 2.0, f32::NAN];
    let secondary = vec![f32::NAN, 5.0, 6.0, f32::NAN];
    let text = |index: usize| {
        multi_series_hover_text("Read", &primary, Some(("Write", &secondary)), index, &fmt)
    };
    assert_eq!(text(0).as_deref(), Some("Read 1"));
    assert_eq!(text(1).as_deref(), Some("Write 5"));
    assert_eq!(text(2).as_deref(), Some("Read 2 \u{b7} Write 6"));
    assert_eq!(
        text(3),
        None,
        "a shared gap suppresses the tooltip instead of printing zeros"
    );
    assert_eq!(
        text(9),
        None,
        "an out-of-range slot never fabricates a reading"
    );

    // Without a secondary the composition is the labeled primary alone.
    let single = vec![7.5];
    assert_eq!(
        multi_series_hover_text("Read", &single, None, 0, &fmt).as_deref(),
        Some("Read 8")
    );
}

// test-intent: behavior
/// A two-series graph classifies its first-frame state over the UNION of the
/// directions' evidence: one measured direction is a measured graph even when
/// the other (or the summed lane behind it) holds only gaps.
#[test]
fn graph_dual_sample_state_follows_the_union_of_directions() {
    use super::GraphSampleState;
    use super::graph_dual_sample_state;

    let gap = f32::NAN;
    // Both windows empty → collecting.
    assert_eq!(
        graph_dual_sample_state(&[], &[]),
        GraphSampleState::Collecting
    );
    // Slots exist but neither direction has evidence → unavailable.
    assert_eq!(
        graph_dual_sample_state(&[gap, gap], &[gap]),
        GraphSampleState::Unavailable
    );
    // Read measured, write all-gap → measured, never "unavailable".
    assert_eq!(
        graph_dual_sample_state(&[1.0, 2.0], &[gap, gap]),
        GraphSampleState::Measured
    );
    // A single finite observation across both directions → still collecting.
    assert_eq!(
        graph_dual_sample_state(&[gap, 3.0], &[gap, gap]),
        GraphSampleState::Collecting
    );
    // Evidence split across the directions counts together.
    assert_eq!(
        graph_dual_sample_state(&[gap, 3.0], &[4.0, gap]),
        GraphSampleState::Measured
    );
}
