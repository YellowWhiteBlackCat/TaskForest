//! test-intent: behavior
//!
//! The refractive-parameter boundary clamps what the GPU must not receive
//! malformed, and fails closed on a blur radius the renderer has not
//! proven. These tests pin both behaviors, the lane the uniform carries,
//! and the observer's fallback when an over-ceiling geometry arrives.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::system::RunSystemOnce;
use bevy::image::Image;
use bevy::math::{Vec2, Vec4};
use bevy::render::render_resource::TextureFormat;
use bevy::scene::ScenePlugin;
use bevy::ui::Node;
use bevy::ui_render::prelude::MaterialNode;
use taskmanager_theme::optical_glass::OpticalGlassContext;
use taskmanager_theme::{GlassTier, OpticalGlassUnavailableReason, Theme};

use super::OpticalGlassParams;
use crate::glass::optical::{
    OPTICAL_GLASS_MAX_BLUR_PX, OpticalGlassBinding, OpticalGlassMaterial, OpticalGlassRaster,
    OpticalGlassSurface, OpticalGlassUniform,
};
use crate::glass::{build_glass_noise, register};

fn raster() -> OpticalGlassRaster {
    OpticalGlassRaster {
        origin_px: Vec2::new(220.0, 150.0),
        size_px: Vec2::new(520.0, 300.0),
        viewport_px: Vec2::new(1600.0, 900.0),
        device_scale_factor: 2.0,
    }
}

#[test]
fn ior_clamps_into_the_valid_glass_range() {
    let admit = |ior: f32| {
        OpticalGlassParams::from_raw(ior, 0.0, 8.0)
            .admitted(OPTICAL_GLASS_MAX_BLUR_PX)
            .expect("the IOR admits")
            .ior
    };
    assert_eq!(admit(0.5), 1.0, "below air clamps to no bending");
    assert_eq!(admit(1.5), 1.5, "a stated glass IOR passes through");
    assert_eq!(admit(2.42), 2.42);
    assert_eq!(admit(9.0), 2.42, "above diamond clamps to diamond");
}

#[test]
fn dispersion_clamps_into_the_unit_fraction() {
    let admit = |dispersion: f32| {
        OpticalGlassParams::from_raw(1.5, dispersion, 8.0)
            .admitted(OPTICAL_GLASS_MAX_BLUR_PX)
            .expect("the dispersion admits")
            .dispersion
    };
    assert_eq!(admit(-0.25), 0.0);
    assert_eq!(admit(0.1), 0.1);
    assert_eq!(admit(1.75), 1.0);
}

#[test]
fn nan_fails_closed_and_infinity_clamps_like_any_out_of_range_value() {
    assert!(
        OpticalGlassParams::from_raw(f32::NAN, 0.0, 8.0)
            .admitted(OPTICAL_GLASS_MAX_BLUR_PX)
            .is_none(),
        "a NaN IOR cannot clamp anywhere meaningful"
    );
    assert!(
        OpticalGlassParams::from_raw(1.5, f32::NAN, 8.0)
            .admitted(OPTICAL_GLASS_MAX_BLUR_PX)
            .is_none(),
        "a NaN dispersion cannot clamp anywhere meaningful"
    );
    let clamped = OpticalGlassParams::from_raw(f32::INFINITY, 0.0, 8.0)
        .admitted(OPTICAL_GLASS_MAX_BLUR_PX)
        .expect("an infinite IOR clamps into the glass range");
    assert_eq!(clamped.ior, 2.42);
}

#[test]
fn blur_beyond_the_proven_ceiling_is_rejected_not_clamped() {
    let params = |blur: f32| OpticalGlassParams::from_raw(1.5, 0.0, blur);
    assert!(params(0.0).admitted(OPTICAL_GLASS_MAX_BLUR_PX).is_some());
    assert!(
        params(OPTICAL_GLASS_MAX_BLUR_PX as f32)
            .admitted(OPTICAL_GLASS_MAX_BLUR_PX)
            .is_some(),
        "the exact proven ceiling admits"
    );
    assert!(
        params(OPTICAL_GLASS_MAX_BLUR_PX as f32 + 0.5)
            .admitted(OPTICAL_GLASS_MAX_BLUR_PX)
            .is_none(),
        "an unproven blur must fail closed, not shrink into a smaller claim"
    );
    assert!(params(-1.0).admitted(OPTICAL_GLASS_MAX_BLUR_PX).is_none());
    assert!(
        params(f32::NAN)
            .admitted(OPTICAL_GLASS_MAX_BLUR_PX)
            .is_none()
    );
}

#[test]
fn the_uniform_carries_the_admitted_refraction_lane() {
    let theme = Theme::dark();
    let spec = theme
        .optical_glass_spec(GlassTier::Card)
        .expect("the card has an optical spec");
    let geometry = spec
        .resolve(OpticalGlassContext::new(theme.root_rem_size, 2.0))
        .expect("the card geometry resolves");
    let uniform =
        OpticalGlassUniform::project(spec, geometry, raster()).expect("a valid placement projects");
    assert_eq!(uniform.ior, OpticalGlassParams::DEFAULT_IOR);
    assert_eq!(uniform.optics.w, spec.chromatic_dispersion);
    assert_eq!(
        uniform.optics,
        Vec4::new(
            geometry.blur_radius_px,
            geometry.refraction_strength_px,
            geometry.refraction_width_px,
            spec.chromatic_dispersion,
        )
    );
}

#[test]
fn an_over_ceiling_geometry_falls_back_before_the_gpu() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Image>>();
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
    let theme = Theme::dark();
    let mut spec = theme
        .optical_glass_spec(GlassTier::Card)
        .expect("the card has an optical spec");
    spec.blur_radius_rem = 24.0;
    let geometry = spec
        .resolve(OpticalGlassContext::new(theme.root_rem_size, 2.0))
        .expect("the geometry itself resolves");
    assert!(geometry.blur_radius_px > OPTICAL_GLASS_MAX_BLUR_PX as f32);
    let entity = app
        .world_mut()
        .spawn((
            Node::default(),
            OpticalGlassSurface {
                spec,
                geometry,
                raster: raster(),
                backdrop,
            },
        ))
        .id();
    app.update();

    let world = app.world();
    assert_eq!(
        world.get::<OpticalGlassBinding>(entity),
        Some(&OpticalGlassBinding::Fallback(
            OpticalGlassUnavailableReason::InvalidSpec
        )),
        "the unproven blur must fall back, never upload"
    );
    assert!(
        world
            .get::<MaterialNode<OpticalGlassMaterial>>(entity)
            .is_none(),
        "a fallback surface must never carry an optical material"
    );
}
