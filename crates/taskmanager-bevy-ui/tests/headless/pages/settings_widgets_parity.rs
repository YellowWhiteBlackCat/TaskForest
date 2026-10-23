//! test-intent: behavior
//!
//! The settings center's owned component layer, end to end:
//!
//! - the **scrollbar** rail: a translucent floating track with a capsule
//!   thumb, bound to the scroll area it belongs to (BEVY-175);
//! - the **tooltip**: a 300 ms hover delay and a viewport fit that flips
//!   the card above a bottom anchor instead of hanging off the edge
//!   (BEVY-176);
//! - the **switch**: a real sliding control with the `Switch` accessibility
//!   role and a thumb that eases between the two ends (BEVY-177);
//! - the **rem axis**: the page's furniture scales with the UI-size choice
//!   instead of riding hard-coded pixels (BEVY-178);
//! - the **focus ring**: the inner 2 px highlight plus the outer 1 px
//!   translucent halo, placed over the focused control (BEVY-179);
//! - the **window edge**: the hairline an undecorated window draws so its
//!   edge stays readable against a dark wallpaper (BEVY-180);
//! - the **UI size**: one frame, one layout — the switch re-scales the
//!   surface with no residual frame at the old scale (BEVY-181);
//! - the **color space**: the translucent surfaces composite in the theme's
//!   own sRGB space, not in the render target's (BEVY-183);
//! - the **text pipeline**: every text node carries the font's own kerning
//!   (BEVY-184).

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::Assets;
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::world::World;
use bevy::image::TextureAtlasLayout;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::math::{Rect, UVec2, Vec2};
use bevy::picking::DefaultPickingPlugins;
use bevy::picking::hover::PickingInteraction;
use bevy::text::{Font, TextFont};
use bevy::transform::TransformPlugin;
use bevy::ui::widget::Text;
use bevy::ui::{Checked, ComputedNode, UiGlobalTransform, UiTransform};
use bevy::ui_widgets::{Checkbox, ScrollArea, Scrollbar, ScrollbarThumb};
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use crate::app::{Page, PageContent, Route};
use crate::focus_visible::{FocusRing, InputModality};
use crate::pages::settings::{SettingsChoice, SettingsField};
use crate::palette::{composite_over, ui_palette};
use crate::tooltip::{TooltipOverlay, TooltipState, TooltipText, resolve_placement};
use crate::widgets::focus_ring::{FocusRingHalo, focus_ring_layers};
use crate::widgets::scrollbar::ProductScrollbar;
use crate::widgets::switch::{SwitchId, SwitchSlide, SwitchThumb};
use crate::window::tests::HeadlessFrontendPlugins;
use crate::window::{FrontendWindowPlugin, WindowPalette};

// ---- the headless composition ---------------------------------------------

const FRAME: (u32, u32) = (1280, 720);

fn headless_shell_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime: crate::runtime::demo_platform_runtime(),
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });
    app.init_resource::<Assets<Font>>();
    app.add_plugins((
        WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..Default::default()
        },
        DefaultPickingPlugins,
        TransformPlugin,
        bevy::text::TextPlugin,
        bevy::ui::UiPlugin,
    ));
    app.init_resource::<Assets<TextureAtlasLayout>>();
    let (width, height) = FRAME;
    let size = UVec2::new(width, height);
    app.world_mut().spawn((
        Window {
            resolution: (width, height).into(),
            decorations: false,
            ..Default::default()
        },
        PrimaryWindow,
    ));
    app.world_mut().spawn((
        Camera2d,
        Camera {
            computed: ComputedCameraValues {
                target_info: Some(RenderTargetInfo {
                    physical_size: size,
                    scale_factor: 1.0,
                }),
                ..Default::default()
            },
            viewport: Some(Viewport {
                physical_size: size,
                ..Default::default()
            }),
            ..Default::default()
        },
    ));
    app
}

fn mount_settings(app: &mut App) {
    app.world_mut().resource_mut::<Route>().page = Page::Settings;
    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<&PageContent, ()>()
            .single(app.world())
            .expect("exactly one page content mounts")
            .page,
        Page::Settings,
        "the settings page is mounted"
    );
}

