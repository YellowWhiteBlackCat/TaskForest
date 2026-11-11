//! Tooltip glass-surface contract: elevation and the flat inset variant.

use crate::gpui_app::elements::overlays::{flat_tooltip, tooltip, tooltip_surface};
use gpui::Styled;
use taskmanager_theme::{
    GlassSurfaceRole, HighContrast, LightDark, ResolvedFonts, Skin, Theme, tokens,
};
use taskmanager_ui::theme_binding::{glass_fill, glass_shadow, rem_length};

#[test]
fn tooltip_uses_the_shared_card_surface_material_and_shadow() {
    for mode in [LightDark::Dark, LightDark::Light] {
        let theme = Theme::build(
            Skin::Gnome,
            mode,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        );
        let mut label = tooltip(&theme, "sample");
        let expected_radius: gpui::AbsoluteLength =
            rem_length(tokens::control_radius(&theme)).into();
        assert_eq!(
            label.style().background.as_ref(),
            Some(&glass_fill(&theme, GlassSurfaceRole::Tooltip.tier()))
        );
        assert_eq!(
            label.style().border_widths.top,
            Some(gpui::AbsoluteLength::Rems(gpui::Rems(
                GlassSurfaceRole::Tooltip.tier().edge_width_rem()
            )))
        );
        assert_eq!(label.style().corner_radii.top_left, Some(expected_radius));
        assert_eq!(label.style().corner_radii.top_right, Some(expected_radius));
        assert_eq!(
            label.style().corner_radii.bottom_right,
            Some(expected_radius)
        );
        assert_eq!(
            label.style().corner_radii.bottom_left,
            Some(expected_radius)
        );
        assert_eq!(
            label.style().box_shadow.as_ref(),
            Some(&glass_shadow(&theme, GlassSurfaceRole::Tooltip.tier()))
        );
    }
}

#[test]
fn graph_tooltip_keeps_its_flat_elevation() {
    let theme = Theme::dark();
    let mut elevated = tooltip(&theme, "sample");
    let mut flat = flat_tooltip(&theme, "sample");
    let elevated_shadow = elevated
        .style()
        .box_shadow
        .as_ref()
        .expect("elevated tooltip paints the shared shadow");
    assert_eq!(
        elevated_shadow,
        &glass_shadow(&theme, GlassSurfaceRole::Tooltip.tier())
    );
    assert!(
        flat.style()
            .box_shadow
            .as_ref()
            .is_none_or(|shadows| shadows.is_empty()),
        "a flat tooltip must not stack a second card shadow"
    );
}

#[test]
fn every_tooltip_surface_spends_the_shared_tooltip_role() {
    let theme = Theme::dark();
    let tier = GlassSurfaceRole::Tooltip.tier();
    let mut elevated = tooltip(&theme, "sample");
    let mut flat = flat_tooltip(&theme, "sample");
    let mut row = tooltip_surface(&theme).render();

    assert_eq!(
        elevated.style().background.as_ref(),
        Some(&glass_fill(&theme, tier))
    );
    assert_eq!(
        flat.style().background.as_ref(),
        Some(&glass_fill(&theme, tier))
    );
    assert_eq!(
        row.style().background.as_ref(),
        Some(&glass_fill(&theme, tier))
    );
    assert_eq!(
        elevated.style().box_shadow.as_ref(),
        Some(&glass_shadow(&theme, tier))
    );
    assert!(
        flat.style()
            .box_shadow
            .as_ref()
            .is_none_or(|shadows| shadows.is_empty()),
        "the chart tooltip keeps the same material without a second elevation"
    );
    assert_eq!(
        row.style().box_shadow.as_ref(),
        Some(&glass_shadow(&theme, tier))
    );
}
