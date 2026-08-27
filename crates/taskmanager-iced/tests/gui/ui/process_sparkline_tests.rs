use super::*;

const SIZE: Size = Size::new(48.0, 16.0);

/// Fewer than two samples render the midpoint horizontal baseline as two
/// points spanning the full width — row height stays stable, no fabricated
/// trend, no panic.
#[test]
fn empty_window_is_a_two_point_midpoint_baseline() {
    let points = process_sparkline_points(&[], SIZE);
    assert_eq!(points.len(), 2, "empty → two-point baseline");
    let mid = SIZE.height * 0.5;
    assert_eq!(points[0], Point::new(0.0, mid), "left edge at the midpoint");
    assert_eq!(
        points[1],
        Point::new(SIZE.width, mid),
        "right edge at the midpoint"
    );
}

/// A single sample is still <2 → the same midpoint baseline (the readout
/// carries the value; the cell keeps its column).
#[test]
fn single_sample_is_the_midpoint_baseline() {
    let points = process_sparkline_points(&[42.0], SIZE);
    assert_eq!(points.len(), 2);
    let mid = SIZE.height * 0.5;
    assert!(points.iter().all(|point| (point.y - mid).abs() < 1e-3));
}

/// A multi-sample window auto-ranges to its OWN finite peak: the peak
/// sample rises to the top edge (within half a stroke) and the lowest
/// sample sits near the midpoint — the regression bait for a fixed 0–100
/// clamp that would flatten a low-CPU row.
#[test]
fn multi_sample_auto_ranges_to_the_peak_touching_the_top() {
    let points = process_sparkline_points(&[10.0, 50.0, 90.0], SIZE);
    assert_eq!(points.len(), 3, "one projected point per sample");
    let mid = SIZE.height * 0.5;
    let amp = (SIZE.height * 0.5 - STROKE_WIDTH * 0.5).max(0.0);
    // x spread evenly across the 48-wide frame: 0, 24, 48.
    assert_eq!(points[0].x, 0.0);
    assert_eq!(points[1].x, 24.0);
    assert_eq!(points[2].x, 48.0);
    // The peak (90, the last sample) reaches the top edge: y ≈ mid - amp.
    assert!(
        (points[2].y - (mid - amp)).abs() < 1e-3,
        "peak must touch the top, got {}",
        points[2].y
    );
    // The lowest sample (10, the first) sits closest to the midpoint, and
    // the mid sample (50) sits between them — the line has real slope.
    assert!(points[0].y > points[1].y && points[1].y > points[2].y);
    assert!(
        points[0].y < mid + 1e-3,
        "the lowest sample stays at or below the midpoint"
    );
}

/// An all-zero history does not divide by zero — the 1e-6 floor keeps every
/// point on the midpoint baseline (the honest idle state).
#[test]
fn all_zero_window_uses_the_floor_and_stays_on_the_baseline() {
    let points = process_sparkline_points(&[0.0, 0.0, 0.0], SIZE);
    let mid = SIZE.height * 0.5;
    assert!(
        points.iter().all(|point| (point.y - mid).abs() < 1e-3),
        "idle samples must sit on the midpoint baseline: {points:?}"
    );
}

/// The fingerprint combines pid and immutable snapshot generation.
#[test]
fn fingerprint_equality_keys_on_pid_and_snapshot_generation() {
    let samples: Rc<[f32]> = Rc::from([1.0, 2.0, 9.0].as_slice());
    let base = SparklineFingerprint::from_samples(7, &samples);
    assert_eq!(base, SparklineFingerprint::from_samples(7, &samples));
    assert_ne!(base, SparklineFingerprint::from_samples(8, &samples));
    let same_len_same_tail: Rc<[f32]> = Rc::from([8.0, 2.0, 9.0].as_slice());
    assert_ne!(
        base,
        SparklineFingerprint::from_samples(7, &same_len_same_tail),
        "a shifted history must invalidate even when len/tail agree"
    );
}

/// The program's `fingerprint()` mirrors `SparklineFingerprint::from_samples`
/// — the seam `draw()` keys the cache-clear gate on — so a process whose
/// history did not change reuses last frame's geometry, and a changed
/// history (or a different pid at the same tree position) rebuilds.
#[test]
fn program_fingerprint_tracks_pid_and_history() {
    let color = Color::WHITE;
    let samples: Rc<[f32]> = Rc::from([1.0, 2.0, 3.0].as_slice());
    let a = ProcessCpuSparkline::new(Rc::clone(&samples), color, 11);
    assert_eq!(
        a.fingerprint(),
        ProcessCpuSparkline::new(Rc::clone(&samples), color, 11).fingerprint()
    );
    let changed: Rc<[f32]> = Rc::from([9.0, 2.0, 3.0].as_slice());
    assert_ne!(
        a.fingerprint(),
        ProcessCpuSparkline::new(changed, color, 11).fingerprint()
    );
    // Same history but a different pid → different fingerprint (a reshuffled
    // row never reuses another process's cached geometry).
    assert_ne!(
        a.fingerprint(),
        ProcessCpuSparkline::new(Rc::clone(&samples), color, 12).fingerprint()
    );
    // Color is NOT part of the fingerprint — a theme switch is rare and a
    // stale-color frame on theme change is acceptable (matches the gpui
    // sparkline, which rebuilds its paint closure each render anyway).
    assert_eq!(
        a.fingerprint(),
        ProcessCpuSparkline::new(samples, Color::BLACK, 11).fingerprint()
    );
}
