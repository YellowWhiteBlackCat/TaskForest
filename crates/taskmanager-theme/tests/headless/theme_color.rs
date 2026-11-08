use super::*;
use zhuge_ui_tokens::{Rgba, contrast_ratio};

fn product_contrast_ratio(first: Color, second: Color) -> f32 {
    contrast_ratio(
        Rgba::new(first.r, first.g, first.b, first.a),
        Rgba::new(second.r, second.g, second.b, second.a),
    )
}

#[test]
fn from_hex_matches_the_legacy_rgb_semantics() {
    let c = Color::from_hex(0x222226);
    assert_eq!(c.r, 0x22 as f32 / 255.0);
    assert_eq!(c.g, 0x22 as f32 / 255.0);
    assert_eq!(c.b, 0x26 as f32 / 255.0);
    assert_eq!(c.a, 1.0);
    assert!(c.is_opaque());
    assert_eq!(c.to_srgb8(), [0x22, 0x22, 0x26]);
}

#[test]
fn with_alpha_preserves_channels_and_sets_alpha() {
    let c = Color::from_hex(0x3584e4).with_alpha(0.55);
    assert_eq!(c.a, 0.55);
    assert_eq!(c.to_srgb8(), [0x35, 0x84, 0xe4]);
    assert!(!c.is_opaque());
    assert_eq!(Color::TRANSPARENT.a, 0.0);
}

#[test]
fn black_and_white_consts_are_pure() {
    assert_eq!(Color::BLACK.to_srgb8(), [0, 0, 0]);
    assert_eq!(Color::WHITE.to_srgb8(), [255, 255, 255]);
    assert!(Color::BLACK.is_opaque());
}

#[test]
fn scalar_tokens_convert_to_f32() {
    assert_eq!(f32::from(Length(8.0)), 8.0);
    assert_eq!(f32::from(Ratio(1.4)), 1.4);
    assert_eq!(f32::from(Weight(450.0)), 450.0);
    assert_eq!(Length(4.0), Length(4.0));
    assert!(Length(2.0) < Length(4.0));
}

#[test]
fn contrast_and_accent_math_are_wcag_correct() {
    // White-on-black is the 21:1 ceiling (within float tolerance).
    assert!((product_contrast_ratio(Color::WHITE, Color::BLACK) - 21.0).abs() < 1e-3);
    // Identical colors are 1:1.
    assert!((product_contrast_ratio(Color::BLACK, Color::BLACK) - 1.0).abs() < 1e-6);
    // on_accent picks the max-contrast foreground.
    assert_eq!(on_accent(Color::BLACK), Color::WHITE);
    assert_eq!(on_accent(Color::WHITE), Color::BLACK);
}

#[test]
fn mix_interpolates_endpoints_and_midpoint() {
    let a = Color::BLACK;
    let b = Color::WHITE;
    assert_eq!(mix(a, b, 0.0), a);
    assert_eq!(mix(a, b, 1.0), b);
    let half = mix(a, b, 0.5);
    for channel in [half.r, half.g, half.b] {
        assert!((channel - 0.5).abs() < 1e-6);
    }
    // Out-of-range factors clamp, never invert.
    assert_eq!(mix(a, b, -1.0), a);
    assert_eq!(mix(a, b, 2.0), b);
}

#[test]
fn mix_blends_on_the_owner_formula_channel_exactly() {
    // Dyadic channels make every step exact in f32, so a blend that moves to
    // another space (gamma, premultiplied, per-channel rounding) fails here
    // instead of drifting silently away from the token owner's `Rgba::lerp`.
    let a = Color::new(0.0, 0.25, 0.5, 1.0);
    let b = Color::new(0.75, 0.0, 1.0, 0.5);
    assert_eq!(mix(a, b, 0.25), Color::new(0.1875, 0.1875, 0.625, 0.875));
    assert_eq!(mix(a, b, 0.5), Color::new(0.375, 0.125, 0.75, 0.75));
    assert_eq!(mix(a, b, 0.75), Color::new(0.5625, 0.0625, 0.875, 0.625));
}
