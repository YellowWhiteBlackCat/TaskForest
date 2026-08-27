use super::*;
use crate::color::contrast_ratio;
use crate::fonts::{FONT_MISANS_VF, FONT_ROBOTO_MONO};

#[test]
fn theme_build_applies_resolved_fonts_and_setters_preserve_them() {
    let fonts = ResolvedFonts {
        ui: FONT_MISANS_VF,
        mono: FONT_ROBOTO_MONO,
    };
    let mut t = Theme::build(Skin::Kde, LightDark::Dark, HighContrast::Off, fonts);
    assert_eq!(t.ui_font, FONT_MISANS_VF);
    assert_eq!(t.mono_font, FONT_ROBOTO_MONO);

    // Skin/mode/HC rebuilds must not silently reset the user's font choice.
    t.set_skin(Skin::Windows);
    t.set_mode(LightDark::Light);
    t.set_high_contrast(true);
    assert_eq!(t.ui_font, FONT_MISANS_VF);
    assert_eq!(t.mono_font, FONT_ROBOTO_MONO);
}

#[test]
fn window_transparency_survives_recompose_and_defaults_off() {
    // Cold start: opaque until the platform edge opts into transparency.
    let t = Theme::dark();
    assert!(!t.window_transparent);

    // The platform edge (startup / RootView construction) flips the flag;
    // skin/mode/HC recomposition must keep it.
    let mut t = Theme::dark();
    t.window_transparent = true;
    t.set_skin(Skin::Windows);
    t.set_mode(LightDark::Light);
    t.toggle_mode();
    t.set_high_contrast(true);
    assert!(t.window_transparent, "recompose lost window transparency");
    assert_eq!(t.window_radius, 8.0);
}

#[test]
fn window_state_corner_policy_follows_maximize_fullscreen_and_tiling() {
    let corners = [
        WindowCorner::TopLeft,
        WindowCorner::TopRight,
        WindowCorner::BottomLeft,
        WindowCorner::BottomRight,
    ];
    let all = |s: WindowChromeState| corners.map(|c| s.corner_enabled(c));

    // Floating window: every corner rounded.
    let floating = WindowChromeState::default();
    assert_eq!(all(floating), [true, true, true, true]);

    // Maximized / fullscreen: every corner square.
    for state in [
        WindowChromeState {
            maximized: true,
            ..WindowChromeState::default()
        },
        WindowChromeState {
            fullscreen: true,
            ..WindowChromeState::default()
        },
        WindowChromeState {
            maximized: true,
            fullscreen: true,
            ..WindowChromeState::default()
        },
    ] {
        assert_eq!(all(state), [false, false, false, false]);
    }

    // Tiled edge suppresses exactly the corners touching that edge.
    let tiled_left = WindowChromeState {
        tiling: EdgeTiling {
            left: true,
            ..EdgeTiling::default()
        },
        ..WindowChromeState::default()
    };
    assert_eq!(all(tiled_left), [false, true, false, true]);

    let tiled_top = WindowChromeState {
        tiling: EdgeTiling {
            top: true,
            ..EdgeTiling::default()
        },
        ..WindowChromeState::default()
    };
    assert_eq!(all(tiled_top), [false, false, true, true]);

    // Top + left tiled (top-left screen corner, e.g. half-tiling).
    let tiled_corner = WindowChromeState {
        tiling: EdgeTiling {
            top: true,
            left: true,
            ..EdgeTiling::default()
        },
        ..WindowChromeState::default()
    };
    assert_eq!(all(tiled_corner), [false, false, false, true]);

    // Maximized wins over tiling facts.
    let maximized_tiled = WindowChromeState {
        maximized: true,
        tiling: EdgeTiling {
            left: true,
            right: true,
            ..EdgeTiling::default()
        },
        ..WindowChromeState::default()
    };
    assert_eq!(all(maximized_tiled), [false, false, false, false]);
}

