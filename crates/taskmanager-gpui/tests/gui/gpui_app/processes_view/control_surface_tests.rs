//! Apps control-band glass contract: one inset PageHeader role, no second shadow.

use super::glass_page_header_surface;
use gpui::Styled;
use taskmanager_theme::{
    GlassSurfaceRole, GlassTier, HighContrast, LightDark, ResolvedFonts, Skin, Theme, tokens,
};
use taskmanager_ui::theme_binding::{glass_fill, glass_shadow, rem_length};

#[test]
fn apps_control_bands_share_one_inset_page_header_role() {
    let skins = Skin::ALL;
    let modes = LightDark::ALL;
    let contrasts = [HighContrast::Off, HighContrast::On];
    for skin in skins {
        for mode in modes {
            for high_contrast in contrasts {
                let theme =
                    Theme::build(skin, mode, high_contrast, ResolvedFonts::system_for(skin));
                let tier = GlassSurfaceRole::PageHeader.tier();
                let expected_radius: gpui::AbsoluteLength =
                    rem_length(tokens::card_radius(&theme)).into();
                for padding in [tokens::SPACE_2, tokens::SPACE_4] {
                    let mut surface = glass_page_header_surface(&theme, padding).render();

                    assert_eq!(tier, GlassTier::Card);
                    assert_eq!(
                        surface.style().background.as_ref(),
                        Some(&glass_fill(&theme, tier)),
                        "Apps control chrome must keep the shared Card glass fill"
                    );
                    assert_eq!(
                        surface.style().border_widths.top,
                        Some(gpui::AbsoluteLength::Rems(gpui::Rems(
                            tier.edge_width_rem()
                        ))),
                        "Apps control chrome must keep the shared Card hairline"
                    );
                    assert_eq!(
                        surface.style().corner_radii.top_left,
                        Some(expected_radius),
                        "both Apps control bands must keep the shared card radius"
                    );
                    assert!(
                        surface
                            .style()
                            .box_shadow
                            .as_ref()
                            .is_none_or(|shadows| shadows.is_empty()),
                        "page chrome must not stack a floating elevation shadow"
                    );
                    assert_eq!(
                        glass_shadow(&theme, tier).len(),
                        2,
                        "the shared shadow ladder remains available to floating surfaces"
                    );
                }
            }
        }
    }
}
