use super::{clamp_proc_col_width, clamp_sidebar_width, px};

/// 10px floor: anything below the floor is dropped (returns `None`), so a
/// stray drag towards zero can never collapse a column to an unusable sliver.
#[test]
fn clamp_floor_drops_below_min() {
    assert_eq!(clamp_proc_col_width(px(0.0), px(100.0)), None);
    assert_eq!(clamp_proc_col_width(px(9.9), px(100.0)), None);
    // Exactly the floor is accepted.
    assert_eq!(clamp_proc_col_width(px(10.0), px(100.0)), Some(px(10.0)));
}

/// 1200px ceiling: oversized candidates clamp down to the max, never above.
#[test]
fn clamp_ceiling_caps_at_max() {
    assert_eq!(
        clamp_proc_col_width(px(5000.0), px(100.0)),
        Some(px(1200.0))
    );
    // Exactly the ceiling passes through unchanged.
    assert_eq!(
        clamp_proc_col_width(px(1200.0), px(100.0)),
        Some(px(1200.0))
    );
}

/// <1px jitter is ignored: the candidate is floored to an integer px first
/// (matching the shared crate's `resize_cols`), then any change smaller than
/// 1px relative to the last committed width returns `None`. Since both the
/// stored width and the floored candidate are integers, this catches the
/// zero-delta case — a drag move that does not move a pixel boundary, so the
/// handler skips a redundant write + redraw.
#[test]
fn clamp_ignores_subpixel_jitter() {
    // Fractional inputs that floor to the SAME width as `old` → no-op.
    assert_eq!(clamp_proc_col_width(px(100.4), px(100.0)), None);
    assert_eq!(clamp_proc_col_width(px(100.6), px(100.0)), None);
    assert_eq!(clamp_proc_col_width(px(100.0), px(100.0)), None);
    // A full 1px change in either direction is applied (boundary is
    // exclusive on both sides: exactly ±1 is NOT jitter).
    assert_eq!(clamp_proc_col_width(px(101.0), px(100.0)), Some(px(101.0)));
    assert_eq!(clamp_proc_col_width(px(99.0), px(100.0)), Some(px(99.0)));
}

/// A normal in-range resize passes through (floor applied, ceiling not hit).
#[test]
fn clamp_passes_through_in_range() {
    assert_eq!(clamp_proc_col_width(px(250.0), px(100.0)), Some(px(250.0)));
    // Fractional input is floored to an integer px.
    assert_eq!(clamp_proc_col_width(px(250.7), px(100.0)), Some(px(250.0)));
}

/// Sidebar clamp mirrors the column clamp's three boundary classes: a 200px
/// floor (below it the drag is dropped), a 460px ceiling (oversized clamps
/// down), and the <1px jitter rule (sub-pixel noise is a no-op).
#[test]
fn clamp_sidebar_bounds_and_jitter() {
    use super::{SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH};
    assert_eq!(SIDEBAR_MIN_WIDTH, 200.0);
    assert_eq!(SIDEBAR_MAX_WIDTH, 460.0);
    // Below floor → dropped (a stray drag towards zero can't collapse it).
    assert_eq!(clamp_sidebar_width(px(150.0), px(260.0)), None);
    // At/above floor, in-range → passes through (floored).
    assert_eq!(clamp_sidebar_width(px(300.0), px(260.0)), Some(px(300.0)));
    assert_eq!(clamp_sidebar_width(px(300.7), px(260.0)), Some(px(300.0)));
    // Ceiling caps an absurd drag.
    assert_eq!(clamp_sidebar_width(px(5000.0), px(260.0)), Some(px(460.0)));
    // <1px jitter vs the last committed width → no-op.
    assert_eq!(clamp_sidebar_width(px(260.4), px(260.0)), None);
}
