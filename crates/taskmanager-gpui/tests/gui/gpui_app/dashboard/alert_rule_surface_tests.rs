//! Alert-rule surfaces share the Dashboard's semantic glass roles.

use super::{glass_surface, inset_glass_surface};
use gpui::Styled;
use taskmanager_theme::{
    GlassSurfaceRole, HighContrast, LightDark, ResolvedFonts, Skin, Theme, tokens,
};
use taskmanager_ui::theme_binding::{glass_fill, glass_shadow};

fn themes() -> [Theme; 4] {
    [
        Theme::dark(),
        Theme::build(
            Skin::Gnome,
            LightDark::Light,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        ),
        Theme::build(
            Skin::Gnome,
            LightDark::Dark,
            HighContrast::On,
            ResolvedFonts::system_for(Skin::Gnome),
        ),
        Theme::build(
            Skin::Gnome,
            LightDark::Light,
            HighContrast::On,
            ResolvedFonts::system_for(Skin::Gnome),
        ),
    ]
}

#[test]
fn rule_cards_cast_one_shared_shadow_and_their_nested_controls_cast_none() {
    let role = GlassSurfaceRole::PageCard;
    for theme in themes() {
        let mut card = glass_surface(&theme, tokens::card_radius(&theme), tokens::SPACE_7);
        let mut control =
            inset_glass_surface(&theme, tokens::control_radius(&theme), tokens::SPACE_6);
        assert_eq!(
            card.style().box_shadow.as_ref(),
            Some(&glass_shadow(&theme, role.tier())),
            "the outer rule card must cast the shared PageCard shadow"
        );
        assert_eq!(
            control.style().box_shadow.as_ref(),
            None,
            "a nested control must not stack a second shadow"
        );
        let Some(card_background) = card.style().background.as_ref() else {
            panic!("the rule card must paint glass");
        };
        let Some(control_background) = control.style().background.as_ref() else {
            panic!("the nested control must paint glass");
        };
        assert_eq!(*card_background, glass_fill(&theme, role.tier()));
        assert_eq!(*control_background, glass_fill(&theme, role.tier()));
    }
}
