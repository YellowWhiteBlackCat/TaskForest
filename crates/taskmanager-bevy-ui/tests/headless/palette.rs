//! test-intent: behavior
//!
//! Headless pure-function tests for the theme-token → bevy adapter.
//!
//! The oracle is the theme crate itself: every resolved bevy value must equal
//! the neutral token it claims to adapt (channel-exact colors, token sizes,
//! token weights, token radius). A mutation that hardcodes a bevy default or
//! reinterprets the token scale fails here without any window.

use bevy::color::Srgba;
use bevy::scene::WorldSceneExt;
use bevy::text::{FontSize, RemSize};
use taskmanager_theme::tokens::{self, UiSize};
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

use super::{UiPalette, theme_color, ui_palette};
use taskmanager_theme::Color;

/// The light-mode counterpart of `Theme::dark()`'s construction: same skin,
/// fonts and contrast, opposite mode.
fn light_theme() -> Theme {
    Theme::build(
        Skin::Gnome,
        LightDark::Light,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    )
}

fn assert_same_color(bevy_color: bevy::color::Color, token: Color) {
    let Srgba {
        red,
        green,
        blue,
        alpha,
    } = bevy_color.to_srgba();
    assert_eq!(
        (red, green, blue, alpha),
        (token.r, token.g, token.b, token.a)
    );
}

#[test]
fn token_colors_round_trip_channel_exact() {
    let dark = Theme::dark();
    let light = light_theme();
    for theme in [&dark, &light] {
        for token in [
            theme.window_bg,
            theme.fg,
            theme.fg_dim,
            theme.accent,
            theme.card_surface(),
            theme.view_bg,
            theme.sidebar_bg,
            theme.sidebar_card_bg,
        ] {
            assert_same_color(theme_color(token), token);
        }
    }
}

fn px(size: FontSize) -> f32 {
    size.eval(bevy::math::Vec2::ZERO, RemSize(16.0))
}

#[test]
fn ui_palette_derives_every_metric_from_tokens() {
    let theme = Theme::dark();
    let palette: UiPalette = ui_palette(&theme, UiSize::Standard);
    assert_same_color(palette.window_clear, theme.window_bg);
    assert_same_color(palette.panel_fill, theme.card_surface());
    assert_same_color(palette.content_bg, theme.view_bg);
    assert_same_color(palette.nav_bg, theme.sidebar_bg);
    assert_same_color(palette.nav_active_bg, theme.sidebar_card_bg);
    assert_same_color(palette.hover_bg, theme.hover_bg());
    assert_same_color(palette.selection_bg, theme.selection_bg());
    assert_same_color(palette.accent, theme.accent);
    assert_same_color(palette.heading_color, theme.fg);
    assert_same_color(palette.body_color, theme.fg);
    assert_same_color(palette.dim_color, theme.fg_dim);
    assert_eq!(
        palette.panel_radius_px,
        tokens::card_radius(&theme).0,
        "the card corner radius comes from the theme radius scale"
    );
    assert_eq!(
        palette.control_radius_px,
        tokens::control_radius(&theme).0,
        "the control corner radius comes from the theme radius scale"
    );
    assert_eq!(
        palette.control_height_px(),
        UiSize::Standard.control_height().0,
        "the control height resolves to the Standard density token"
    );
    assert_eq!(
        px(palette.heading.font_size),
        UiSize::Standard.page_title_font_size().0,
        "the heading size is the Standard page-title token"
    );
    assert_eq!(
        px(palette.body.font_size),
        UiSize::Standard.body_font_size().0,
        "the body size is the Standard reading-size token"
    );
    assert_eq!(
        px(palette.caption.font_size),
        UiSize::Standard.caption_font_size().0,
        "the caption size meets the 12px Standard Chinese floor"
    );
    assert_eq!(
        px(palette.mono.font_size),
        UiSize::Standard.body_font_size().0,
        "the mono role tracks the body baseline size"
    );
    assert_eq!(palette.heading.weight.0, 600, "header weight token");
    assert_eq!(palette.body.weight.0, 400, "normal weight token");
    assert_eq!(palette.caption.weight.0, 400, "normal weight token");
    assert_eq!(palette.mono.weight.0, 400, "mono weight token");
}

#[test]
fn ui_size_axis_rescales_the_rem_ladder() {
    let small = ui_palette(&Theme::dark(), UiSize::Small);
    let standard = ui_palette(&Theme::dark(), UiSize::Standard);
    let large = ui_palette(&Theme::dark(), UiSize::Large);
    // RemSize follows each step's body type; every rem Val in every scene
    // re-scales from this one resource.
    assert_eq!(small.rem_px, 14.0);
    assert_eq!(standard.rem_px, 16.0);
    assert_eq!(large.rem_px, 18.0);
    // The control height keeps resolving to the density table's px per step.
    assert_eq!(small.control_height_px(), 32.0);
    assert_eq!(standard.control_height_px(), 38.0);
    assert_eq!(large.control_height_px(), 46.0);
    // Type roles are rem ratios of the step's body type, so a step switch
    // re-scales them without touching a scene.
    assert_eq!(small.body.font_size, FontSize::Rem(1.0));
    assert_eq!(standard.heading.font_size, FontSize::Rem(28.0 / 16.0));
    assert_eq!(
        large.caption.font_size,
        FontSize::Rem(UiSize::Large.caption_font_size().0 / 18.0)
    );
}

