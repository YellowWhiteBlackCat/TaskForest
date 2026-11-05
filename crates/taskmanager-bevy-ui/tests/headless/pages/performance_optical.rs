//! test-intent: behavior
//!
//! The Performance page must keep its painted card whenever optical rendering
//! is unavailable, including a live switch to high contrast.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::camera::Camera;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::image::{Image, TextureAtlasLayout};
use bevy::picking::DefaultPickingPlugins;
use bevy::scene::ScenePlugin;
use bevy::text::{Font, TextPlugin};
use bevy::transform::TransformPlugin;
use bevy::ui::{BackgroundColor, UiPlugin};
use bevy::ui_render::prelude::MaterialNode;
use bevy::window::{Window, WindowPlugin, WindowResolution};
use taskmanager_shell::demo_app;
use taskmanager_theme::tokens::UiSize;
use taskmanager_theme::{GlassTier, OpticalGlassUnavailableReason, Theme};

use super::{CurveCard, SystemCurve};
use crate::app::{FrontendTrack, Page, Route};
use crate::glass::optical::{OpticalGlassBinding, OpticalGlassMaterial};
use crate::glass::{GlassNoiseLayer, OpticalBackdropSource, OpticalGlassCard};
use crate::palette::ui_palette;
use crate::widgets::chart::ChartCanvas;
use crate::window::{BackdropCaptureTarget, FrontendWindowPlugin, WindowPalette};

fn windowed_performance_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(WindowPlugin {
        primary_window: Some(Window {
            resolution: WindowResolution::new(1280, 720),
            ..Window::default()
        }),
        ..WindowPlugin::default()
    });
    app.add_plugins((
        AssetPlugin::default(),
        ScenePlugin,
        bevy::input::InputPlugin,
        TransformPlugin,
        TextPlugin,
        UiPlugin,
        DefaultPickingPlugins,
    ));
    app.init_resource::<Assets<Font>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<TextureAtlasLayout>>();
    app.add_plugins(FrontendWindowPlugin {
        runtime: crate::runtime::demo_platform_runtime(),
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });
    app.world_mut().resource_mut::<Route>().page = Page::Performance;
    app.world_mut()
        .get_non_send_mut::<FrontendTrack>()
        .expect("frontend track")
        .shell = demo_app();
    app
}

fn enabled_card(world: &mut bevy::ecs::world::World) -> Entity {
    let mut query = world.query::<(Entity, &OpticalGlassCard)>();
    query
        .iter(world)
        .find(|(_, card)| card.enabled)
        .map(|(entity, _)| entity)
        .expect("the Performance hero card is marked for optical glass")
}

#[test]
fn performance_hero_retains_fallback_without_a_gpu_and_in_high_contrast() {
    let mut app = windowed_performance_app();
    for _ in 0..6 {
        app.update();
    }

    let card = enabled_card(app.world_mut());
    let world = app.world_mut();
    assert_eq!(
        world.get::<OpticalGlassBinding>(card),
        Some(&OpticalGlassBinding::Fallback(
            OpticalGlassUnavailableReason::Headless
        ))
    );
    assert!(
        world
            .get::<MaterialNode<OpticalGlassMaterial>>(card)
            .is_none(),
        "an unavailable renderer does not attach an optical material"
    );
    let palette = &world.resource::<WindowPalette>().inner;
    assert_eq!(
        world.get::<BackgroundColor>(card).expect("card fill").0,
        palette.glass_fill(GlassTier::Card)
    );
    let target = world.resource::<BackdropCaptureTarget>();
    assert!(
        !world
            .get::<Camera>(target.camera)
            .expect("capture camera")
            .is_active,
        "an unsupported renderer does not render a backdrop every frame"
    );

    let mut cards = world.query::<(&CurveCard, &Children)>();
    let card_children = cards
        .iter(world)
        .find(|(curve, _)| curve.0 == SystemCurve::default())
        .map(|(_, children)| children)
        .expect("hero curve card children");
    let grain = card_children
        .iter()
        .find(|child| world.get::<GlassNoiseLayer>(**child).is_some())
        .expect("painted grain fallback");
    assert_eq!(
        world.get::<bevy::camera::visibility::Visibility>(*grain),
        Some(&bevy::camera::visibility::Visibility::Visible),
        "the fallback keeps its grain"
    );

    let mut sources = world.query::<(&OpticalBackdropSource, Option<&ChartCanvas>)>();
    let sources = sources.iter(world).collect::<Vec<_>>();
    assert!(
        sources.iter().any(|(source, _)| source.enabled),
        "the measured chart underlay is marked as the backdrop source"
    );
    assert!(
        sources
            .iter()
            .filter(|(source, _)| source.enabled)
            .all(|(_, canvas)| canvas.is_some_and(|canvas| canvas.style.grid.is_some())),
        "the fallback keeps the chart grid"
    );

    let mut theme = Theme::dark();
    theme.set_high_contrast(true);
    app.world_mut().resource_mut::<WindowPalette>().inner = ui_palette(&theme, UiSize::Standard);
    for _ in 0..3 {
        app.update();
    }
    let world = app.world();
    assert_eq!(
        world.get::<OpticalGlassBinding>(card),
        Some(&OpticalGlassBinding::Fallback(
            OpticalGlassUnavailableReason::HighContrast
        ))
    );
    assert!(
        world
            .get::<MaterialNode<OpticalGlassMaterial>>(card)
            .is_none(),
        "high contrast must never attach an optical material"
    );
    assert_eq!(
        world.get::<BackgroundColor>(card).expect("card fill").0,
        world
            .resource::<WindowPalette>()
            .inner
            .glass_fill(GlassTier::Card)
    );
}
