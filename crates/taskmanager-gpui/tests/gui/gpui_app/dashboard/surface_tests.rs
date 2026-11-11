//! Dashboard page cards use the shared page-card role.

use super::page_card;
use gpui::Styled;
use taskmanager_theme::{GlassSurfaceRole, HighContrast, LightDark, ResolvedFonts, Skin, Theme};
use taskmanager_ui::theme_binding::glass_fill;
use taskmanager_ui::theme_binding::glass_shadow;

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
fn page_cards_keep_the_shared_floating_shadow_and_material() {
    let role = GlassSurfaceRole::PageCard;
    for theme in themes() {
        let mut painted = page_card(theme.palette()).render();
        assert_eq!(
            painted.style().box_shadow.as_ref(),
            Some(&glass_shadow(&theme, role.tier())),
            "page card must keep the shared floating shadow"
        );
        let Some(background) = painted.style().background.as_ref() else {
            panic!("page card must paint glass");
        };
        assert_eq!(
            *background,
            glass_fill(&theme, role.tier()),
            "page card must paint the shared Card-tier fill"
        );
    }
}
