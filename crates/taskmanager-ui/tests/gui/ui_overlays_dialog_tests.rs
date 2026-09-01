use super::{Dialog, panel_shadow};

use taskmanager_theme::Theme;
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
}

/// The panel shadow is the Mission Center two-layer pair in the palette's
/// own ink: an ambient layer that is wider AND weaker than the edge layer,
/// and the edge layer carrying the token color verbatim.
#[test]
fn panel_shadow_is_two_layer_token_ink() {
    for mode in [
        taskmanager_theme::LightDark::Light,
        taskmanager_theme::LightDark::Dark,
    ] {
        let theme = Theme::build(
            taskmanager_theme::Skin::Gnome,
            mode,
            taskmanager_theme::HighContrast::Off,
            taskmanager_theme::ResolvedFonts::system_for(taskmanager_theme::Skin::Gnome),
        );
        let palette = theme.palette();
        let shadow = panel_shadow(&palette);
        assert_eq!(shadow.len(), 2, "panel shadow must have two layers");
        let ink: gpui::Hsla = crate::theme_binding::hsla(palette.card_shadow);
        let ambient: gpui::Hsla =
            crate::theme_binding::hsla(palette.card_shadow.with_alpha(palette.card_shadow.a * 0.6));
        assert_eq!(shadow[1].color, ink, "edge layer carries the token ink");
        assert_eq!(
            shadow[0].color, ambient,
            "ambient layer scales the ink alpha"
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
