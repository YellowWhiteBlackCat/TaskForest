// test-intent: behavior
//! The lazy-invalidation discipline, stated as the properties the four
//! migrated tables now share: two different surfaces never share a key even
//! with identical visual inputs, one theme rule invalidates every surface,
//! and set-valued inputs hash order-independently.

use super::LazyKey;
use taskmanager_theme::{HighContrast, LightDark, Skin, Theme};

fn dark_theme() -> Theme {
    Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        taskmanager_theme::ResolvedFonts {
            ui: "Adwaita Sans",
            mono: "Adwaita Mono",
        },
    )
}

#[test]
fn two_surfaces_with_identical_visual_inputs_never_share_an_invalidation_key() {
    let theme = dark_theme();
    let applications = LazyKey::new("applications-table")
        .revision(7)
        .theme(&theme)
        .field("query")
        .finish();
    let inventory = LazyKey::new("inventory-table")
        .revision(7)
        .theme(&theme)
        .field("query")
        .finish();
    let history = LazyKey::new("app-history-table")
        .revision(7)
        .theme(&theme)
        .field("query")
        .finish();

    assert_ne!(applications, inventory);
    assert_ne!(applications, history);
    assert_ne!(inventory, history);
}

#[test]
fn one_theme_rule_invalidates_every_surface_and_stability_holds() {
    let dark = dark_theme();
    let light = Theme::build(
        Skin::Gnome,
        LightDark::Light,
        HighContrast::Off,
        taskmanager_theme::ResolvedFonts {
            ui: "Adwaita Sans",
            mono: "Adwaita Mono",
        },
    );
    for scope in ["applications-table", "inventory-table"] {
        let on_dark = LazyKey::new(scope).revision(1).theme(&dark).finish();
        let on_light = LazyKey::new(scope).revision(1).theme(&light).finish();
        assert_ne!(
            on_dark, on_light,
            "a skin change must invalidate {scope} like any other table"
        );
        assert_eq!(
            LazyKey::new(scope).revision(1).theme(&dark).finish(),
            on_dark,
            "identical inputs must produce the identical key"
        );
    }
}

#[test]
fn a_visual_input_change_invalidates_but_the_scope_alone_never_does() {
    let theme = dark_theme();
    let before = LazyKey::new("perf-rail").revision(3).theme(&theme).finish();
    let after_new_watermark = LazyKey::new("perf-rail").revision(4).theme(&theme).finish();
    assert_ne!(before, after_new_watermark);
    assert_eq!(
        LazyKey::new("perf-rail").revision(3).theme(&theme).finish(),
        before
    );
}
