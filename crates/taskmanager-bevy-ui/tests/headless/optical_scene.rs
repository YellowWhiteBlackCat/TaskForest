//! test-intent: behavior
//!
//! These tests drive the production optical scene systems against Bevy's
//! measured physical geometry. They exercise the chart-grid capture scaffold
//! and painted fallback; the scaffold is not a real lower composite.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Handle};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::Image;
use bevy::math::{UVec2, Vec2};
use bevy::scene::ScenePlugin;
use bevy::ui::widget::Text;
use bevy::ui::{
    BackgroundColor, BorderColor, BoxShadow, ComputedNode, Node, UiGlobalTransform, UiTargetCamera,
};
use taskmanager_theme::optical_glass::OpticalGlassFeature;
use taskmanager_theme::tokens::UiSize;
use taskmanager_theme::{GlassTier, OpticalGlassUnavailableReason, Theme};

use super::super::{OpticalGlassBinding, OpticalGlassRaster};
use super::{
    OpticalBackdropChartMirror, OpticalBackdropSource, OpticalGlassCard,
    sync_optical_backdrop_chart, sync_optical_backdrop_paint,
};
use crate::glass::GlassNoiseLayer;
use crate::palette::ui_palette;
use crate::widgets::chart::canvas::{CanvasFill, CanvasGrid};
use crate::widgets::chart::{ChartCanvas, ChartContent, ChartPaintState, ChartStyle};
use crate::window::{AppShellRoot, BackdropCaptureTarget, WindowPalette};

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

fn grid() -> CanvasGrid {
    CanvasGrid {
        color: Color::WHITE,
        alpha: 0.2,
        horizontal: 5,
        vertical: 6,
        reference: &[0.25, 0.5, 0.75],
        reference_alpha: 0.3,
    }
}

#[test]
fn card_layout_resolves_physical_origin_size_and_dpi() {
    let raster = OpticalGlassRaster::from_layout(
        &computed(Vec2::new(200.0, 100.0), 0.5),
        &UiGlobalTransform::from_xy(300.0, 150.0),
        &computed(Vec2::new(800.0, 600.0), 0.5),
        &UiGlobalTransform::from_xy(400.0, 300.0),
    )
    .expect("measured card has a physical raster");
    assert_eq!(
        raster,
        OpticalGlassRaster {
            origin_px: Vec2::new(200.0, 100.0),
            size_px: Vec2::new(200.0, 100.0),
            viewport_px: Vec2::new(800.0, 600.0),
            device_scale_factor: 2.0,
        }
    );
}

#[test]
fn the_capture_scaffold_mirrors_the_chart_grid() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    let camera = app.world_mut().spawn_empty().id();
    app.insert_resource(backdrop(camera));

    let grid = grid();
    let content = ChartContent::Samples {
        values: vec![12.0, 30.0, 24.0, 48.0],
        ceiling: 100.0,
        capacity: 4,
    };
    let source_style = ChartStyle {
        line: Color::WHITE,
        fill: Some(CanvasFill {
            color: Color::WHITE,
            alpha: 0.35,
        }),
        grid: Some(grid),
        ..ChartStyle::default()
    };
    let world = app.world_mut();
    world.spawn((
        AppShellRoot,
        computed(Vec2::new(800.0, 600.0), 1.0),
        UiGlobalTransform::from_xy(400.0, 300.0),
    ));
    world.spawn((
        OpticalBackdropSource {
            enabled: true,
            grid: Some(grid),
        },
        ChartCanvas {
            content: content.clone(),
            style: source_style,
        },
        ChartPaintState::default(),
        computed(Vec2::new(400.0, 200.0), 1.0),
        UiGlobalTransform::from_xy(600.0, 150.0),
    ));

    app.world_mut()
        .run_system_once(sync_optical_backdrop_chart)
        .expect("backdrop mirror system runs");
    app.update();

    let world = app.world_mut();
    let mut mirrors = world.query_filtered::<
        (Entity, &Node, &ChartCanvas),
        bevy::ecs::query::With<OpticalBackdropChartMirror>,
    >();
    let mirrors = mirrors.iter(world).collect::<Vec<_>>();
    assert_eq!(mirrors.len(), 1, "the slice owns exactly one mirror");
    let (mirror, node, canvas) = mirrors[0];
    assert_eq!(canvas.content, content);
    assert_eq!(canvas.style.line, Color::NONE);
    assert_eq!(
        canvas.style.fill, None,
        "the backdrop samples grid structure without double-compositing the chart wash"
    );
    assert_eq!(canvas.style.grid, Some(grid));
    assert_eq!(node.left, bevy::ui::Val::Px(400.0));
    assert_eq!(node.top, bevy::ui::Val::Px(50.0));
    assert_eq!(node.width, bevy::ui::Val::Px(400.0));
    assert_eq!(node.height, bevy::ui::Val::Px(200.0));
    assert_eq!(
        world
            .get::<UiTargetCamera>(mirror)
            .expect("mirror belongs to the capture camera")
            .0,
        camera
    );
    assert!(
        world.get::<Text>(mirror).is_none(),
        "the optical backdrop carries no text; product text stays above the glass"
    );
}

