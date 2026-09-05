//! test-intent: behavior
//!
//! Unit and scene assembly tests for the ported controls:
//! - Tooltip: state transitions (hover/focus) and scene layout with caption typography.
//! - Slider: bounded math (clamp, step snap, progress fraction, delta steps) and control scene.
//! - Scrollbar: geometry computation (visibility, thumb sizing, offset clamping, drag mapping) and rail scene.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::widget::Text;
use taskmanager_theme::Theme;

use super::{
    MIN_SCROLLBAR_THUMB_PX, ScrollbarOrientation, SliderState, TooltipPlacement, TooltipSpec,
    TooltipState, compute_scrollbar_geometry, scrollbar_scene, slider_scene, snap_slider_value,
    thumb_drag_to_scroll, tooltip_anchored_scene, tooltip_scene,
};
use crate::palette::ui_palette;

fn headless_scene_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app
}

fn text_census(app: &mut App, scene: impl bevy::scene::Scene) -> Vec<String> {
    let world = app.world_mut();
    let root = world
        .spawn_scene(scene)
        .expect("scene resolves without assets")
        .id();
    let texts = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect::<Vec<String>>();
    assert!(world.despawn(root), "the scene despawns cleanly");
    texts
}

#[test]
fn tooltip_state_transitions_follow_hover_and_focus() {
    let mut state = TooltipState::default();
    assert!(!state.is_visible(), "idle tooltip is hidden");

    state.set_hovered(true);
    assert!(state.is_visible(), "hover activates tooltip");

    state.set_hovered(false);
    assert!(!state.is_visible(), "unhover deactivates tooltip");

    state.set_focused(true);
    assert!(state.is_visible(), "keyboard focus activates tooltip");

    state.set_focused(false);
    assert!(!state.is_visible(), "blur deactivates tooltip");
}

#[test]
fn tooltip_spec_builds_with_optional_key_hint_and_placement() {
    let spec = TooltipSpec::new("Search processes")
        .with_key_hint("Ctrl+F")
        .with_placement(TooltipPlacement::Bottom);

    assert_eq!(spec.text, "Search processes");
    assert_eq!(spec.key_hint.as_deref(), Some("Ctrl+F"));
    assert_eq!(spec.placement, TooltipPlacement::Bottom);
}

#[test]
fn tooltip_scene_spawns_caption_typography_and_key_hint() {
    let palette = ui_palette(&Theme::dark());
    let spec = TooltipSpec::new("End Task").with_key_hint("Delete");
    let mut app = headless_scene_app();
    let texts = text_census(&mut app, tooltip_scene(&spec, &palette));

    assert_eq!(texts.len(), 2, "explanation text plus key hint");
    assert_eq!(texts[0], "End Task");
    assert_eq!(texts[1], "Delete");
}

#[test]
fn tooltip_anchored_scene_conditionally_mounts_content() {
    let palette = ui_palette(&Theme::dark());
    let spec = TooltipSpec::new("Help");
    let mut state = TooltipState::default();
    let mut app = headless_scene_app();

    let texts_hidden = text_census(&mut app, tooltip_anchored_scene(&spec, &state, &palette));
    assert!(
        texts_hidden.is_empty(),
        "hidden tooltip produces no text nodes"
    );

    state.set_hovered(true);
    let texts_visible = text_census(&mut app, tooltip_anchored_scene(&spec, &state, &palette));
    assert_eq!(
        texts_visible.len(),
        1,
        "visible tooltip produces explanation text"
    );
    assert_eq!(texts_visible[0], "Help");
}

