//! test-intent: behavior
//!
//! Headless pure-function tests for the theme-token → bevy adapter.
//!
//! The oracle is the theme crate itself: every resolved bevy value must equal
//! the neutral token it claims to adapt (channel-exact colors, token sizes,
//! token weights, token radius). A mutation that hardcodes a bevy default or
//! reinterprets the token scale fails here without any window.

use bevy::color::Srgba;
use bevy::text::FontSize;
use taskmanager_theme::tokens::{self, UiSize};
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

use super::{UiPalette, theme_color, ui_palette};

/// The light-mode counterpart of `Theme::dark()`'s construction: same skin,
/// fonts and contrast, opposite mode.
fn light_theme() -> Theme {
    Theme::build(
        Skin::Gnome,
        LightDark::Light,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    )
}

fn assert_same_color(bevy_color: bevy::color::Color, token: taskmanager_theme::Color) {
    let Srgba {
        red,
        green,
        blue,
        alpha,
    } = bevy_color.to_srgba();
    assert_eq!(
        (red, green, blue, alpha),
        (token.r, token.g, token.b, token.a)
    );
}

#[test]
fn token_colors_round_trip_channel_exact() {
    let dark = Theme::dark();
    let light = light_theme();
    for theme in [&dark, &light] {
        for token in [
            theme.window_bg,
            theme.fg,
            theme.fg_dim,
            theme.accent,
            theme.card_surface(),
            theme.view_bg,
            theme.sidebar_bg,
            theme.sidebar_card_bg,
        ] {
            assert_same_color(theme_color(token), token);
        }
    }
}

fn px(size: FontSize) -> f32 {
    size.eval(bevy::math::Vec2::ZERO, 16.0)
}

#[test]
fn ui_palette_derives_every_metric_from_tokens() {
    let theme = Theme::dark();
    let palette: UiPalette = ui_palette(&theme);
    assert_same_color(palette.window_clear, theme.window_bg);
    assert_same_color(palette.panel_fill, theme.card_surface());
    assert_same_color(palette.content_bg, theme.view_bg);
    assert_same_color(palette.nav_bg, theme.sidebar_bg);
    assert_same_color(palette.nav_active_bg, theme.sidebar_card_bg);
    assert_same_color(palette.hover_bg, theme.hover_bg());
    assert_same_color(palette.selection_bg, theme.selection_bg());
    assert_same_color(palette.accent, theme.accent);
    assert_same_color(palette.heading_color, theme.fg);
    assert_same_color(palette.body_color, theme.fg);
    assert_same_color(palette.dim_color, theme.fg_dim);
    assert_eq!(
        palette.panel_radius_px,
        tokens::card_radius(&theme).0,
        "the card corner radius comes from the theme radius scale"
    );
    assert_eq!(
        palette.control_radius_px,
        tokens::control_radius(&theme).0,
        "the control corner radius comes from the theme radius scale"
    );
    assert_eq!(
        palette.control_height_px,
        UiSize::Standard.control_height().0,
        "the control height is the Standard density token"
    );
    assert_eq!(
        px(palette.heading.font_size),
        UiSize::Standard.page_title_font_size().0,
        "the heading size is the Standard page-title token"
    );
    assert_eq!(
        px(palette.body.font_size),
        f32::from(tokens::FONT_BODY),
        "the body size is the body baseline token"
    );
    assert_eq!(
        px(palette.caption.font_size),
        f32::from(tokens::FONT_CAPTION),
        "the caption size is the caption baseline token"
    );
    assert_eq!(
        px(palette.mono.font_size),
        f32::from(tokens::FONT_BODY),
        "the mono role keeps the metric text baseline size"
    );
    assert_eq!(palette.heading.weight.0, 600, "header weight token");
    assert_eq!(palette.body.weight.0, 400, "normal weight token");
    assert_eq!(palette.caption.weight.0, 400, "normal weight token");
    assert_eq!(palette.mono.weight.0, 400, "mono weight token");
}

#[test]
fn light_and_dark_themes_resolve_to_different_surfaces() {
    let dark = ui_palette(&Theme::dark());
    let light = ui_palette(&light_theme());
    assert_ne!(
        dark.window_clear.to_srgba(),
        light.window_clear.to_srgba(),
        "the adapter must respect the theme mode, not bake one skin"
    );
    assert_ne!(
        dark.dim_color.to_srgba(),
        light.dim_color.to_srgba(),
        "ink tokens follow the mode too"
    );
    assert_ne!(
        dark.nav_bg.to_srgba(),
        light.nav_bg.to_srgba(),
        "nav surfaces follow the mode too"
    );
}

#[test]
fn high_contrast_theme_strengthens_bevy_palette_and_flags_hc() {
    let standard = ui_palette(&Theme::dark());
    let hc_theme = Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::On,
        ResolvedFonts::system_for(Skin::Gnome),
    );
    let hc = ui_palette(&hc_theme);
    assert!(!standard.high_contrast);
    assert!(hc.high_contrast);
    assert_ne!(standard.border_color.to_srgba(), hc.border_color.to_srgba());
    assert_eq!(hc.border_color.to_srgba().alpha, 1.0);
}
