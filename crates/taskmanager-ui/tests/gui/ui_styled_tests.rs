use super::{active_fill, blend, disabled_fg, focus_ring_refinement, hover_fill, scrim};
use gpui::Hsla;
use taskmanager_theme::color::{contrast_ratio, on_accent};
use taskmanager_theme::{Color, Theme};

fn palette() -> taskmanager_theme::Palette {
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
        contrast_ratio(accent, fg) >= 3.0,
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

#[test]
fn focus_ring_uses_palette_ring() {
    let p = palette();
    let refinement = focus_ring_refinement(&p);
    let ring_color: Hsla = crate::theme_binding::hsla(p.ring);
    assert_eq!(refinement.border_color, Some(ring_color));
}

#[test]
fn scrim_is_derived_from_surface() {
    let p = palette();
    let s = scrim(&p, 0.5);
    assert_eq!(s.r, p.surface.r);
    assert!((s.a - 0.5).abs() < 1e-4);
    // Alpha clamps.
    assert_eq!(scrim(&p, 2.0).a, 1.0);
    assert_eq!(scrim(&p, -1.0).a, 0.0);
}