#[test]
fn slider_math_clamps_and_snaps_correctly() {
    let mut state = SliderState::new(0.0, 100.0, 5.0, 12.0);
    assert_eq!(
        state.clamped_value(),
        10.0,
        "12.0 snaps to nearest step 10.0"
    );
    assert!((state.fraction() - 0.1).abs() < 1e-4);

    state.step_forward(1.0);
    assert_eq!(
        state.clamped_value(),
        15.0,
        "stepping forward advances by step"
    );

    state.step_backward(2.0);
    assert_eq!(
        state.clamped_value(),
        5.0,
        "stepping backward decrements by steps"
    );

    state.set_fraction(0.5);
    assert_eq!(state.clamped_value(), 50.0, "fraction 0.5 maps to midpoint");

    state.set_value(150.0);
    assert_eq!(
        state.clamped_value(),
        100.0,
        "value clamps to maximum bound"
    );

    state.set_value(-20.0);
    assert_eq!(state.clamped_value(), 0.0, "value clamps to minimum bound");

    assert_eq!(snap_slider_value(7.3, 0.0, 10.0, 2.5), 7.5);
    assert_eq!(snap_slider_value(5.0, 0.0, 10.0, 0.0), 5.0);
}

#[test]
fn slider_scene_spawns_track_and_numeric_readout() {
    let palette = ui_palette(&Theme::dark());
    let state = SliderState::new(0.0, 10.0, 1.0, 4.0);
    let mut app = headless_scene_app();
    let texts = text_census(&mut app, slider_scene(&state, &palette));

    assert_eq!(texts.len(), 1, "slider renders formatted readout text");
    assert_eq!(texts[0], "4.0");
}

#[test]
fn scrollbar_geometry_computes_viewport_proportions_and_bounds() {
    // Degenerate inputs
    let degenerate = compute_scrollbar_geometry(0.0, 100.0, 0.0, 200.0, MIN_SCROLLBAR_THUMB_PX);
    assert!(!degenerate.visible);

    // Content fits in viewport -> invisible
    let fitting = compute_scrollbar_geometry(100.0, 80.0, 0.0, 200.0, MIN_SCROLLBAR_THUMB_PX);
    assert!(!fitting.visible);
    assert_eq!(fitting.thumb_offset_px, 0.0);
    assert_eq!(fitting.thumb_size_px, 200.0);

    // Content exceeds viewport -> visible
    let geo = compute_scrollbar_geometry(100.0, 400.0, 0.0, 200.0, MIN_SCROLLBAR_THUMB_PX);
    assert!(geo.visible);
    assert_eq!(geo.thumb_offset_px, 0.0, "top scroll has zero thumb offset");
    assert_eq!(
        geo.thumb_size_px, 50.0,
        "100/400 ratio yields 50px thumb on 200px rail"
    );
    assert_eq!(geo.scroll_ratio(), 0.0);

    // Scrolled to bottom
    let geo_bottom = compute_scrollbar_geometry(100.0, 400.0, 300.0, 200.0, MIN_SCROLLBAR_THUMB_PX);
    assert!(geo_bottom.visible);
    assert_eq!(
        geo_bottom.thumb_offset_px, 150.0,
        "bottom scroll offsets thumb by track span (200 - 50 = 150)"
    );
    assert_eq!(geo_bottom.scroll_ratio(), 1.0);

    // Drag position to scroll offset inverse calculation
    let scroll = thumb_drag_to_scroll(75.0, 200.0, 50.0, 100.0, 400.0);
    assert_eq!(
        scroll, 150.0,
        "midpoint drag maps to midpoint scroll (300 * 0.5 = 150)"
    );
}

#[test]
fn scrollbar_scene_spawns_vertical_and_horizontal_without_panic() {
    let palette = ui_palette(&Theme::dark());
    let geo = compute_scrollbar_geometry(100.0, 300.0, 50.0, 150.0, MIN_SCROLLBAR_THUMB_PX);
    let mut app = headless_scene_app();

    let world = app.world_mut();
    let v_root = world
        .spawn_scene(scrollbar_scene(
            &geo,
            ScrollbarOrientation::Vertical,
            &palette,
        ))
        .expect("vertical scrollbar scene resolves")
        .id();
    assert!(world.despawn(v_root));

    let h_root = world
        .spawn_scene(scrollbar_scene(
            &geo,
            ScrollbarOrientation::Horizontal,
            &palette,
        ))
        .expect("horizontal scrollbar scene resolves")
        .id();
    assert!(world.despawn(h_root));
}