#[test]
fn light_and_dark_themes_resolve_to_different_surfaces() {
    let dark = ui_palette(&Theme::dark(), UiSize::Standard);
    let light = ui_palette(&light_theme(), UiSize::Standard);
    assert_ne!(
        dark.window_clear.to_srgba(),
        light.window_clear.to_srgba(),
        "the adapter must respect the theme mode, not bake one skin"
    );
    assert_ne!(
        dark.dim_color.to_srgba(),
        light.dim_color.to_srgba(),
        "ink tokens follow the mode too"
    );
    assert_ne!(
        dark.nav_bg.to_srgba(),
        light.nav_bg.to_srgba(),
        "nav surfaces follow the mode too"
    );
}

#[test]
fn high_contrast_theme_strengthens_bevy_palette_and_flags_hc() {
    let standard = ui_palette(&Theme::dark(), UiSize::Standard);
    let hc_theme = Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::On,
        ResolvedFonts::system_for(Skin::Gnome),
    );
    let hc = ui_palette(&hc_theme, UiSize::Standard);
    assert!(!standard.high_contrast);
    assert!(hc.high_contrast);
    assert_ne!(standard.border_color.to_srgba(), hc.border_color.to_srgba());
    assert_eq!(hc.border_color.to_srgba().alpha, 1.0);
}

/// The rem axis is a behavior, not a source convention: a control's height is
/// declared as a rem ratio and resolves against the live `RemSize`, so every
/// UI-size step re-scales the type ladder without a scene rebuild.
#[test]
fn control_rows_ride_the_rem_axis_across_every_ui_size_step() {
    for size in UiSize::ALL {
        let palette = ui_palette(&Theme::dark(), size);
        let mut app = scene_app();
        let world = app.world_mut();
        let root = world
            .spawn_scene(crate::widgets::controls::stat_row_scene(
                "CPU".to_owned(),
                Box::new(crate::icons::icon_scene(
                    crate::icons::PLATE_ICONS[0],
                    &palette,
                    palette.dim_color,
                )),
                &palette,
            ))
            .expect("the stat row scene resolves without assets")
            .id();
        let node = world
            .get::<bevy::ui::Node>(root)
            .expect("the stat row mounts a layout node");
        assert_eq!(
            node.min_height,
            bevy::ui::Val::Rem(palette.control_height_rem),
            "the row height is declared on the rem axis, not in px"
        );
        assert_eq!(
            resolve_px(node.min_height, palette.rem_px),
            palette.control_height_px(),
            "the declared ratio resolves to the {size:?} control height"
        );
    }
}

/// The icon extent rides the same axis: the node declares the palette's icon
/// ratio, so the semantic icon re-scales with the reading size.
#[test]
fn icon_extents_ride_the_palette_icon_step() {
    for size in UiSize::ALL {
        let palette = ui_palette(&Theme::dark(), size);
        let mut app = scene_app();
        let world = app.world_mut();
        let root = world
            .spawn_scene(crate::icons::icon_scene(
                crate::icons::PLATE_ICONS[0],
                &palette,
                palette.dim_color,
            ))
            .expect("the icon scene resolves without assets")
            .id();
        let node = world
            .get::<bevy::ui::Node>(root)
            .expect("the icon mounts a layout node");
        assert_eq!(node.width, bevy::ui::Val::Rem(palette.icon_rem));
        assert_eq!(node.height, bevy::ui::Val::Rem(palette.icon_rem));
        assert_eq!(
            resolve_px(node.width, palette.rem_px),
            palette.icon_size_px(),
            "the icon extent resolves to the {size:?} icon step"
        );
    }
}

/// The rem axis is live: one declared ratio follows the `RemSize` resource, so
/// a UI-size choice re-scales the surface without touching a scene.
#[test]
fn a_declared_rem_ratio_follows_the_live_rem_size() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let declared = bevy::ui::Val::Rem(palette.control_height_rem);
    assert_eq!(resolve_px(declared, palette.rem_px), 38.0);
    assert_eq!(resolve_px(declared, 14.0), 33.25);
    assert_eq!(resolve_px(declared, 18.0), 42.75);
}

/// A headless scene host: the scene plugin plus the font store the text
/// component needs. No window, no compositor, no asset reads.
fn scene_app() -> bevy::app::App {
    let mut app = bevy::app::App::new();
    app.add_plugins(bevy::MinimalPlugins);
    app.add_plugins((
        bevy::asset::AssetPlugin::default(),
        bevy::scene::ScenePlugin,
    ));
    app.init_resource::<bevy::asset::Assets<bevy::text::Font>>();
    app
}

/// Resolve a length against a rem size the way the layout pass does.
fn resolve_px(value: bevy::ui::Val, rem_px: f32) -> f32 {
    value
        .resolve(
            1.0,
            0.0,
            bevy::math::Vec2::ZERO,
            bevy::text::EmSize(rem_px),
            bevy::text::RemSize(rem_px),
        )
        .expect("a rem length resolves against the rem size")
}
