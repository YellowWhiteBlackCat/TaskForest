use super::press_shadow;
use gpui::Pixels;

/// The pressed-state shadow is a single tight, faint drop (1px offset,
/// 2px blur) — a press reads as a sink, never a floating glow.
#[test]
fn press_shadow_is_tight_faint_and_black_derived() {
    let shadows = press_shadow();
    assert_eq!(shadows.len(), 1, "one drop, not a layered glow");
    let shadow = &shadows[0];
    assert_eq!(shadow.offset.x, Pixels::from(0.0));
    assert_eq!(shadow.offset.y, Pixels::from(1.0));
    assert_eq!(shadow.blur_radius, Pixels::from(2.0));
    assert!(shadow.color.a <= 0.1, "faint, never a glow");
}
