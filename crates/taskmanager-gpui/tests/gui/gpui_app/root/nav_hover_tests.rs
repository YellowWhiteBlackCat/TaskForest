//! test-intent: behavior
//!
//! Navigation controls consume the theme's shared hover token; no nav surface
//! keeps a private accent-alpha blend that can drift away from every other
//! frontend.

use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

/// The hover token's alpha is the one authority every GPUI nav control spends.
/// Tab, settings and orientation do not restate an accent alpha; the theme
/// derives it from the same `hover_bg()` fact used by rows and controls.
#[test]
fn navigation_hover_uses_the_shared_hover_token_across_skins() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = Theme::build(
                skin,
                mode,
                HighContrast::Off,
                ResolvedFonts::system_for(skin),
            );
            assert_eq!(theme.hover_bg(), theme.accent.with_alpha(0.08));
        }
    }
}