/// The switch slot: the entity that owns the toggle behavior.
fn switch_slot(world: &mut World) -> Entity {
    world
        .query_filtered::<(Entity, &SwitchId), ()>()
        .iter(world)
        .map(|(entity, _)| entity)
        .next()
        .expect("a settings boolean row mounts a switch")
}

/// The measured width of one node, in logical pixels.
fn measured_width(world: &mut World, entity: Entity) -> f32 {
    let mut query = world.query::<(&ComputedNode, &UiGlobalTransform)>();
    let (node, transform) = query.get(world, entity).expect("the node is measured");
    let _ = transform;
    node.size().x * node.inverse_scale_factor
}

// ---- BEVY-175: the scrollbar rail -----------------------------------------

#[test]
fn settings_scroll_area_owns_a_bound_scrollbar_rail() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    let world = app.world_mut();
    let rail = world
        .query_filtered::<Entity, With<ProductScrollbar>>()
        .iter(world)
        .next()
        .expect("the scroll area owns a visible scrollbar rail");
    let scrollbar = world
        .get::<Scrollbar>(rail)
        .expect("the rail carries the official scrollbar behavior");
    let target = scrollbar.target;
    assert!(
        world.get::<ScrollArea>(target).is_some(),
        "the rail drives the scroll area it belongs to"
    );
    let thumb = world
        .query_filtered::<Entity, With<ScrollbarThumb>>()
        .iter(world)
        .next()
        .expect("the rail carries a draggable thumb");
    assert_ne!(thumb, rail, "the thumb is the rail's own child");
    let fill = world
        .get::<bevy::ui::BackgroundColor>(rail)
        .expect("the track paints a fill");
    assert!(
        fill.0.to_srgba().alpha > 0.0,
        "the track is a visible translucent rail: {:?}",
        fill.0.to_srgba()
    );
}

// ---- BEVY-176: the tooltip's delay and viewport fit -----------------------

#[test]
fn tooltip_card_flips_above_a_bottom_anchor_and_stays_inside_the_viewport() {
    let viewport = Vec2::new(800.0, 600.0);
    let card = Vec2::new(200.0, 60.0);
    let gap = 4.0;

    // An anchor in the middle: the card sits below it.
    let middle = Rect::from_corners(Vec2::new(300.0, 200.0), Vec2::new(400.0, 240.0));
    let below = resolve_placement(middle, card, viewport, gap);
    assert!(!below.flipped, "a card with room below stays below");
    assert_eq!(below.origin.x, 300.0, "the card aligns with its anchor");
    assert_eq!(
        below.origin.y, 244.0,
        "the card clears the anchor by the gap"
    );

    // An anchor at the bottom: the card flips above it instead of leaving
    // the viewport.
    let bottom = Rect::from_corners(Vec2::new(300.0, 540.0), Vec2::new(400.0, 580.0));
    let above = resolve_placement(bottom, card, viewport, gap);
    assert!(above.flipped, "a card that would fall off the bottom flips");
    assert_eq!(
        above.origin.y, 476.0,
        "the flipped card sits above its anchor"
    );
    assert!(
        above.origin.y + card.y <= viewport.y,
        "the flipped card stays inside the viewport"
    );

    // An anchor at the right edge: the card is clamped inside.
    let right = Rect::from_corners(Vec2::new(780.0, 200.0), Vec2::new(800.0, 240.0));
    let clamped = resolve_placement(right, card, viewport, gap);
    assert_eq!(
        clamped.origin.x, 600.0,
        "the card is clamped inside the viewport"
    );
    assert!(
        clamped.origin.x + card.x <= viewport.x,
        "the card never hangs off the side"
    );
}

