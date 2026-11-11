//! test-intent: behavior
//!
//! The optical shader's offscreen lower composite must contain the product's
//! real painted glass surfaces, not a flat canvas. These tests drive the
//! production mirror system against measured physical geometry and prove
//! that a mirror is placed, re-painted on a palette change, invalidated on a
//! measured change, and swept when its source disappears.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Handle};
use bevy::ecs::entity::Entity;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::Image;
use bevy::math::{UVec2, Vec2};
use bevy::picking::Pickable;
use bevy::scene::ScenePlugin;
use bevy::ui::prelude::{BackgroundColor, BorderColor, BoxShadow, Node, Val};
use bevy::ui::{ComputedNode, UiGlobalTransform, UiTargetCamera};
use taskmanager_theme::fonts::ResolvedFonts;
use taskmanager_theme::tokens::UiSize;
use taskmanager_theme::{GlassTier, HighContrast, LightDark, Skin, Theme};

use super::super::backdrop_mirror::{
    BackdropMirror, BackdropMirrorSlot, painted_tier, sync_backdrop_mirrors,
};
use crate::palette::ui_palette;
use crate::window::{AppShellRoot, BackdropCaptureTarget, BackdropRevision, WindowPalette};

fn computed(size: Vec2, inverse_scale_factor: f32) -> ComputedNode {
    ComputedNode {
        size,
        unrounded_size: size,
        inverse_scale_factor,
        ..ComputedNode::default()
    }
}

fn backdrop(camera: Entity) -> BackdropCaptureTarget {
    BackdropCaptureTarget {
        image: Handle::<Image>::default(),
        camera,
        physical_size: UVec2::new(800, 600),
        scale_factor: 2.0,
    }
}

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<BackdropRevision>();
    let camera = app.world_mut().spawn_empty().id();
    app.insert_resource(backdrop(camera));
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    app.world_mut().spawn((
        AppShellRoot,
        computed(Vec2::new(800.0, 600.0), 1.0),
        UiGlobalTransform::from_xy(400.0, 300.0),
    ));
    let source = app
        .world_mut()
        .spawn((
            BackdropMirror {
                tier: GlassTier::Card,
            },
            computed(Vec2::new(200.0, 100.0), 0.5),
            UiGlobalTransform::from_xy(300.0, 150.0),
        ))
        .id();
    (app, camera, source)
}

#[test]
fn layered_shadow_projection_preserves_the_marked_surface_bounds_in_the_backdrop_mirror() {
    let (mut app, camera, source) = fixture();
    app.world_mut()
        .run_system_once(sync_backdrop_mirrors)
        .expect("the mirror system runs");
    app.update();

    let world = app.world_mut();
    let palette = world.resource::<WindowPalette>().inner.clone();
    let appearance = painted_tier(GlassTier::Card, &palette);
    let mut slots = world.query_filtered::<(Entity, &BackdropMirrorSlot), ()>();
    let slots: Vec<_> = slots
        .iter(world)
        .map(|(entity, slot)| (entity, *slot))
        .collect();
    assert_eq!(slots.len(), 1, "one source owns one mirror");
    let (mirror, slot) = slots[0];
    assert_eq!(slot.0, Some(source), "the mirror remembers its source");
    let node = world.get::<Node>(mirror).expect("mirror node");
    assert_eq!(node.left, Val::Px(100.0));
    assert_eq!(node.top, Val::Px(50.0));
    assert_eq!(node.width, Val::Px(100.0));
    assert_eq!(node.height, Val::Px(50.0));
    assert_eq!(
        world.get::<BackgroundColor>(mirror).map(|fill| fill.0),
        Some(appearance.fill),
        "the mirror paints the tier's fallback fill"
    );
    assert_eq!(
        world.get::<BorderColor>(mirror).map(|edge| edge.top),
        Some(appearance.edge)
    );
    assert_eq!(
        world
            .get::<BoxShadow>(mirror)
            .map(|shadow| shadow.0.clone()),
        Some(appearance.shadows),
        "the mirror carries the shared layered shadow"
    );
    assert_eq!(
        world.get::<UiTargetCamera>(mirror).map(|target| target.0),
        Some(camera),
        "the mirror renders into the offscreen camera"
    );
    assert_eq!(
        world.get::<Pickable>(mirror),
        Some(&Pickable::IGNORE),
        "the mirror never steals pointer routing"
    );
    assert_eq!(
        world.resource::<BackdropRevision>().0,
        1,
        "a new mirror invalidates the captured lower composite"
    );
}

