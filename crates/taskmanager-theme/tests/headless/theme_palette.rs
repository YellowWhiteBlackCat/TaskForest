use super::*;
use crate::fonts::ResolvedFonts;
use crate::theme::{HighContrast, LightDark, Skin};

fn variants() -> impl Iterator<Item = (Skin, LightDark, HighContrast)> {
    Skin::ALL.into_iter().flat_map(|skin| {
        [LightDark::Light, LightDark::Dark]
            .into_iter()
            .flat_map(move |mode| {
                [HighContrast::Off, HighContrast::On]
                    .into_iter()
                    .map(move |hc| (skin, mode, hc))
            })
    })
}

/// The upstream lesson pinned: `window_backdrop` can be transparent while
/// `surface` never is — and the two are independent tokens (different
/// values), never the same source.
#[test]
fn window_backdrop_is_transparent_capable_and_surface_never_is() {
    for (skin, mode, hc) in variants() {
        let mut theme = Theme::build(skin, mode, hc, ResolvedFonts::system_for(skin));
        // Opaque surface by default.
        let palette = theme.palette();
        assert_eq!(
            palette.surface.a,
            1.0,
            "{} {} {} surface must be opaque",
            skin.label(),
            mode.label(),
            if hc == HighContrast::On {
                "HC"
            } else {
                "normal"
            },
        );
        assert_eq!(palette.window_backdrop.a, theme.window_bg.a);

        // Linux CSD: the backdrop goes fully transparent, the surface
        // stays opaque, and the tokens never collapse into one source.
        theme.window_transparent = true;
        let palette = theme.palette();
        assert_eq!(
            palette.window_backdrop.a,
            0.0,
            "{} {} CSD backdrop must be transparent",
            skin.label(),
            mode.label(),
        );
        assert_eq!(palette.surface.a, 1.0);
        assert_ne!(
            palette.window_backdrop,
            palette.surface,
            "{} {} backdrop and surface must be independent tokens",
            skin.label(),
            mode.label(),
        );
    }
}

/// `ring` carries the per-frame focus-visible decision in its alpha: 0 for
/// pointer/programmatic renders, full accent for keyboard renders.
#[test]
fn ring_alpha_tracks_focus_visible() {
    for (skin, mode, hc) in variants() {
        let theme = Theme::build(skin, mode, hc, ResolvedFonts::system_for(skin));
        let focused = theme.with_focus_visible(true).palette();
        let unfocused = theme.with_focus_visible(false).palette();
        assert_eq!(
            focused.ring.a,
            1.0,
            "{} {} keyboard render must paint an opaque focus ring",
            skin.label(),
            mode.label(),
        );
        assert_eq!(
            unfocused.ring.a,
            0.0,
            "{} {} non-keyboard render must suppress the focus ring",
            skin.label(),
            mode.label(),
        );
        assert_eq!(focused.ring.r, unfocused.ring.r);
        assert_eq!(focused.ring.g, unfocused.ring.g);
        assert_eq!(focused.ring.b, unfocused.ring.b);
    }
}

/// High-contrast keeps the palette readable: solid max-contrast borders,
/// dim text pulled toward the foreground, surfaces still opaque.
#[test]
fn high_contrast_keeps_palette_readable() {
    for (skin, mode) in Skin::ALL.into_iter().flat_map(|skin| {
        [LightDark::Light, LightDark::Dark]
            .into_iter()
            .map(move |mode| (skin, mode))
    }) {
        let theme = Theme::build(
            skin,
            mode,
            HighContrast::On,
            ResolvedFonts::system_for(skin),
        );
        let palette = theme.palette();
        assert_eq!(
            palette.border,
            if mode.is_dark() {
                Color::WHITE
            } else {
                Color::BLACK
            },
            "{} {} HC border must be solid max-contrast",
            skin.label(),
            mode.label(),
        );
        assert!(
            palette.fg_muted.a >= 0.9,
            "{} {} HC muted text must be near-full opacity",
            skin.label(),
            mode.label(),
        );
        assert_eq!(palette.surface.a, 1.0);
    }
}