#[test]
fn tooltip_waits_for_the_hover_delay_before_it_shows() {
    let mut app = headless_shell_app();
    // The clock is the hover's own: one frame is one delay.
    *app.world_mut()
        .resource_mut::<bevy::time::TimeUpdateStrategy>() =
        bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(100));
    app.update();

    let target = app
        .world_mut()
        .spawn((
            TooltipText("CPU usage breakdown".into()),
            PickingInteraction::None,
        ))
        .id();
    app.update();

    *app.world_mut()
        .get_mut::<PickingInteraction>(target)
        .expect("the target carries its hover state") = PickingInteraction::Hovered;
    app.update();

    let hidden = app
        .world_mut()
        .query_filtered::<Entity, With<TooltipOverlay>>()
        .iter(app.world())
        .next()
        .expect("the card mounts with the hover");
    assert!(
        !app.world().resource::<TooltipState>().is_ready(),
        "a fresh hover is not ready yet"
    );
    assert_eq!(
        app.world().get::<bevy::prelude::Visibility>(hidden),
        Some(&bevy::prelude::Visibility::Hidden),
        "the card stays hidden while the pointer is still moving"
    );

    // The pointer rests on the same target: after the delay the card shows.
    for _ in 0..4 {
        app.update();
    }
    assert!(
        app.world().resource::<TooltipState>().is_ready(),
        "the settled hover is ready"
    );
    assert_eq!(
        app.world().get::<bevy::prelude::Visibility>(hidden),
        Some(&bevy::prelude::Visibility::Visible),
        "the card shows once the delay elapsed"
    );
}

// ---- BEVY-177: the switch --------------------------------------------------

#[test]
fn settings_boolean_rows_mount_a_sliding_switch() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    let world = app.world_mut();
    let slot = switch_slot(world);
    assert!(
        world.get::<Checkbox>(slot).is_some(),
        "the switch rides the official toggle behavior"
    );
    let node = world
        .get::<bevy::a11y::AccessibilityNode>(slot)
        .expect("the switch publishes an accessibility node");
    assert_eq!(
        node.0.role(),
        accesskit::Role::Switch,
        "the platform tree announces a switch, not a checkbox"
    );
    let thumb = world
        .query_filtered::<Entity, With<SwitchThumb>>()
        .iter(world)
        .next()
        .expect("the switch has a thumb");
    let slide = world
        .get::<SwitchSlide>(thumb)
        .expect("the thumb carries its slide state");
    assert_eq!(
        slide.position, 0.0,
        "an off switch rests at the off end of its slot"
    );
}

#[test]
fn the_switch_thumb_eases_between_its_two_ends() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    let slot = switch_slot(app.world_mut());
    let thumb = app
        .world_mut()
        .query_filtered::<Entity, With<SwitchThumb>>()
        .iter(app.world())
        .next()
        .expect("the switch has a thumb");

    // The switch turns on: the thumb starts moving toward the other end.
    app.world_mut().entity_mut(slot).insert(Checked);
    app.update();
    let transform = app
        .world()
        .get::<UiTransform>(thumb)
        .expect("the thumb carries its transform");
    let offset = match transform.translation.x {
        bevy::ui::Val::Px(value) => value,
        _ => 0.0,
    };
    assert!(offset > 0.0, "the thumb left the off end: {offset}");
    assert!(
        offset
            < crate::widgets::switch::SLOT_WIDTH_REM
                * app.world().resource::<WindowPalette>().inner.rem_px,
        "the thumb is still on its way: {offset}"
    );
}

// ---- BEVY-178: the rem axis ------------------------------------------------

#[test]
fn settings_furniture_scales_with_the_ui_size_instead_of_fixed_pixels() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    let slot = switch_slot(app.world_mut());
    let standard = measured_width(app.world_mut(), slot);
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    assert!(
        (standard - crate::widgets::switch::SLOT_WIDTH_REM * palette.rem_px).abs() < 1.0,
        "the switch rides the rem axis: {standard}"
    );

    // The UI-size choice re-scales the whole surface.
    let large = app
        .world_mut()
        .query_filtered::<(Entity, &SettingsChoice), ()>()
        .iter(app.world())
        .find(|(_, choice)| matches!(choice.0, SettingsField::UiSize(UiSize::Large)))
        .map(|(entity, _)| entity)
        .expect("the UI-size choice is mounted");
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::ValueChange::<bool> {
            source: large,
            value: true,
            is_final: true,
        });
    app.update();

    let scaled_slot = switch_slot(app.world_mut());
    let scaled = measured_width(app.world_mut(), scaled_slot);
    let large_palette = ui_palette(&Theme::dark(), UiSize::Large);
    assert!(
        (scaled - crate::widgets::switch::SLOT_WIDTH_REM * large_palette.rem_px).abs() < 1.0,
        "the surface re-scaled with the reading size: {scaled}"
    );
    assert!(
        scaled > standard,
        "the large step is wider than the standard one: {scaled} vs {standard}"
    );
}

