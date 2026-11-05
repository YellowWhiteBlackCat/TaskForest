//! test-intent: behavior
//!
//! The product scrollbar rail's two contracts: a rail is in the layout only
//! while its scroll area actually scrolls (Bevy's widget sizes an idle thumb
//! to the whole track, which would paint a full-height bar on a panel that
//! fits), and the capsule that does paint is the shared rail's narrow ink,
//! not a heavy track.

use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::{App, PostUpdate};
use bevy::asset::{AssetPlugin, Assets};
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::image::{Image, TextureAtlasLayout};
use bevy::math::UVec2;
use bevy::scene::{ScenePlugin, WorldSceneExt, bsn};
use bevy::text::{Font, TextPlugin};
use bevy::time::TimeUpdateStrategy;
use bevy::ui::prelude::*;
use bevy::ui::{ComputedNode, UiPlugin, UiSystems};
use bevy::ui_widgets::{ControlOrientation, ScrollArea, Scrollbar, ScrollbarThumb};
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use crate::palette::{composite_over, ui_palette, with_alpha};
use crate::widgets::scrollbar::{
    LANE_WIDTH_REM, ProductScrollbar, bind_scrollbar_targets, rail_lane, scrollbar_rail_scene,
    sync_rail_visibility,
};
use crate::window::WindowPalette;

/// The probe box: one scroll area and its rail inside a fixed viewport.
const BOX_WIDTH: f32 = 240.0;
const BOX_HEIGHT: f32 = 200.0;

/// Mount one scroll area with `content_height` of content beside its rail,
/// with the production binding and visibility systems wired.
fn rail_app(content_height: f32) -> App {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((
        AssetPlugin::default(),
        ScenePlugin,
        bevy::input::InputPlugin,
        bevy::picking::PickingPlugin,
        bevy::picking::InteractionPlugin,
        bevy::transform::TransformPlugin,
        UiPlugin,
        TextPlugin,
    ));
    app.init_resource::<Assets<Font>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<TextureAtlasLayout>>();
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        16,
    )));
    app.add_systems(
        PostUpdate,
        (
            bind_scrollbar_targets.before(UiSystems::Layout),
            sync_rail_visibility.after(UiSystems::PostLayout),
        ),
    );
    app.add_plugins(WindowPlugin {
        primary_window: None,
        exit_condition: ExitCondition::DontExit,
        ..Default::default()
    });
    let size = UVec2::new(640, 480);
    app.world_mut().spawn((
        Window {
            resolution: (640, 480).into(),
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
    let world = app.world_mut();
    let wrapper = world
        .spawn_scene(bsn! {
            Node {
                width: px(BOX_WIDTH),
                height: px(BOX_HEIGHT),
                position_type: PositionType::Relative,
            }
        })
        .expect("the wrapper scene spawns")
        .id();
    let area = world
        .spawn_scene(bsn! {
            Node {
                width: percent(100),
                height: percent(100),
                overflow: Overflow::scroll_y(),
                padding: rail_lane(UiRect::default()),
                flex_direction: FlexDirection::Column,
            }
            ScrollArea
        })
        .expect("the scroll area scene spawns")
        .id();
    let content = world
        .spawn_scene(bsn! {
            Node {
                width: percent(100),
                height: px(content_height),
                flex_shrink: 0.0,
            }
        })
        .expect("the content scene spawns")
        .id();
    let rail = world
        .spawn_scene(scrollbar_rail_scene(&palette))
        .expect("the rail scene spawns")
        .id();
    world.entity_mut(area).add_child(content);
    world.entity_mut(wrapper).add_child(area);
    world.entity_mut(wrapper).add_child(rail);
    // The palette is the binding system's rem authority: it arrived before
    // the first update, so the scene is built from the same value.
    for _ in 0..6 {
        app.update();
    }
    app
}

/// The one rail the probe mounts.
fn mounted_rail(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<ProductScrollbar>>()
        .single(app.world())
        .expect("the probe mounts one rail")
}

#[test]
fn a_scroll_area_that_fits_takes_its_rail_out_of_the_layout() {
    // The content is shorter than the box: there is nothing to scroll, and
    // the widget's idle thumb would otherwise fill the whole track.
    let mut app = rail_app(BOX_HEIGHT - 40.0);
    let rail = mounted_rail(&mut app);
    let world = app.world_mut();
    let node = world.get::<Node>(rail).expect("the rail is a node");
    assert_eq!(
        node.display,
        Display::None,
        "a rail with nothing to scroll is not painted"
    );
}

#[test]
fn a_scroll_area_that_overflows_puts_its_rail_in_the_layout() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let mut app = rail_app(BOX_HEIGHT + 400.0);
    let rail = mounted_rail(&mut app);
    let world = app.world_mut();
    let node = world.get::<Node>(rail).expect("the rail is a node");
    assert_ne!(
        node.display,
        Display::None,
        "a scrollable area wears its rail"
    );
    let scrollbar = world
        .get::<Scrollbar>(rail)
        .expect("the rail binds the scroll area beside it");
    assert_eq!(
        scrollbar.orientation,
        ControlOrientation::Vertical,
        "a vertical scroller gets a vertical rail"
    );
    assert!(
        scrollbar.min_thumb_length >= 48.0,
        "the capsule never shrinks below the shared minimum thumb: {}",
        scrollbar.min_thumb_length
    );
    let rail_width = world
        .get::<ComputedNode>(rail)
        .expect("the rail is measured")
        .size()
        .x;
    let capsule = 0.1875 * palette.rem_px;
    assert!(
        (rail_width - capsule).abs() < 0.5,
        "the rail is the capsule lane, not a track: {rail_width} vs {capsule}"
    );
}

#[test]
fn the_capsule_wears_the_calibrated_ink_and_the_lane_is_reserved() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let mut app = rail_app(BOX_HEIGHT + 400.0);
    let rail = mounted_rail(&mut app);
    let world = app.world_mut();
    let thumb = world
        .get::<Children>(rail)
        .and_then(|children| {
            children
                .iter()
                .find(|child| world.get::<ScrollbarThumb>(**child).is_some())
        })
        .expect("the rail carries the capsule thumb");
    let capsule = world
        .get::<BackgroundColor>(*thumb)
        .expect("the capsule paints");
    let expected = composite_over(palette.content_bg, with_alpha(palette.body_color, 0.30));
    assert_eq!(
        capsule.0.to_srgba(),
        expected.to_srgba(),
        "the capsule is the theme foreground at the shared idle alpha, composited in sRGB"
    );
    // The rail itself paints no track: the capsule is the whole affordance.
    let track = world
        .get::<BackgroundColor>(rail)
        .expect("the rail has a background slot");
    assert_eq!(track.0.to_srgba().alpha, 0.0, "the lane paints no track");
    // The scroll area reserves the lane so no content sits under the capsule.
    let area = world
        .query_filtered::<Entity, With<ScrollArea>>()
        .single(world)
        .expect("the probe mounts a scroll area");
    let padding = world.get::<Node>(area).expect("the area is a node").padding;
    assert_eq!(
        padding.right,
        Val::Rem(LANE_WIDTH_REM),
        "the lane is the shared hit strip on the rem axis"
    );
}
