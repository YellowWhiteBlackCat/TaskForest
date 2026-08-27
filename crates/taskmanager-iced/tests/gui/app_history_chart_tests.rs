use super::*;

/// The fingerprint is one immutable series generation: the same `Rc` is an
/// idle-frame hit, while any new snapshot invalidates even when length and tail
/// are unchanged.
#[test]
fn fingerprint_equality_keys_on_snapshot_generation() {
    let samples: Rc<[f32]> = Rc::from([1.0, 2.0, 9.0].as_slice());
    let base = AppSparkFingerprint::from_samples(&samples);
    assert_eq!(base, AppSparkFingerprint::from_samples(&samples));
    let same_len_same_tail: Rc<[f32]> = Rc::from([8.0, 2.0, 9.0].as_slice());
    assert_ne!(
        base,
        AppSparkFingerprint::from_samples(&same_len_same_tail),
        "a changed middle sample must invalidate the geometry"
    );
}

/// The program's `fingerprint()` mirrors `AppSparkFingerprint::from_samples`
/// — the seam `draw()` keys the cache-clear gate on — so an app whose
/// history did not change reuses last frame's geometry, and a tick that
/// moved the last sample rebuilds.
#[test]
fn program_fingerprint_tracks_history() {
    let color = Color::WHITE;
    let samples: Rc<[f32]> = Rc::from([1.0, 2.0, 3.0].as_slice());
    let a = Sparkline::new(Rc::clone(&samples), color);
    assert_eq!(
        a.fingerprint(),
        Sparkline::new(Rc::clone(&samples), color).fingerprint()
    );
    let changed: Rc<[f32]> = Rc::from([9.0, 2.0, 3.0].as_slice());
    assert_ne!(
        a.fingerprint(),
        Sparkline::new(changed, color).fingerprint()
    );
    // Color is NOT part of the fingerprint — a theme switch is rare and a
    // stale-color frame on theme change is acceptable (matches round-1
    // process_sparkline; explicit assertion here so it is not "fixed" back).
    assert_eq!(
        a.fingerprint(),
        Sparkline::new(samples, Color::BLACK).fingerprint()
    );
}
