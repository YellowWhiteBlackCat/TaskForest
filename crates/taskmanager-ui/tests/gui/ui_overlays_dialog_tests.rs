use super::{Dialog, panel_shadow};

use crate::primitives::card_surface::{CardSurface, GlassSurfaceRole};
use crate::theme_binding::{glass_fill, glass_shadow};
use gpui::Styled;
use taskmanager_theme::HighContrast;
use taskmanager_theme::LightDark;
use taskmanager_theme::ResolvedFonts;
use taskmanager_theme::Skin;
use taskmanager_theme::Theme;
use taskmanager_theme::theme::{GlassShadowProjection, GlassTier};

/// The dialog panel resolves its material through the shared role contract:
/// Modal tier, floating elevation, one hairline edge.
#[test]
fn the_dialog_panel_uses_the_shared_modal_surface_contract() {
    let theme = Theme::dark();
    let mut owned = CardSurface::unpadded_for_role(theme.palette(), GlassSurfaceRole::Dialog)
        .radius(theme.palette().panel_radius)
        .render();
    assert_eq!(
        owned.style().background.as_ref(),
        Some(&glass_fill(&theme, GlassTier::Modal))
    );
    assert_eq!(
        owned.style().box_shadow.as_ref(),
        Some(&glass_shadow(&theme, GlassTier::Modal))
    );
    assert_eq!(
        owned.style().box_shadow.as_ref(),
        Some(&panel_shadow(&theme))
    );
}
#[test]
fn dialog_confirm_preset_disables_mask_close_and_close_button() {
    let dialog = Dialog::new().confirm();
    assert!(!dialog.mask_closable);
    assert!(!dialog.close_button);
    assert!(dialog.keyboard);
}

#[test]
fn alert_preset_sets_ok_only_footer() {
    let dialog = Dialog::new().alert();
    assert!(dialog.footer.is_some());
    assert!(!dialog.mask_closable);
    assert!(!dialog.close_button);
}

#[test]
fn default_mask_is_palette_scrim() {
    let dialog = Dialog::new().palette(Theme::dark().palette());
    let spec = dialog.into_modal_spec();
    let mask = spec.mask.expect("default mask present");
    assert!((mask.alpha - 0.5).abs() < 1e-4);
    let depth = mask.color();
    assert_eq!(
        (depth.r, depth.g, depth.b),
        (
            mask.palette.theme.glass_scrim().fill.r,
            mask.palette.theme.glass_scrim().fill.g,
            mask.palette.theme.glass_scrim().fill.b,
        ),
        "the default mask resolves through the theme's Scrim tier"
    );
}

/// The panel shadow is the Mission Center two-layer pair in the palette's
/// own ink: an ambient layer that is wider AND weaker than the edge layer,
/// while both layers share one bounded optical-depth alpha budget.
#[test]
fn panel_shadow_is_two_layer_token_ink() {
    for mode in [LightDark::Light, LightDark::Dark] {
        let theme = Theme::build(
            Skin::Gnome,
            mode,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        );
        let shadow = panel_shadow(&theme);
        assert_eq!(shadow.len(), 2, "panel shadow must have two layers");
        let layered = GlassTier::Modal.projected_shadow(&theme, GlassShadowProjection::Layered);
        assert_eq!(
            shadow[1].color,
            crate::theme_binding::hsla(layered.contact.color),
            "edge layer carries the shared contact projection"
        );
        assert_eq!(
            shadow[0].color,
            crate::theme_binding::hsla(layered.ambient.color),
            "ambient layer carries the shared ambient projection"
        );
        let ink = theme.glass_modal().shadow;
        let combined = 1.0 - (1.0 - layered.ambient.color.a) * (1.0 - layered.contact.color.a);
        assert!(
            (combined - ink.a).abs() < 1e-4,
            "the two layers stay within one token alpha budget: {combined} vs {ink:?}"
        );
        assert!(
            shadow[0].blur_radius > shadow[1].blur_radius,
            "ambient layer must blur wider than the edge layer"
        );
        assert!(
            shadow[0].offset.y > shadow[1].offset.y,
            "ambient layer must drop further than the edge layer"
        );
    }
}
