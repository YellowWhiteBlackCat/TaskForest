//! test-intent: behavior
//!
//! The glass grain is a real shared texture, not a color-fiction: startup
//! decodes the embedded plate, the observer stamps a tiled image with the
//! tier's opacity, and a missing store degrades to no grain rather than
//! panicking.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::system::RunSystemOnce;
use bevy::image::{Image, ImageAddressMode, ImageSampler};
use bevy::math::{Vec2, Vec4};
use bevy::render::render_resource::TextureFormat;
use bevy::scene::ScenePlugin;
use bevy::scene::WorldSceneExt;
use bevy::ui::Node;
use bevy::ui::widget::{ImageNode, NodeImageMode};
use bevy::ui::{BackgroundGradient, Gradient};
use bevy::ui_render::prelude::MaterialNode;
use taskmanager_assets::{GLASS_NOISE_RGBA_SIZE, glass_noise_rgba};
use taskmanager_theme::tokens::UiSize;
use taskmanager_theme::{
    GlassTier, OpticalGlassContext, OpticalGlassFallback, OpticalGlassFeature,
    OpticalGlassGeometry, OpticalGlassQuality, OpticalGlassSpec, OpticalGlassUnavailableReason,
    Theme,
};

use super::optical::{
    OPTICAL_GLASS_MAX_BLUR_PX, OpticalGlassBinding, OpticalGlassMaterial, OpticalGlassMaterialKind,
    OpticalGlassRaster, OpticalGlassSurface, OpticalGlassUniform,
};
use super::optical_wesl::OpticalGlassParams;
use super::{
    GlassNoise, GlassNoiseLayer, GlassPanelMaterial, build_glass_noise, card_material,
    noise_overlay_scene, panel_material, register,
};
use crate::palette::ui_palette;

fn app_with_images() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Image>>();
    app
}

#[test]
fn embedded_grain_decodes_into_one_shared_image() {
    let mut app = app_with_images();
    app.world_mut()
        .run_system_once(build_glass_noise)
        .expect("startup builds the grain");
    let noise = app.world().resource::<GlassNoise>();
    let handle = noise.handle().expect("the embedded plate decodes");
    let image = app
        .world()
        .resource::<Assets<Image>>()
        .get(&handle)
        .expect("the handle resolves in the image store");
    assert_eq!(
        image.data.as_ref().map(Vec::len),
        Some((GLASS_NOISE_RGBA_SIZE * GLASS_NOISE_RGBA_SIZE * 4) as usize)
    );
    assert_eq!(image.data.as_deref(), Some(glass_noise_rgba()));
    let ImageSampler::Descriptor(sampler) = &image.sampler else {
        panic!("the grain must use its own repeating sampler");
    };
    assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
    assert_eq!(sampler.address_mode_v, ImageAddressMode::Repeat);
}

#[test]
fn observer_stamps_a_tiled_grain_below_content() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let mut app = app_with_images();
    register(&mut app);
    let world = app.world_mut();
    world
        .run_system_once(build_glass_noise)
        .expect("grain startup");
    let root = world
        .spawn_scene(noise_overlay_scene(
            GlassTier::Card,
            &palette,
            palette.panel_radius_rem(),
        ))
        .expect("the grain node spawns")
        .id();
    app.update();

    let world = app.world();
    let layer = world
        .get::<GlassNoiseLayer>(root)
        .expect("the declared tier strength survives");
    assert_eq!(layer.strength, GlassTier::Card.noise_strength());
    assert_eq!(
        layer.ink, palette.card_shadow,
        "the grain ink is the theme shadow ink"
    );
    let gradient = world
        .get::<BackgroundGradient>(root)
        .expect("the surface has a sheen");
    let Gradient::Linear(gradient) = &gradient.0[0] else {
        panic!("the glass sheen must be linear");
    };
    assert_eq!(gradient.stops[0].color, palette.glass_sheen);
    assert_eq!(gradient.stops[1].color.to_srgba().alpha, 0.0);
    let image = world
        .get::<ImageNode>(root)
        .expect("the observer stamps the tiled image");
    assert_eq!(
        image.image,
        app.world().resource::<GlassNoise>().handle().unwrap()
    );
    assert!(matches!(
        image.image_mode,
        NodeImageMode::Tiled {
            tile_x: true,
            tile_y: true,
            ..
        }
    ));
    let alpha = image.color.to_srgba().alpha;
    assert!(
        alpha > 0.0 && alpha < 0.1,
        "grain stays subtle, got {alpha}"
    );
}

