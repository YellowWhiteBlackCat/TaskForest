//! Appearance adaptation over `taskmanager-theme`: maps native appearance
//! facts onto the owned palette/token types via [`detect`] and builds the
//! CJK-safe font fallbacks. The gc-global bridges were removed in P6; the
//! neutral crate remains the token authority and is imported from its owner
//! paths directly (no aggregate facade).

use ::gpui::{Font, FontFallbacks, font};
use taskmanager_core::core::appearance::{DesktopAppearance, DesktopFamily, PreferredColorScheme};
use taskmanager_theme::{
    FONT_MISANS_VF, FONT_ROBOTO_MONO, HighContrast, LightDark, NativeAppearance, ResolvedFonts,
    Skin, Theme,
};

/// Build the UI font with the product CJK face as an explicit glyph fallback.
/// The primary family may be bundled, a verified system family, or a user
/// selection from the startup catalog; the fallback keeps Chinese text from
/// silently depending on the selected Latin face's coverage.
pub fn ui_font_with_fallback(theme: &Theme) -> Font {
    let mut resolved = font(theme.ui_font);
    resolved.fallbacks = Some(FontFallbacks::from_fonts(vec![
        FONT_MISANS_VF.to_owned(),
        FONT_ROBOTO_MONO.to_owned(),
    ]));
    resolved
}

/// Build the metrics font with a deterministic product fallback chain.
pub fn mono_font_with_fallback(theme: &Theme) -> Font {
    let mut resolved = font(theme.mono_font);
    resolved.fallbacks = Some(FontFallbacks::from_fonts(vec![
        FONT_ROBOTO_MONO.to_owned(),
        FONT_MISANS_VF.to_owned(),
    ]));
    resolved
}

/// Return the skin that corresponds to a native appearance snapshot without
/// consulting environment overrides. Late platform events use this mapping so
/// a timeout fallback can be corrected without reapplying `TM_SKIN` or a
/// persisted explicit skin.
pub fn skin_for_appearance(appearance: DesktopAppearance) -> Skin {
    match appearance.family {
        DesktopFamily::Gnome => Skin::Gnome,
        DesktopFamily::Kde => Skin::Kde,
        DesktopFamily::Windows => Skin::Windows,
        DesktopFamily::Macos => Skin::Macos,
        DesktopFamily::Unknown => Skin::Gnome,
    }
}

/// Return the testing/developer skin override, if it is syntactically valid.
/// This is kept separate from [`skin_for_appearance`] so a late native event
/// cannot accidentally erase an intentional test override.
pub fn forced_skin_from_env() -> Option<Skin> {
    std::env::var("TM_SKIN")
        .ok()
        .and_then(|value| parse_skin(&value))
        .map(|(skin, _)| skin)
}

/// Resolve the active theme from native-adapter appearance facts.
/// Honors `TM_SKIN=<skin>-<light|dark>` (and optional `TM_SKIN_HC=1`) for
/// testing.
pub fn detect(appearance: DesktopAppearance) -> Theme {
    if let Ok(v) = std::env::var("TM_SKIN")
        && let Some((skin, mode)) = parse_skin(&v)
    {
        let hc = if std::env::var("TM_SKIN_HC")
            .map(|s| !s.is_empty())
            .unwrap_or(false)
        {
            HighContrast::On
        } else {
            HighContrast::Off
        };
        return Theme::build(skin, mode, hc, ResolvedFonts::system_for(skin));
    }
    Theme::detect(NativeAppearance {
        family: match appearance.family {
            DesktopFamily::Gnome => Some(Skin::Gnome),
            DesktopFamily::Kde => Some(Skin::Kde),
            DesktopFamily::Windows => Some(Skin::Windows),
            DesktopFamily::Macos => Some(Skin::Macos),
            // Unknown stays None: the fallback decision lives in the theme
            // crate's detection, not here.
            DesktopFamily::Unknown => None,
        },
        scheme: match appearance.color_scheme {
            PreferredColorScheme::Dark => Some(LightDark::Dark),
            PreferredColorScheme::Light => Some(LightDark::Light),
            PreferredColorScheme::Unknown => None,
        },
        high_contrast: appearance.high_contrast,
    })
}

fn parse_skin(s: &str) -> Option<(Skin, LightDark)> {
    let (p, m) = s.split_once('-')?;
    let skin = match p.to_ascii_lowercase().as_str() {
        "gnome" => Skin::Gnome,
        "kde" => Skin::Kde,
        "win" | "windows" => Skin::Windows,
        "mac" | "macos" => Skin::Macos,
        _ => return None,
    };
    let mode = match m.to_ascii_lowercase().as_str() {
        "dark" => LightDark::Dark,
        "light" => LightDark::Light,
        "eyeforest" | "eye-forest" => LightDark::EyeForest,
        _ => return None,
    };
    Some((skin, mode))
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_theme_tests.rs"]
mod tests;