#[test]
fn window_corner_radius_is_zero_off_transparent_or_under_suppressed_state() {
    let corners = [
        WindowCorner::TopLeft,
        WindowCorner::TopRight,
        WindowCorner::BottomLeft,
        WindowCorner::BottomRight,
    ];

    // Non-transparent surface (macOS/Windows): system rounds natively, the
    // app paints no window corner radius at all.
    let opaque = Theme::dark();
    assert!(
        corners
            .map(|c| opaque.window_corner_radius(c))
            .iter()
            .all(|r| *r == 0.0)
    );

    // Transparent CSD + floating: the per-skin radius everywhere.
    let mut csd = Theme::dark();
    csd.window_transparent = true;
    assert_eq!(csd.window_corner_radius(WindowCorner::TopLeft), 12.0);
    assert_eq!(csd.window_corner_radius(WindowCorner::BottomRight), 12.0);

    // Transparent CSD + maximized: no radius anywhere.
    csd.window_state = WindowChromeState {
        maximized: true,
        ..WindowChromeState::default()
    };
    assert!(
        corners
            .map(|c| csd.window_corner_radius(c))
            .iter()
            .all(|r| *r == 0.0)
    );

    // Transparent CSD + left-tiled: only the right corners keep the radius.
    csd.window_state = WindowChromeState {
        tiling: EdgeTiling {
            left: true,
            ..EdgeTiling::default()
        },
        ..WindowChromeState::default()
    };
    assert_eq!(csd.window_corner_radius(WindowCorner::TopLeft), 0.0);
    assert_eq!(csd.window_corner_radius(WindowCorner::BottomLeft), 0.0);
    assert_eq!(csd.window_corner_radius(WindowCorner::TopRight), 12.0);
    assert_eq!(csd.window_corner_radius(WindowCorner::BottomRight), 12.0);
}

#[test]
fn radius_scale_tiers_are_strictly_increasing_and_compat_fields_track_them() {
    for skin in Skin::ALL {
        for mode in [LightDark::Light, LightDark::Dark] {
            let t = Theme::build(
                skin,
                mode,
                HighContrast::Off,
                ResolvedFonts::system_for(skin),
            );
            // A gradient must be strictly increasing XSmall→XLarge, and the
            // radius must not depend on light/dark (it is a skin property).
            let mut prev = -1.0;
            for tier in RadiusScale::ALL {
                let r = t.radius(tier);
                assert!(
                    r > prev,
                    "{} {} {tier:?} = {r} must be strictly increasing",
                    skin.label(),
                    mode.label(),
                );
                prev = r;
            }
            // The legacy compat fields mirror the tiers they replaced, so
            // renderers still reading raw fields cannot diverge from the
            // scale-based call sites.
            assert_eq!(t.control_radius, t.radius(RadiusScale::Medium));
            assert_eq!(t.card_radius, t.radius(RadiusScale::Large));
            assert_eq!(t.window_radius, t.radius(RadiusScale::XLarge));
        }
    }
}

#[test]
fn window_corner_radius_keeps_legacy_per_skin_values() {
    // The CSD behavior is pinned: a transparent floating window resolves
    // to the skin's window radius (gnome 12, kde 6, win 8, mac 10), now
    // served by the XLarge tier of the radius scale.
    let expected = |skin: Skin| match skin {
        Skin::Gnome => 12.0,
        Skin::Kde => 6.0,
        Skin::Windows => 8.0,
        Skin::Macos => 10.0,
    };
    for skin in Skin::ALL {
        let mut t = Theme::build(
            skin,
            LightDark::Dark,
            HighContrast::Off,
            ResolvedFonts::system_for(skin),
        );
        t.window_transparent = true;
        assert_eq!(
            t.window_corner_radius(WindowCorner::TopLeft),
            expected(skin),
            "{} window corner radius drifted from its legacy value",
            skin.label(),
        );
    }
}

#[test]
fn background_appearance_tracks_material_and_window_transparency() {
    // The gpui-mapped decision is a feature-gated concern (ADR-026);
    // this neutral test pins the INPUT facts the binding reads:
    // material axis + the CSD transparency flag survive composition.
    let opaque = Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    );
    assert_eq!(opaque.material, Material::Opaque);
    assert!(!opaque.window_transparent);

    let mut linux_csd = Theme::dark();
    linux_csd.window_transparent = true;
    assert!(linux_csd.window_transparent);

    for skin in [Skin::Windows, Skin::Macos] {
        let t = Theme::build(
            skin,
            LightDark::Dark,
            HighContrast::Off,
            ResolvedFonts::system_for(skin),
        );
        assert_ne!(t.material, Material::Opaque);
    }
}

#[test]
fn background_appearance_skips_transparency_flag_on_material_skins() {
    // Same neutral input facts: a Mica/Vibrancy skin keeps priority over
    // the CSD transparency flag (the adapter maps it to Blurred).
    let mut win = Theme::build(
        Skin::Windows,
        LightDark::Light,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Windows),
    );
    win.window_transparent = true;
    assert_ne!(win.material, Material::Opaque);
    assert!(win.window_transparent);
}