#[test]
fn an_attached_material_does_not_hide_the_painted_fallback() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    let card = app
        .world_mut()
        .spawn((
            OpticalGlassCard {
                tier: GlassTier::Card,
                enabled: true,
            },
            BackgroundColor(palette.glass_fill(GlassTier::Card)),
            BorderColor::all(palette.glass_edge(GlassTier::Card)),
            BoxShadow(palette.glass_shadow(GlassTier::Card)),
            OpticalGlassBinding::Attached(Handle::default()),
        ))
        .id();
    let grain = app
        .world_mut()
        .spawn((
            GlassNoiseLayer::default(),
            Visibility::Visible,
            ChildOf(card),
        ))
        .id();
    let grid = grid();
    let source = app
        .world_mut()
        .spawn((
            OpticalBackdropSource {
                enabled: true,
                grid: Some(grid),
            },
            ChartCanvas {
                content: ChartContent::Empty,
                style: ChartStyle {
                    grid: Some(grid),
                    ..ChartStyle::default()
                },
            },
            ChildOf(card),
        ))
        .id();

    app.world_mut()
        .run_system_once(sync_optical_backdrop_paint)
        .expect("painted/optical ownership sync runs");

    let world = app.world();
    assert_eq!(
        world.get::<BackgroundColor>(card).expect("card fill").0,
        Color::NONE
    );
    assert_eq!(
        world.get::<BorderColor>(card).expect("card edge").top,
        Color::NONE
    );
    let attached = world.get::<BoxShadow>(card).expect("card shadow").0.clone();
    let painted = palette.glass_shadow(GlassTier::Card);
    assert_eq!(
        attached.len(),
        painted.len(),
        "the shared shadow ladder stays"
    );
    for (attached_layer, painted_layer) in attached.iter().zip(painted.iter()) {
        assert_eq!(attached_layer.y_offset, painted_layer.y_offset);
        assert_eq!(attached_layer.blur_radius, painted_layer.blur_radius);
    }
    assert_ne!(
        attached.last().map(|layer| layer.color),
        painted.last().map(|layer| layer.color),
        "the optical material's Fresnel rim replaces the contact ink"
    );
    assert_eq!(
        world.get::<Visibility>(grain).copied(),
        Some(Visibility::Hidden)
    );
    assert_eq!(
        world
            .get::<ChartCanvas>(source)
            .expect("source canvas")
            .style
            .grid,
        Some(grid)
    );

    app.world_mut()
        .entity_mut(card)
        .insert(OpticalGlassBinding::Fallback(
            OpticalGlassUnavailableReason::MissingFeature(OpticalGlassFeature::Grain),
        ));
    app.world_mut()
        .run_system_once(sync_optical_backdrop_paint)
        .expect("fallback ownership sync runs");

    let world = app.world();
    assert_eq!(
        world.get::<BackgroundColor>(card).expect("fallback fill").0,
        palette.glass_fill(GlassTier::Card)
    );
    assert_eq!(
        world.get::<BorderColor>(card).expect("fallback edge").top,
        palette.glass_edge(GlassTier::Card)
    );
    assert_eq!(
        world.get::<BoxShadow>(card).expect("fallback shadow").0,
        palette.glass_shadow(GlassTier::Card),
        "the painted fallback restores the theme's layered shadow"
    );
    assert_eq!(
        world.get::<Visibility>(grain).copied(),
        Some(Visibility::Visible)
    );
    assert_eq!(
        world
            .get::<ChartCanvas>(source)
            .expect("fallback source canvas")
            .style
            .grid,
        Some(grid)
    );
}
