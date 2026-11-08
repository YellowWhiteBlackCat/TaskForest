use super::*;
use crate::fonts::ResolvedFonts;
use crate::theme::HighContrast;

use zhuge_ui_tokens::Rgba;

fn contrast_ratio(first: Color, second: Color) -> f32 {
    Rgba::new(first.r, first.g, first.b, first.a)
        .contrast_ratio(Rgba::new(second.r, second.g, second.b, second.a))
}

/// Product color modes own the backdrop; the native skin still owns the
/// secondary chrome contract. Pin both properties so future token edits
/// cannot let OS colors take over the product palette again.
#[test]
fn product_modes_and_secondary_native_chrome_are_stable() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let tokens = tokens_for(skin, mode);
            let product = product_tokens(mode);
            assert_eq!(tokens.window_bg, product.window_bg);
            assert_eq!(tokens.view_bg, product.view_bg);
            assert_eq!(tokens.card_bg, product.card_bg);
            assert_eq!(tokens.accent, product.accent);
            // Panel surfaces are always opaque in every variant.
            assert_eq!(tokens.view_bg.a, 1.0);
            assert_eq!(tokens.card_bg.a, 1.0);
        }
    }
}

#[test]
fn semantic_status_accents_stay_distinct_and_typed() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let tokens = tokens_for(skin, mode);
            assert_ne!(
                tokens.success,
                tokens.warning,
                "{} {} success and warning must be distinct hues",
                skin.label(),
                mode.label(),
            );
            assert_ne!(
                tokens.success,
                tokens.danger,
                "{} {} success must not be the danger red",
                skin.label(),
                mode.label(),
            );
        }
    }
}

#[test]
fn accent_control_text_meets_wcag_aa_in_every_theme() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            for contrast in [HighContrast::Off, HighContrast::On] {
                let theme = Theme::build(skin, mode, contrast, ResolvedFonts::system_for(skin));
                let ratio = contrast_ratio(theme.accent, theme.accent_text);
                assert!(
                    ratio >= 4.5,
                    "{} {} accent control contrast was only {ratio:.2}:1",
                    skin.label(),
                    mode.label(),
                );
            }
        }
    }
}

#[test]
fn radius_gradients_are_strictly_increasing_and_skin_distinct() {
    let mut seen: Vec<(Skin, [f32; 5])> = Vec::new();
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let tokens = tokens_for(skin, mode);
            assert!(
                tokens.radii[2] >= 11.0 && tokens.radii[3] >= 15.0,
                "{} controls and panels must keep the rounded product geometry",
                skin.label(),
            );
            let mut prev = -1.0;
            for (i, r) in tokens.radii.into_iter().enumerate() {
                assert!(
                    r > prev,
                    "{} {} radius tier {i} = {r} must be strictly increasing",
                    skin.label(),
                    mode.label(),
                );
                prev = r;
            }
            // Radius is a skin property: light and dark share the gradient.
            if let Some((_, previous)) = seen.iter().find(|(s, _)| *s == skin) {
                assert_eq!(
                    *previous,
                    tokens.radii,
                    "{} radius must not depend on light/dark",
                    skin.label(),
                );
            } else {
                seen.push((skin, tokens.radii));
            }
        }
    }
    for (index, (_, radii)) in seen.iter().enumerate() {
        assert!(
            seen.iter().skip(index + 1).all(|(_, other)| radii != other),
            "each skin must retain a distinct radius gradient",
        );
    }
}

#[test]
fn variant_identity_fields_survive_materialization() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let t = tokens_for(skin, mode).into_theme(skin, mode);
            assert_eq!(t.skin, skin);
            assert_eq!(t.mode, mode);
            assert_eq!(t.dark, mode.is_dark());
            assert!(!t.hc, "high contrast is a build-time post-transform");
            assert!(!t.window_transparent);
            assert_eq!(t.window_radius, t.radius(RadiusScale::XLarge));
        }
    }
}

#[test]
fn material_and_window_controls_follow_platform_idioms() {
    assert_eq!(
        tokens_for(Skin::Gnome, LightDark::Dark).material,
        Material::Opaque
    );
    assert_eq!(
        tokens_for(Skin::Kde, LightDark::Dark).material,
        Material::Opaque
    );
    assert_eq!(
        tokens_for(Skin::Windows, LightDark::Dark).material,
        Material::Mica
    );
    assert_eq!(
        tokens_for(Skin::Macos, LightDark::Dark).material,
        Material::Vibrancy
    );
    assert_eq!(
        tokens_for(Skin::Windows, LightDark::Dark).window_controls,
        WindowControls::Caption
    );
    assert_eq!(
        tokens_for(Skin::Macos, LightDark::Dark).window_controls,
        WindowControls::TrafficLight
    );
}