// ---- BEVY-179: the focus ring ---------------------------------------------

#[test]
fn the_focus_ring_paints_an_inner_highlight_and_an_outer_halo() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let layers = focus_ring_layers(&palette);
    assert_eq!(layers.inner_width, 2.0, "the inner highlight is 2 px wide");
    assert_eq!(layers.halo_width, 1.0, "the outer halo is 1 px wide");
    assert!(
        layers.halo_ink.to_srgba().alpha < layers.inner_ink.to_srgba().alpha,
        "the halo is the translucent layer: {:?} vs {:?}",
        layers.halo_ink.to_srgba(),
        layers.inner_ink.to_srgba()
    );

    let mut app = headless_shell_app();
    mount_settings(&mut app);

    let halo = app
        .world_mut()
        .query_filtered::<Entity, With<FocusRingHalo>>()
        .iter(app.world())
        .next()
        .expect("the shell owns the focus ring's halo overlay");
    let node = app
        .world()
        .get::<bevy::ui::Node>(halo)
        .expect("the halo is a node");
    assert_eq!(
        node.border,
        bevy::ui::UiRect::all(bevy::ui::Val::Px(layers.halo_width)),
        "the halo is a one-pixel band"
    );
}

#[test]
fn the_focus_halo_follows_the_focused_control() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    let slot = switch_slot(app.world_mut());
    assert!(
        app.world().get::<FocusRing>(slot).is_some(),
        "the switch is a focusable control"
    );

    // The keyboard takes the focus: the ring — and its halo — appear.
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(slot, FocusCause::Navigated);
    app.world_mut()
        .resource_mut::<crate::focus_visible::InputModalityState>()
        .transition(InputModality::Keyboard);
    app.update();

    let halo = app
        .world_mut()
        .query_filtered::<Entity, With<FocusRingHalo>>()
        .iter(app.world())
        .next()
        .expect("the shell owns the focus ring's halo overlay");
    let halo_node = app
        .world()
        .get::<bevy::ui::Node>(halo)
        .expect("the halo is a node");
    assert_ne!(
        halo_node.width,
        bevy::ui::Val::Px(0.0),
        "the halo was placed over the focused control"
    );
    assert_eq!(
        app.world().get::<bevy::prelude::Visibility>(halo),
        Some(&bevy::prelude::Visibility::Visible),
        "the halo is visible while the keyboard holds the focus"
    );
}

// ---- BEVY-180: the window edge --------------------------------------------

#[test]
fn an_undecorated_window_draws_a_one_pixel_edge_highlight() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    // The edge highlight is the shell's outermost node: a transparent box
    // whose only ink is a one-pixel border. It is found by its OWN marker —
    // "the first node with a border" would bind to whichever surface the
    // archetype order happened to yield (the focus halo draws a border too).
    let edge = app
        .world_mut()
        .query_filtered::<Entity, With<crate::window::edge::WindowEdgeHighlight>>()
        .iter(app.world())
        .next();
    let edge = edge.expect("the undecorated window draws its edge highlight");
    let node = app
        .world()
        .get::<bevy::ui::Node>(edge)
        .expect("the edge is a node");
    assert_eq!(
        node.border,
        bevy::ui::UiRect::all(bevy::ui::Val::Px(1.0)),
        "the edge is a one-pixel hairline"
    );
    assert_ne!(
        app.world().get::<bevy::prelude::Visibility>(edge),
        Some(&bevy::prelude::Visibility::Hidden),
        "the edge shows while the compositor draws no frame"
    );
}

// ---- BEVY-181: the UI size -------------------------------------------------

