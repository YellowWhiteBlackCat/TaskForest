use taskmanager_theme::{GlassSurfaceRole, Theme, tokens::UiSize};

use crate::glass::{card_material, surface_material};
use crate::palette::ui_palette;

#[test]
fn role_materials_keep_tier_tokens_and_follow_shared_elevation() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    for role in GlassSurfaceRole::ALL {
        let material = surface_material(role, &palette);
        let tier = role.tier();
        assert_eq!(material.fill, palette.glass_fill(tier), "{role:?} fill");
        assert_eq!(material.edge, palette.glass_edge(tier), "{role:?} edge");
        assert_eq!(
            material.edge_width_rem,
            tier.edge_width_rem(),
            "{role:?} edge width"
        );
        assert_eq!(
            material.shadows,
            if role.elevation().casts_shadow() {
                palette.glass_shadow(tier)
            } else {
                Vec::new()
            },
            "{role:?} shadow projection"
        );
    }
}

/// The legacy page-card helper is a named binding to the shared floating role,
/// not an independent tier/shadow decision.
#[test]
fn card_material_is_the_floating_page_card_role() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let card = card_material(&palette);
    let role = surface_material(GlassSurfaceRole::PageCard, &palette);
    assert_eq!(card.fill, role.fill);
    assert_eq!(card.edge, role.edge);
    assert_eq!(card.edge_width_rem, role.edge_width_rem);
    assert_eq!(card.radius_rem, role.radius_rem);
    assert_eq!(card.shadows, role.shadows);
    assert_eq!(
        card.shadows,
        palette.glass_shadow(GlassSurfaceRole::PageCard.tier())
    );
}
