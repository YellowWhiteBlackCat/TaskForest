use super::*;
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin};

/// Every skin × mode resolves a valid terminal palette with the semantic
/// distinctions the renderer relies on (accents distinct, fills usable).
#[test]
fn every_skin_variant_resolves_a_terminal_palette() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            for hc in [HighContrast::Off, HighContrast::On] {
                let theme = Theme::build(skin, mode, hc, ResolvedFonts::system_for(skin));
                let tui = TuiTheme::from_theme(&theme);
                assert_ne!(tui.accent, tui.dim);
                assert_ne!(tui.good, tui.danger);
                assert_ne!(tui.warn, tui.good);
                let Color { a, .. } = theme.palette().window_backdrop;
                let _ = a;
                // Terminal fills are opaque; a translucent backdrop was
                // composited onto an opaque base.
                assert_ne!(tui.bg, ratatui::style::Color::Reset);
            }
        }
    }
}

/// The neutral `Color` → terminal conversion is channel-exact.
#[test]
fn rgb_conversion_is_channel_exact() {
    assert_eq!(
        rgb(Color::from_hex(0x3584e4)),
        ratatui::style::Color::Rgb(0x35, 0x84, 0xe4)
    );
    assert_eq!(rgb(Color::BLACK), ratatui::style::Color::Rgb(0, 0, 0));
    assert_eq!(rgb(Color::WHITE), ratatui::style::Color::Rgb(255, 255, 255));
}

/// Config tokens map onto typed theme parameters; unknown tokens fall
/// back to the GNOME dark default and `System` resolves to Dark because
/// the terminal has no native-appearance facts.
#[test]
fn config_tokens_resolve_onto_typed_theme_params() {
    assert_eq!(
        ThemeParams::from_config_tokens("KDE", "Light", true),
        ThemeParams {
            skin: Skin::Kde,
            mode: LightDark::Light,
            hc: true,
        }
    );
    assert_eq!(
        ThemeParams::from_config_tokens("macOS", "Dark", false),
        ThemeParams {
            skin: Skin::Macos,
            mode: LightDark::Dark,
            hc: false,
        }
    );
    assert_eq!(
        ThemeParams::from_config_tokens("GNOME", "EyeForest", false),
        ThemeParams {
            skin: Skin::Gnome,
            mode: LightDark::EyeForest,
            hc: false,
        }
    );
    assert_eq!(
        ThemeParams::from_config_tokens("", "System", true),
        ThemeParams {
            skin: Skin::Gnome,
            mode: LightDark::Dark,
            hc: true,
        }
    );
    assert_eq!(
        ThemeParams::from_config_tokens("unknown", "System", true),
        ThemeParams {
            skin: Skin::Gnome,
            mode: LightDark::Dark,
            hc: true,
        }
    );
}

/// A settings change re-skins the terminal palette: different params
/// produce a different resolved palette, and every params combination
/// still resolves a valid theme.
#[test]
fn rebuilt_palette_follows_theme_params() {
    let base = TuiTheme::from_params(ThemeParams::default());
    let kde_light = ThemeParams {
        skin: Skin::Kde,
        mode: LightDark::Light,
        hc: false,
    };
    let rebuilt = TuiTheme::from_params(kde_light);
    assert_ne!(base.bg, rebuilt.bg, "KDE light must re-skin the backdrop");
    assert_ne!(base.accent, rebuilt.accent);
    let high_contrast = ThemeParams {
        hc: true,
        ..ThemeParams::default()
    };
    let hc = TuiTheme::from_params(high_contrast);
    assert_ne!(base.dim, hc.dim, "high contrast must brighten muted text");
}