#[test]
fn the_ui_size_choice_moves_the_rem_axis_in_its_own_frame() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    let large = app
        .world_mut()
        .query_filtered::<(Entity, &SettingsChoice), ()>()
        .iter(app.world())
        .find(|(_, choice)| matches!(choice.0, SettingsField::UiSize(UiSize::Large)))
        .map(|(entity, _)| entity)
        .expect("the UI-size choice is mounted");
    // The activation observer runs synchronously: the rem axis must have
    // moved with it, so the frame's layout measures the new reading size
    // instead of waiting for a later schedule pass.
    app.world_mut()
        .trigger(bevy::ui_widgets::ValueChange::<bool> {
            source: large,
            value: true,
            is_final: true,
        });
    let large_palette = ui_palette(&Theme::dark(), UiSize::Large);
    assert_eq!(
        app.world().resource::<bevy::ui::prelude::RemSize>().0,
        large_palette.rem_px,
        "the rem axis moved in the choice's own frame"
    );
}

#[test]
fn the_ui_size_choice_relayouts_in_the_same_frame() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    let slot = switch_slot(app.world_mut());
    let before = measured_width(app.world_mut(), slot);

    let large = app
        .world_mut()
        .query_filtered::<(Entity, &SettingsChoice), ()>()
        .iter(app.world())
        .find(|(_, choice)| matches!(choice.0, SettingsField::UiSize(UiSize::Large)))
        .map(|(entity, _)| entity)
        .expect("the UI-size choice is mounted");
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::ValueChange::<bool> {
            source: large,
            value: true,
            is_final: true,
        });
    // One frame: the choice, the rem axis, the layout.
    app.update();

    let after_slot = switch_slot(app.world_mut());
    let after = measured_width(app.world_mut(), after_slot);
    let large_palette = ui_palette(&Theme::dark(), UiSize::Large);
    assert!(
        (after - crate::widgets::switch::SLOT_WIDTH_REM * large_palette.rem_px).abs() < 1.0,
        "the surface is already at the new reading size after one frame: {after}"
    );
    assert!(
        after > before,
        "the large step re-laid the surface out: {after} vs {before}"
    );
    assert_eq!(
        app.world().resource::<bevy::ui::prelude::RemSize>().0,
        large_palette.rem_px,
        "the rem axis moved with the choice"
    );
}

// ---- BEVY-183: the color space ---------------------------------------------

#[test]
fn translucent_surfaces_composite_in_the_theme_srgb_space() {
    let white = bevy::color::Color::srgba(1.0, 1.0, 1.0, 1.0);
    let half_black = bevy::color::Color::srgba(0.0, 0.0, 0.0, 0.5);
    let composited = composite_over(white, half_black).to_srgba();
    assert!(
        (composited.red - 0.5).abs() < 0.001,
        "the composite happens in sRGB space, not in linear space: {composited:?}"
    );
}

#[test]
fn the_scrollbar_rail_wears_the_calibrated_track_ink() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let mut app = headless_shell_app();
    mount_settings(&mut app);
    let rail = app
        .world_mut()
        .query_filtered::<Entity, With<ProductScrollbar>>()
        .iter(app.world())
        .next()
        .expect("the scroll area owns a visible scrollbar rail");
    let fill = app
        .world()
        .get::<bevy::ui::BackgroundColor>(rail)
        .expect("the track paints a fill");
    let expected = composite_over(
        palette.content_bg,
        crate::palette::with_alpha(palette.border_color, 0.25),
    );
    assert_eq!(
        fill.0.to_srgba(),
        expected.to_srgba(),
        "the rail's track is the calibrated composite"
    );
}

// ---- BEVY-184: the text pipeline -------------------------------------------

#[test]
fn every_text_node_carries_the_font_kerning() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    let expected = bevy::text::FontFeatures::builder()
        .enable(bevy::text::FontFeatureTag::new(b"kern"))
        .build();
    let world = app.world_mut();
    let mut query = world.query::<(&Text, &TextFont)>();
    let mut checked = 0;
    for (text, font) in query.iter(world) {
        assert!(!text.0.is_empty(), "the census only covers real text nodes");
        assert_eq!(
            font.font_features, expected,
            "the text node {:?} carries the font's kerning",
            text.0
        );
        checked += 1;
    }
    assert!(checked > 0, "the settings page renders text nodes");
}
