//! Alert-rule glass surfaces: one shared elevation, no nested stacking.

use super::*;
use gpui::Styled;
use taskmanager_theme::{GlassTier, HighContrast, LightDark, ResolvedFonts, Skin};
use taskmanager_ui::theme_binding::glass_shadow;

fn shared_shadow(theme: &Theme) -> Vec<gpui::BoxShadow> {
    glass_shadow(theme, GlassTier::Card)
}

#[test]
fn every_alert_rule_glass_surface_uses_the_shared_card_shadow() {
    for mode in [LightDark::Dark, LightDark::Light] {
        let theme = Theme::build(
            Skin::Gnome,
            mode,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        );
        let expected = shared_shadow(&theme);
        for (radius, padding) in [
            (tokens::control_radius(&theme), tokens::SPACE_7),
            (tokens::card_radius(&theme), tokens::SPACE_8),
        ] {
            let mut surface = glass_surface(&theme, radius, padding);
            let painted = surface
                .style()
                .box_shadow
                .as_ref()
                .expect("alert rule glass surface must paint the shared card shadow");
            assert_eq!(painted, &expected);
        }
    }
}

#[test]
fn inset_alert_controls_keep_glass_without_stacking_a_card_shadow() {
    let theme = Theme::dark();
    let mut surface = inset_glass_surface(&theme, tokens::control_radius(&theme), tokens::SPACE_7);
    assert!(
        surface
            .style()
            .box_shadow
            .as_ref()
            .is_none_or(|shadows| shadows.is_empty()),
        "inset controls must not stack a second card shadow"
    );
}