#[test]
fn detect_resolves_axes_from_native_facts() {
    // Fully observed facts map straight through.
    let appearance = NativeAppearance {
        family: Some(Skin::Kde),
        scheme: Some(LightDark::Dark),
        high_contrast: Some(true),
    };
    let t = Theme::detect(appearance);
    assert_eq!(t.skin, Skin::Kde);
    assert!(t.dark);
    assert!(t.hc);

    // Unknown facts fall back to GNOME light, no high contrast.
    let t = Theme::detect(NativeAppearance::default());
    assert_eq!(t.skin, Skin::Gnome);
    assert!(!t.dark);
    assert!(!t.hc);
}

/// `card_surface` = the verbatim `card_bg` whenever the skin table already
/// distinguishes cards from the view surface; otherwise (card == view) the
/// dark modes derive a lift toward the foreground and light modes blend
/// toward white. Always opaque, always strictly above the view surface.
#[test]
fn card_surface_lifts_flat_skins_and_keeps_verbatim_tables() {
    for skin in Skin::ALL {
        for mode in [LightDark::Light, LightDark::Dark] {
            let t = Theme::build(
                skin,
                mode,
                HighContrast::Off,
                ResolvedFonts::system_for(skin),
            );
            let surface = t.card_surface();
            assert_eq!(
                surface.a,
                1.0,
                "{} {} card surface must stay opaque",
                skin.label(),
                mode.label(),
            );
            if t.card_bg != t.view_bg {
                // Verbatim tables (GNOME, light KDE/macOS): the skin's own
                // card token wins untouched.
                assert_eq!(
                    surface,
                    t.card_bg,
                    "{} {} must keep the verbatim card_bg",
                    skin.label(),
                    mode.label(),
                );
            } else {
                // Flat tables (dark KDE/Windows/macOS, light Windows): the
                // derived surface must read strictly lighter than the
                // backdrop in dark (channel-wise sum). Light tables whose
                // card is already pure white (GNOME light) cannot lift —
                // they keep white and rely on the border.
                let surface_sum = surface.r + surface.g + surface.b;
                let view_sum = t.view_bg.r + t.view_bg.g + t.view_bg.b;
                if mode.is_dark() {
                    assert!(
                        surface_sum > view_sum,
                        "{} {} dark card surface must be lighter than the view",
                        skin.label(),
                        mode.label(),
                    );
                } else {
                    assert!(
                        surface_sum >= view_sum,
                        "{} {} light card surface must not be darker than the view",
                        skin.label(),
                        mode.label(),
                    );
                }
            }
        }
    }
}

/// The selection tint backs the accent rail: translucent (never opaque),
/// strictly stronger than the hover tint so selected rows still read as
/// the top state, and hue-locked to the accent.
#[test]
fn selection_tint_stays_gentle_and_above_hover() {
    for skin in Skin::ALL {
        for mode in [LightDark::Light, LightDark::Dark] {
            let t = Theme::build(
                skin,
                mode,
                HighContrast::Off,
                ResolvedFonts::system_for(skin),
            );
            let selection = t.selection_bg();
            assert!(
                selection.a < 1.0 && selection.a > t.hover_bg().a,
                "{} {} selection (a={}) must be translucent and above hover",
                skin.label(),
                mode.label(),
                selection.a,
            );
            assert_eq!(
                (selection.r, selection.g, selection.b),
                (t.accent.r, t.accent.g, t.accent.b),
                "{} {} selection must keep the accent hue",
                skin.label(),
                mode.label(),
            );
        }
    }
}

