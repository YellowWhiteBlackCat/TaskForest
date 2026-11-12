use super::{active_fill, blend, disabled_fg, disabled_opacity, hover_fill, scrim};
use crate::primitives::card_surface::CardSurface;
use taskmanager_theme::Palette;
use taskmanager_theme::color::on_accent;
use taskmanager_theme::{Color, Theme};
use zhuge_ui_tokens::Rgba;

fn palette() -> Palette {
    Theme::dark().palette()
}

#[test]
fn blend_interpolates_channels_and_clamps_t() {
    let a = Color::BLACK;
    let b = Color::new(1.0, 0.5, 0.0, 1.0);
    let mid = blend(a, b, 0.5);
    assert!((mid.r - 0.5).abs() < 1e-4);
    assert!((mid.g - 0.25).abs() < 1e-4);
    // Out-of-range t clamps instead of extrapolating.
    assert_eq!(blend(a, b, -1.0), a);
    assert_eq!(blend(a, b, 2.0), b);
}

#[test]
fn on_accent_picks_max_contrast_black_or_white() {
    let accent = palette().accent;
    let fg = on_accent(accent);
    assert!(fg == Color::BLACK || fg == Color::WHITE);
    assert!(
        Rgba::new(accent.r, accent.g, accent.b, accent.a)
            .contrast_ratio(Rgba::new(fg.r, fg.g, fg.b, fg.a,))
            >= 3.0,
        "accent text must be readable"
    );
}

#[test]
fn hover_and_active_move_fill_toward_distinct_states() {
    let base = palette().accent;
    let hover = hover_fill(base);
    let active = active_fill(base);
    assert_ne!(hover, base);
    assert_ne!(active, base);
    assert_ne!(hover, active);
}

#[test]
fn disabled_fg_is_muted_relative_to_fg() {
    let p = palette();
    let d = disabled_fg(&p);
    assert_ne!(d, p.fg);
}

/// The disabled weight is one shared fact: every control that mutes itself
/// reads this helper, so an alpha change cannot silently split the family.
#[test]
fn disabled_opacity_is_the_single_control_weight() {
    assert_eq!(disabled_opacity(), 0.5);
}

#[test]
fn scrim_is_derived_from_the_glass_depth_token() {
    let p = palette();
    let s = scrim(&p, 0.5);
    let depth = p.theme.glass_scrim().fill;
    assert_eq!((s.r, s.g, s.b), (depth.r, depth.g, depth.b));
    assert!((s.a - 0.5).abs() < 1e-4);
    // Alpha clamps.
    assert_eq!(scrim(&p, 2.0).a, 1.0);
    assert_eq!(scrim(&p, -1.0).a, 0.0);
}

#[test]
fn explicit_card_fill_is_opaque_and_default_card_fill_is_glass() {
    let p = palette();
    let glass = CardSurface::new(p);
    assert_eq!(glass.fill(&p), p.theme.glass_card().fill);

    let custom = CardSurface::new(p).background(p.surface);
    assert_eq!(custom.fill(&p), p.surface);
    assert_eq!(custom.fill(&p).a, 1.0);
}
