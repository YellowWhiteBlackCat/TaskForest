use taskmanager_theme::{GlassSurfaceRole, Theme, tokens::UiSize};

use crate::glass::surface_material;
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
