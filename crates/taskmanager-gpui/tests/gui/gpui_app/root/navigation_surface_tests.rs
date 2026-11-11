//! Navigation glass-surface contract: persistent chrome never floats.

use super::navigation_surface;
use gpui::Styled;
use taskmanager_theme::{
    GlassSurfaceRole, GlassTier, HighContrast, Length, LightDark, ResolvedFonts, Skin, Theme,
    tokens,
};
use taskmanager_ui::theme_binding::{glass_fill, glass_shadow, rem_length};

#[test]
fn navigation_surface_uses_the_chrome_role_without_a_floating_shadow() {
    for skin in [Skin::Gnome, Skin::Macos, Skin::Kde, Skin::Windows] {
        for mode in [LightDark::Dark, LightDark::Light] {
            let theme = Theme::build(
                skin,
                mode,
                HighContrast::Off,
                ResolvedFonts::system_for(skin),
            );
            let tier = GlassSurfaceRole::Chrome.tier();
            let mut surface = navigation_surface(&theme, tokens::SPACE_6).render();
            let expected_radius: gpui::AbsoluteLength =
                rem_length(tokens::card_radius(&theme)).into();

            assert_eq!(tier, GlassTier::Sidebar);
            assert_eq!(
                surface.style().background.as_ref(),
                Some(&glass_fill(&theme, tier))
            );
            assert_eq!(surface.style().corner_radii.top_left, Some(expected_radius));
            assert!(
                surface
                    .style()
                    .box_shadow
                    .as_ref()
                    .is_none_or(|shadows| shadows.is_empty()),
                "persistent navigation must not stack a floating elevation shadow"
            );
            assert_ne!(
                glass_shadow(&theme, tier).len(),
                0,
                "the shared tier still owns a shadow that true floating surfaces use"
            );
        }
    }
}

#[test]
fn vertical_navigation_keeps_the_same_chrome_material_with_its_own_inset() {
    let theme = Theme::dark();
    let mut horizontal = navigation_surface(&theme, Length(0.0)).render();
    let mut vertical = navigation_surface(&theme, tokens::SPACE_6).render();
    let tier = GlassSurfaceRole::Chrome.tier();

    assert_eq!(
        horizontal.style().background.as_ref(),
        Some(&glass_fill(&theme, tier))
    );
    assert_eq!(
        vertical.style().background.as_ref(),
        Some(&glass_fill(&theme, tier))
    );
    assert!(
        horizontal
            .style()
            .box_shadow
            .as_ref()
            .is_none_or(|shadows| shadows.is_empty())
    );
    assert!(
        vertical
            .style()
            .box_shadow
            .as_ref()
            .is_none_or(|shadows| shadows.is_empty())
    );
}