#[test]
fn headless_without_an_image_store_keeps_the_overlay_invisible() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    register(&mut app);
    let world = app.world_mut();
    let root = world
        .spawn_scene(noise_overlay_scene(
            GlassTier::Modal,
            &palette,
            palette.panel_radius_rem(),
        ))
        .expect("the overlay scene still spawns")
        .id();
    app.update();
    let world = app.world();
    let image = world
        .get::<ImageNode>(root)
        .expect("the node keeps a transparent image payload");
    assert_eq!(image.image, bevy::asset::Handle::default());
}

#[test]
fn panel_material_binds_every_card_dimension_to_the_palette() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let GlassPanelMaterial {
        fill,
        edge,
        shadows,
        edge_width_rem,
        radius_rem,
        grain,
    } = panel_material(GlassTier::Card, &palette);
    assert_eq!(fill, palette.glass_fill(GlassTier::Card));
    assert_eq!(edge, palette.glass_edge(GlassTier::Card));
    assert_eq!(shadows, palette.glass_shadow(GlassTier::Card));
    assert_eq!(edge_width_rem, GlassTier::Card.edge_width_rem());
    assert_eq!(radius_rem, palette.panel_radius_rem());

    let mut app = app_with_images();
    register(&mut app);
    let root = app
        .world_mut()
        .spawn_scene(grain)
        .expect("the card grain scene mounts")
        .id();
    assert_eq!(
        app.world()
            .get::<GlassNoiseLayer>(root)
            .expect("the card material owns a card-tier grain")
            .strength,
        GlassTier::Card.noise_strength()
    );
}

#[test]
fn card_material_is_the_floating_card_tier() {
    let palette = ui_palette(&Theme::dark(), UiSize::Large);
    let GlassPanelMaterial {
        fill,
        edge,
        edge_width_rem,
        radius_rem,
        ..
    } = card_material(&palette);
    assert_eq!(fill, palette.glass_fill(GlassTier::Card));
    assert_eq!(edge, palette.glass_edge(GlassTier::Card));
    assert_eq!(edge_width_rem, GlassTier::Card.edge_width_rem());
    assert_eq!(radius_rem, palette.panel_radius_rem());
}

fn card_spec_and_geometry() -> (OpticalGlassSpec, OpticalGlassGeometry) {
    let theme = Theme::dark();
    let spec = theme
        .optical_glass_spec(GlassTier::Card)
        .expect("the card has optical design intent");
    let geometry = spec
        .resolve(OpticalGlassContext::new(theme.root_rem_size, 2.0))
        .expect("the card geometry resolves");
    (spec, geometry)
}

fn card_raster() -> OpticalGlassRaster {
    OpticalGlassRaster {
        origin_px: Vec2::new(220.0, 150.0),
        size_px: Vec2::new(520.0, 300.0),
        viewport_px: Vec2::new(1600.0, 900.0),
        device_scale_factor: 2.0,
    }
}

#[test]
fn optical_uniform_projects_the_shared_spec_without_private_constants() {
    let (spec, geometry) = card_spec_and_geometry();
    let uniform = OpticalGlassUniform::project(
        spec,
        geometry,
        card_raster(),
        OpticalGlassMaterialKind::Optical,
    )
    .expect("valid placement projects");

    assert_eq!(
        uniform.optics,
        Vec4::new(
            geometry.blur_radius_px,
            geometry.refraction_strength_px,
            geometry.refraction_width_px,
            spec.chromatic_dispersion,
        )
    );
    assert_eq!(
        uniform.finish,
        Vec4::new(
            geometry.specular_width_px,
            spec.specular_intensity,
            spec.frost_opacity,
            1.0,
        ),
        "balanced quality is encoded in the fourth lane"
    );
    assert_eq!(uniform.tint, {
        let linear = crate::palette::theme_color(spec.tint).to_linear();
        Vec4::new(linear.red, linear.green, linear.blue, spec.tint.a)
    });
    assert_eq!(uniform.grain.w, spec.grain_strength);
    assert_eq!(uniform.element, Vec4::new(220.0, 150.0, 520.0, 300.0));
    assert_eq!(
        uniform.refraction,
        Vec2::new(
            OpticalGlassParams::DEFAULT_IOR,
            OpticalGlassParams::from_spec(spec, geometry)
                .admitted(OPTICAL_GLASS_MAX_BLUR_PX)
                .expect("the card Fresnel lane admits")
                .fresnel_base,
        )
    );
    assert_eq!(
        uniform.viewport,
        Vec4::new(1600.0, 900.0, geometry.corner_radius_px, 2.0)
    );
    assert_eq!(spec.quality, OpticalGlassQuality::Balanced);
}