#[test]
fn a_moved_mirror_republishes_the_composite_and_a_stale_slot_is_swept() {
    let (mut app, _, source) = fixture();
    app.world_mut()
        .run_system_once(sync_backdrop_mirrors)
        .expect("the first mirror pass runs");
    app.update();
    let mirror = app
        .world_mut()
        .query_filtered::<Entity, bevy::ecs::query::With<BackdropMirrorSlot>>()
        .iter(app.world())
        .next()
        .expect("one mirror");
    app.world_mut().entity_mut(source).insert((
        computed(Vec2::new(240.0, 120.0), 0.5),
        UiGlobalTransform::from_xy(340.0, 180.0),
    ));
    app.world_mut()
        .run_system_once(sync_backdrop_mirrors)
        .expect("the moved mirror pass runs");
    assert_eq!(
        app.world().resource::<BackdropRevision>().0,
        2,
        "a measured move invalidates the composite"
    );
    assert_eq!(
        app.world().get::<Node>(mirror).expect("mirror node").left,
        Val::Px(110.0)
    );

    app.world_mut().entity_mut(source).despawn();
    app.world_mut()
        .run_system_once(sync_backdrop_mirrors)
        .expect("the stale mirror pass runs");
    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, bevy::ecs::query::With<BackdropMirrorSlot>>()
            .iter(app.world())
            .count(),
        0,
        "a mirror without a live source is swept"
    );
    assert_eq!(app.world().resource::<BackdropRevision>().0, 3);
}

#[test]
fn a_palette_change_repaints_the_mirror_without_a_second_node() {
    let (mut app, _, _) = fixture();
    app.world_mut()
        .run_system_once(sync_backdrop_mirrors)
        .expect("the first mirror pass runs");
    app.update();
    let mirror = app
        .world_mut()
        .query_filtered::<Entity, bevy::ecs::query::With<BackdropMirrorSlot>>()
        .iter(app.world())
        .next()
        .expect("one mirror");
    let light = ui_palette(
        &Theme::build(
            Skin::Gnome,
            LightDark::Light,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        ),
        UiSize::Standard,
    );
    app.insert_resource(WindowPalette {
        inner: light.clone(),
    });
    app.world_mut()
        .run_system_once(sync_backdrop_mirrors)
        .expect("the palette mirror pass runs");

    let appearance = painted_tier(GlassTier::Card, &light);
    let world = app.world_mut();
    assert_eq!(
        world.get::<BackgroundColor>(mirror).map(|fill| fill.0),
        Some(appearance.fill),
        "the light-tier fill replaces the dark-tier fill"
    );
    let slots = world
        .query_filtered::<Entity, bevy::ecs::query::With<BackdropMirrorSlot>>()
        .iter(world)
        .count();
    assert_eq!(slots, 1, "a palette change never duplicates the mirror");
    assert_eq!(world.resource::<BackdropRevision>().0, 2);
}

#[test]
fn a_matching_mirror_does_not_invalidate_the_composite() {
    let (mut app, _, _) = fixture();
    app.world_mut()
        .run_system_once(sync_backdrop_mirrors)
        .expect("the first mirror pass runs");
    app.update();
    let before = app.world().resource::<BackdropRevision>().0;
    app.world_mut()
        .run_system_once(sync_backdrop_mirrors)
        .expect("the quiet mirror pass runs");
    assert_eq!(
        app.world().resource::<BackdropRevision>().0,
        before,
        "an unchanged painted surface does not repaint the backdrop"
    );
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, bevy::ecs::query::With<BackdropMirrorSlot>>()
            .iter(app.world())
            .count(),
        1,
        "the quiet pass keeps exactly one mirror"
    );
}
