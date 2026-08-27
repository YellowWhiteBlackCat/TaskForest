use super::*;
use std::rc::Rc;

/// Segment origins split the frame width evenly, oldest→newest left→right.
#[test]
fn segment_origins_are_evenly_spaced() {
    let size = Size::new(100.0, 40.0);
    let origins: Vec<Point> = (0..5).map(|i| segment_origin(i, 5, size)).collect();
    assert_eq!(
        origins,
        vec![
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(40.0, 0.0),
            Point::new(60.0, 0.0),
            Point::new(80.0, 0.0),
        ]
    );
}

/// A series with one sample yields no polyline (the caption still shows) —
/// the honest too-few-samples state.
#[test]
fn single_sample_has_no_polyline() {
    let runs = series_point_runs_for(&[50.0], Size::new(20.0, 30.0), 100.0);
    assert_eq!(runs[0].len(), 1);
    assert!(line_path(&runs[0]).is_none(), "one point must not stroke");
    let two = series_point_runs_for(&[50.0, 60.0], Size::new(20.0, 30.0), 100.0);
    assert!(line_path(&two[0]).is_some(), "two points must stroke");
}

/// A bytes/sec series auto-scaled to its own peak produces a real polyline
/// whose vertices are NOT all pinned at the ceiling — the regression bait
/// for the old 0–100 clamp that flattened disk/network traffic.
#[test]
fn scaled_bytes_series_is_not_a_flat_ceiling_line() {
    let size = Size::new(40.0, 30.0);
    let peak = 5_000_000.0; // 5 MB/s
    let runs = series_point_runs_for(&[1_000_000.0, peak], size, peak);
    let points = &runs[0];
    assert!(line_path(points).is_some());
    // The lower sample sits well above (larger y) the peak sample — the line
    // actually has slope, it is not clamped flat at y≈0.
    assert!(
        points[0].y > points[1].y + 5.0,
        "expected a rising line, got {:?}",
        points
    );
}

/// The strip fingerprint combines each immutable snapshot generation with its
/// auto-scale max.
#[test]
fn strip_fingerprint_keys_on_every_entry_generation_and_max() {
    let cpu: Rc<[f32]> = Rc::from([10.0, 50.0].as_slice());
    let memory: Rc<[f32]> = Rc::from([20.0, 40.0].as_slice());
    let stable = || {
        vec![
            entry_rc("CPU", Rc::clone(&cpu), 100.0),
            entry_rc("MEM", Rc::clone(&memory), 100.0),
        ]
    };
    let base = TrendStripFingerprint::from_entries(&stable());
    assert_eq!(base, TrendStripFingerprint::from_entries(&stable()));
    let shifted: Rc<[f32]> = Rc::from([99.0, 50.0].as_slice());
    assert_ne!(
        base,
        TrendStripFingerprint::from_entries(&[
            entry_rc("CPU", shifted, 100.0),
            entry_rc("MEM", Rc::clone(&memory), 100.0),
        ])
    );
    assert_ne!(
        base,
        TrendStripFingerprint::from_entries(&[
            entry_rc("CPU", Rc::clone(&cpu), 100.0),
            entry_rc("MEM", Rc::clone(&memory), 90.0),
        ]),
        "a max change must force a rebuild"
    );
    assert_ne!(
        base,
        TrendStripFingerprint::from_entries(&[entry_rc("CPU", cpu, 100.0)])
    );
}

/// The program's `fingerprint()` mirrors `TrendStripFingerprint::from_entries`
/// — the seam `draw()` keys the cache-clear gate on. Colors/captions are NOT
/// in the fingerprint: a theme switch is rare and one stale-color frame is
/// acceptable (matches round-1 process_sparkline; asserted here so it is
/// not "fixed" back).
#[test]
fn strip_program_fingerprint_tracks_data_not_color() {
    let white = Color::WHITE;
    let black = Color::BLACK;
    let cpu: Rc<[f32]> = Rc::from([10.0, 50.0].as_slice());
    let memory: Rc<[f32]> = Rc::from([20.0].as_slice());
    let strip = TrendStrip::new(
        vec![
            entry_rc("CPU", Rc::clone(&cpu), 100.0),
            entry_rc("MEM", Rc::clone(&memory), 100.0),
        ],
        white,
    );
    // Same data + same caption color → same fingerprint.
    assert_eq!(
        strip.fingerprint(),
        TrendStrip::new(
            vec![
                entry_rc("CPU", Rc::clone(&cpu), 100.0),
                entry_rc("MEM", Rc::clone(&memory), 100.0)
            ],
            white,
        )
        .fingerprint()
    );
    // Same data but different stroke/caption colors → SAME fingerprint.
    assert_eq!(
        strip.fingerprint(),
        TrendStrip::new(
            vec![
                entry_rc("CPU", Rc::clone(&cpu), 100.0),
                entry_rc("MEM", Rc::clone(&memory), 100.0)
            ],
            black,
        )
        .fingerprint()
    );
    // A tick on any entry → different fingerprint (cache clears).
    assert_ne!(
        strip.fingerprint(),
        TrendStrip::new(
            vec![
                entry("CPU", &[10.0, 60.0], 100.0),
                entry_rc("MEM", Rc::clone(&memory), 100.0)
            ],
            white,
        )
        .fingerprint()
    );
}

/// Build a trend entry for the fingerprint tests (caption is cosmetic for
/// the fingerprint — only the samples + max feed it).
fn entry(caption: &'static str, samples: &[f32], max: f32) -> TrendEntry {
    entry_rc(caption, Rc::from(samples.to_vec().into_boxed_slice()), max)
}

fn entry_rc(caption: &'static str, samples: Rc<[f32]>, max: f32) -> TrendEntry {
    TrendEntry {
        caption,
        samples,
        color: Color::WHITE,
        max,
    }
}