/// The card-shadow ink is locked per mode for all 8 variants: dark skins
/// cast pure translucent BLACK (alpha 0.45), light skins a 55%-darkened
/// `shade` at alpha 0.40 — always translucent, never the border hue.
#[test]
fn card_shadow_ink_is_mode_locked_and_never_opaque() {
    for skin in Skin::ALL {
        for mode in [LightDark::Light, LightDark::Dark] {
            let t = Theme::build(
                skin,
                mode,
                HighContrast::Off,
                ResolvedFonts::system_for(skin),
            );
            let shadow = t.card_shadow();
            assert!(
                shadow.a > 0.0 && shadow.a < 1.0,
                "{} {} card shadow must be translucent (a={})",
                skin.label(),
                mode.label(),
                shadow.a,
            );
            if mode.is_dark() {
                // Dark: locked black ink at 45%.
                assert_eq!(
                    (shadow.r, shadow.g, shadow.b),
                    (0.0, 0.0, 0.0),
                    "{} dark card shadow must stay pure black",
                    skin.label(),
                );
                assert!(
                    (shadow.a - 0.45).abs() < 1e-4,
                    "{} dark card shadow alpha must be locked at 0.45",
                    skin.label(),
                );
            } else {
                // Light: shade darkened 55% toward black, locked 40% alpha —
                // channel-exact, so the grey ink cannot drift toward the
                // border hue or pure black.
                let darken = 0.55;
                assert!(
                    (shadow.a - 0.4).abs() < 1e-4,
                    "{} light card shadow alpha must be locked at 0.4",
                    skin.label(),
                );
                for (channel, shade_channel) in [
                    (shadow.r, t.shade.r),
                    (shadow.g, t.shade.g),
                    (shadow.b, t.shade.b),
                ] {
                    assert!(
                        (channel - shade_channel * (1.0 - darken)).abs() < 1e-4,
                        "{} light card shadow must be the shade darkened by {darken}",
                        skin.label(),
                    );
                }
                assert_ne!(
                    shadow.with_alpha(1.0),
                    t.border,
                    "{} light card shadow must never collapse into the border token",
                    skin.label(),
                );
            }
        }
    }
}

/// The accent gradient brackets the accent in every variant:
/// `gradient_from` is strictly lighter (each channel >= accent, at least
/// one strictly), `gradient_to` strictly darker, and both keep the
/// accent's own alpha.
#[test]
fn gradient_stops_bracket_the_accent_in_every_variant() {
    for skin in Skin::ALL {
        for mode in [LightDark::Light, LightDark::Dark] {
            let t = Theme::build(
                skin,
                mode,
                HighContrast::Off,
                ResolvedFonts::system_for(skin),
            );
            let from = t.gradient_from();
            let to = t.gradient_to();
            assert_eq!(
                from.a,
                t.accent.a,
                "{} {} gradient_from alpha",
                skin.label(),
                mode.label()
            );
            assert_eq!(
                to.a,
                t.accent.a,
                "{} {} gradient_to alpha",
                skin.label(),
                mode.label()
            );
            assert!(
                from.r >= t.accent.r && from.g >= t.accent.g && from.b >= t.accent.b,
                "{} {} gradient_from must be lighter than the accent",
                skin.label(),
                mode.label(),
            );
            assert!(
                to.r <= t.accent.r && to.g <= t.accent.g && to.b <= t.accent.b,
                "{} {} gradient_to must be darker than the accent",
                skin.label(),
                mode.label(),
            );
            assert_ne!(
                from,
                t.accent,
                "{} {} gradient_from must differ",
                skin.label(),
                mode.label()
            );
            assert_ne!(
                to,
                t.accent,
                "{} {} gradient_to must differ",
                skin.label(),
                mode.label()
            );
            assert_ne!(
                from,
                to,
                "{} {} stops must differ",
                skin.label(),
                mode.label()
            );
        }
    }
}

#[test]
fn product_color_modes_are_primary_over_native_skin_variations() {
    for mode in LightDark::ALL {
        let themes: Vec<_> = Skin::ALL
            .into_iter()
            .map(|skin| {
                Theme::build(
                    skin,
                    mode,
                    HighContrast::Off,
                    ResolvedFonts::system_for(skin),
                )
            })
            .collect();
        let first = themes[0];
        assert!(
            themes.iter().all(|theme| theme.window_bg == first.window_bg
                && theme.view_bg == first.view_bg
                && theme.card_bg == first.card_bg
                && theme.accent == first.accent),
            "{} product colors must not be replaced by native skin colors",
            mode.label()
        );
        assert!(
            contrast_ratio(first.fg, first.card_surface()) >= 4.5,
            "{} primary text must remain WCAG-readable on the card surface",
            mode.label()
        );
        assert!(
            contrast_ratio(first.accent, first.accent_text) >= 4.5,
            "{} accent controls must remain WCAG-readable",
            mode.label()
        );
    }

    let light = Theme::build(
        Skin::Gnome,
        LightDark::Light,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    );
    let dark = Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    );
    let forest = Theme::build(
        Skin::Gnome,
        LightDark::EyeForest,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    );
    assert_ne!(light.window_bg, dark.window_bg);
    assert_ne!(light.window_bg, forest.window_bg);
    assert_ne!(dark.window_bg, forest.window_bg);
    assert_ne!(
        Theme::build(
            Skin::Gnome,
            LightDark::Light,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        )
        .radius_scale,
        Theme::build(
            Skin::Kde,
            LightDark::Light,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Kde),
        )
        .radius_scale,
        "native skin geometry remains a secondary, visible adaptation"
    );
}
