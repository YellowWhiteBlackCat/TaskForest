use crate::gpui_app::elements::overlays::{flat_tooltip, tooltip};
use gpui::Styled;
use taskmanager_theme::{GlassTier, HighContrast, LightDark, ResolvedFonts, Skin, Theme, tokens};
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
            Some(&glass_fill(&theme, GlassTier::Card))
        );
        assert_eq!(
            label.style().border_widths.top,
            Some(gpui::AbsoluteLength::Rems(gpui::Rems(
                GlassTier::Card.edge_width_rem()
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
            Some(&glass_shadow(&theme, GlassTier::Card))
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
    assert_eq!(elevated_shadow, &glass_shadow(&theme, GlassTier::Card));
    let shadows = flat
        .style()
        .box_shadow
        .as_ref()
        .expect("flat tooltip must explicitly clear shadows");
    assert!(shadows.is_empty());
}