/// The radius contract: panel / small / xsmall resolve from the skin's
/// gradient (strictly increasing, skin-distinct), so UI components that
/// round through `palette` stay platform-idiomatic per skin.
#[test]
fn radius_tokens_track_the_skin_gradient() {
    for (skin, mode, hc) in variants() {
        let theme = Theme::build(skin, mode, hc, ResolvedFonts::system_for(skin));
        let palette = theme.palette();
        let (panel, small, xsmall) = (
            f32::from(palette.panel_radius),
            f32::from(palette.small_radius),
            f32::from(palette.xsmall_radius),
        );
        assert_eq!(
            f32::from(palette.control_radius),
            theme.radius(RadiusScale::Medium)
        );
        assert_eq!(panel, theme.radius(RadiusScale::Large));
        assert_eq!(small, theme.radius(RadiusScale::Small));
        assert_eq!(xsmall, theme.radius(RadiusScale::XSmall));
        assert!(
            xsmall < small && small < panel,
            "{} {} radius gradient must be strictly increasing",
            skin.label(),
            mode.label(),
        );
    }
}

/// The derived surface tokens are translucent tints of the theme's own
/// hues: selection > hover in strength, zebra nearly invisible, and all
/// three keep the accent/fg hue they derive from — so rows/controls read
/// the same token the views use (single source, no per-view alphas).
#[test]
fn derived_surface_tokens_keep_hue_and_strength_order() {
    for (skin, mode, hc) in variants() {
        let theme = Theme::build(skin, mode, hc, ResolvedFonts::system_for(skin));
        let palette = theme.palette();
        assert_eq!(palette.selection, theme.selection_bg());
        assert_eq!(palette.hover, theme.hover_bg());
        assert_eq!(palette.highlight_fg, theme.highlight_fg());
        // Selection is the strongest tint, hover fainter, zebra faintest —
        // and all are translucent (never opaque).
        assert!(
            palette.selection.a > palette.hover.a && palette.hover.a > theme.zebra_bg().a,
            "{} {} surface strength order must be selection > hover > zebra",
            skin.label(),
            mode.label(),
        );
        assert!(
            palette.selection.a < 1.0,
            "{} {} selection must stay a translucent tint",
            skin.label(),
            mode.label(),
        );
        // Tints keep the hue of the token they derive from.
        assert_eq!(
            (
                palette.selection.r,
                palette.selection.g,
                palette.selection.b
            ),
            (palette.accent.r, palette.accent.g, palette.accent.b)
        );
        assert_eq!(
            (palette.hover.r, palette.hover.g, palette.hover.b),
            (palette.accent.r, palette.accent.g, palette.accent.b)
        );
        // The search highlight resolves to the accent hue too.
        assert_eq!(palette.highlight_fg, palette.accent);
    }
}

/// The shadow + gradient tokens flow verbatim from the theme: components
/// that read `Palette::card_shadow` / `gradient_from` / `gradient_to`
/// can never drift from the skin-derived values.
#[test]
fn shadow_and_gradient_tokens_flow_into_the_palette() {
    for (skin, mode, hc) in variants() {
        let theme = Theme::build(skin, mode, hc, ResolvedFonts::system_for(skin));
        let palette = theme.palette();
        assert_eq!(
            palette.card_shadow,
            theme.card_shadow(),
            "{} {} palette card_shadow must copy the theme token",
            skin.label(),
            mode.label(),
        );
        assert_eq!(
            palette.gradient_from,
            theme.gradient_from(),
            "{} {} palette gradient_from must copy the theme token",
            skin.label(),
            mode.label(),
        );
        assert_eq!(
            palette.gradient_to,
            theme.gradient_to(),
            "{} {} palette gradient_to must copy the theme token",
            skin.label(),
            mode.label(),
        );
    }
}