#[test]
fn optical_uniform_rejects_invalid_physical_placement() {
    let (spec, geometry) = card_spec_and_geometry();
    for raster in [
        OpticalGlassRaster {
            size_px: Vec2::ZERO,
            ..card_raster()
        },
        OpticalGlassRaster {
            viewport_px: Vec2::new(f32::NAN, 900.0),
            ..card_raster()
        },
        OpticalGlassRaster {
            device_scale_factor: 0.0,
            ..card_raster()
        },
    ] {
        assert!(
            OpticalGlassUniform::project(
                spec,
                geometry,
                raster,
                OpticalGlassMaterialKind::Optical,
            )
            .is_none()
        );
    }
}

#[test]
fn optical_surface_observer_attaches_the_real_material_and_textures() {
    let mut app = app_with_images();
    register(&mut app);
    app.world_mut()
        .run_system_once(build_glass_noise)
        .expect("grain startup");
    let backdrop = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            1600,
            900,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let (spec, geometry) = card_spec_and_geometry();
    let entity = app
        .world_mut()
        .spawn((
            Node::default(),
            OpticalGlassSurface {
                kind: OpticalGlassMaterialKind::Optical,
                spec,
                geometry,
                raster: card_raster(),
                backdrop: backdrop.clone(),
            },
        ))
        .id();
    app.update();

    let world = app.world();
    let OpticalGlassBinding::Optical(handle) = world
        .get::<OpticalGlassBinding>(entity)
        .expect("binding state is observable")
    else {
        panic!("complete inputs must bind the optical material");
    };
    let material_node = world
        .get::<MaterialNode<OpticalGlassMaterial>>(entity)
        .expect("the observer attaches the UI material");
    assert_eq!(material_node.0, *handle);

    let material = world
        .resource::<Assets<OpticalGlassMaterial>>()
        .get(handle)
        .expect("the material asset resolves");
    assert_eq!(material.backdrop, backdrop);
    assert_eq!(
        material.grain,
        world
            .resource::<GlassNoise>()
            .handle()
            .expect("the shared grain is registered")
    );
    assert_eq!(
        material.uniform,
        OpticalGlassUniform::project(
            spec,
            geometry,
            card_raster(),
            OpticalGlassMaterialKind::Optical,
        )
        .unwrap()
    );
}

#[test]
fn optical_surface_without_the_shared_grain_stays_on_the_fallback() {
    let mut app = app_with_images();
    register(&mut app);
    let (spec, geometry) = card_spec_and_geometry();
    let backdrop = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            1600,
            900,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let entity = app
        .world_mut()
        .spawn((
            Node::default(),
            OpticalGlassSurface {
                kind: OpticalGlassMaterialKind::Optical,
                spec,
                geometry,
                raster: card_raster(),
                backdrop,
            },
        ))
        .id();
    app.update();

    let world = app.world();
    assert_eq!(
        world.get::<OpticalGlassBinding>(entity),
        Some(&OpticalGlassBinding::Fallback {
            reason: OpticalGlassUnavailableReason::MissingFeature(OpticalGlassFeature::Grain),
            material: OpticalGlassFallback::Painted,
        })
    );
    assert!(
        world
            .get::<MaterialNode<OpticalGlassMaterial>>(entity)
            .is_none(),
        "a partial material must never be painted as optical glass"
    );
}
