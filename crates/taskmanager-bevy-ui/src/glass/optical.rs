//! Production bridge from the shared optical-glass contract to a Bevy UI
//! material.
//!
//! The theme owns every authored value. This module only:
//! - resolves an already-resolved physical geometry into GPU-aligned lanes;
//! - converts the theme's sRGB colors into linear space for shader blending;
//! - binds the backdrop and the shared grain texture.

use bevy::asset::{Asset, AssetApp, Handle, load_internal_asset, uuid_handle};
use bevy::ecs::component::Component;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::image::Image;
use bevy::math::{Vec2, Vec4};
use bevy::reflect::TypePath;
use bevy::render::RenderPlugin;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{Shader, ShaderRef};
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::ui_render::prelude::{MaterialNode, UiMaterial, UiMaterialPlugin};
use taskmanager_theme::color::Color;
use taskmanager_theme::optical_glass::{
    OpticalGlassFeature, OpticalGlassGeometry, OpticalGlassQuality, OpticalGlassSpec,
    OpticalGlassUnavailableReason,
};

use super::GlassNoise;

const OPTICAL_GLASS_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("cbbc6713-31a6-4b99-95d6-5f4c1193832c");

pub(crate) mod capability;
pub(crate) use capability::{OPTICAL_GLASS_MAX_BLUR_PX, optical_glass_capability};
mod backdrop_mirror;
pub(crate) use backdrop_mirror::BackdropMirror;
mod chain;
mod nesting;
pub(crate) use nesting::OpticalNestingDepths;
mod scene;
pub(crate) use scene::{OpticalBackdropSource, OpticalGlassCard};

use super::optical_wesl::OpticalGlassParams;

/// A physical-pixel placement for one glass node in the captured backdrop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OpticalGlassRaster {
    /// Node origin in physical pixels.
    pub(crate) origin_px: Vec2,
    /// Node size in physical pixels.
    pub(crate) size_px: Vec2,
    /// Full offscreen target size in physical pixels.
    pub(crate) viewport_px: Vec2,
    /// Physical pixels per logical pixel at the time this placement was made.
    pub(crate) device_scale_factor: f32,
}

impl OpticalGlassRaster {
    /// Resolve a UI node's physical rectangle against the shell viewport.
    ///
    /// `ComputedNode`, `UiGlobalTransform`, and the shell root are all in
    /// physical pixels. Bevy UI accumulates transforms with y pointing down;
    /// the returned origin uses the same top-left/down convention as the
    /// offscreen texture and shader UV space.
    pub(crate) fn from_layout(
        node: &ComputedNode,
        transform: &UiGlobalTransform,
        root: &ComputedNode,
        root_transform: &UiGlobalTransform,
    ) -> Option<Self> {
        let viewport_px = root.size();
        let size_px = node.size();
        let device_scale_factor = node.inverse_scale_factor.recip();
        if node.is_empty()
            || !viewport_px.is_finite()
            || !size_px.is_finite()
            || !device_scale_factor.is_finite()
            || device_scale_factor <= 0.0
        {
            return None;
        }
        let center = transform.translation;
        let root_center = root_transform.translation;
        Some(Self {
            origin_px: Vec2::new(
                center.x - root_center.x + viewport_px.x * 0.5 - size_px.x * 0.5,
                center.y - root_center.y + viewport_px.y * 0.5 - size_px.y * 0.5,
            ),
            size_px,
            viewport_px,
            device_scale_factor,
        })
    }

    pub(crate) fn is_valid(self) -> bool {
        self.origin_px.is_finite()
            && self.size_px.is_finite()
            && self.viewport_px.is_finite()
            && self.device_scale_factor.is_finite()
            && self.size_px.x > 0.0
            && self.size_px.y > 0.0
            && self.viewport_px.x > 0.0
            && self.viewport_px.y > 0.0
            && self.device_scale_factor > 0.0
    }
}

/// GPU-aligned projection of [`OpticalGlassSpec`] plus its resolved geometry.
///
/// Six aligned lanes keep every field 16-byte aligned and avoid
/// backend-specific packing rules:
/// - `optics`: blur radius, refraction strength, refraction width, dispersion;
/// - `finish`: specular width, specular intensity, frost opacity, quality;
/// - `tint`: linear RGB plus tint mix alpha;
/// - `grain`: linear ink RGB plus strength;
/// - `element`: origin XY, size XY;
/// - `viewport`: target XY, corner radius, device scale factor.
///
/// The trailing `refraction` lane carries the deep refraction's IOR and the
/// theme-derived Schlick F0; both are clamped and admitted by
/// [`OpticalGlassParams`] before upload and stay GPU aligned.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(crate) struct OpticalGlassUniform {
    pub(crate) optics: Vec4,
    pub(crate) finish: Vec4,
    pub(crate) tint: Vec4,
    pub(crate) grain: Vec4,
    pub(crate) element: Vec4,
    pub(crate) viewport: Vec4,
    pub(crate) refraction: Vec2,
}

impl OpticalGlassUniform {
    /// Project the theme-owned spec and resolved geometry into stable lanes.
    ///
    /// Invalid raster placement fails closed before a malformed value reaches
    /// the GPU, and so does a blur radius the proven ceiling never admitted.
    /// The spec/geometry are produced by
    /// [`OpticalGlassSpec::resolve`](taskmanager_theme::OpticalGlassSpec::resolve)
    /// and remain the only source of material values.
    pub(crate) fn project(
        spec: OpticalGlassSpec,
        geometry: OpticalGlassGeometry,
        raster: OpticalGlassRaster,
    ) -> Option<Self> {
        if !raster.is_valid() {
            return None;
        }
        let params =
            OpticalGlassParams::from_spec(spec, geometry).admitted(OPTICAL_GLASS_MAX_BLUR_PX)?;
        Some(Self {
            optics: Vec4::new(
                geometry.blur_radius_px,
                geometry.refraction_strength_px,
                geometry.refraction_width_px,
                params.dispersion,
            ),
            finish: Vec4::new(
                geometry.specular_width_px,
                spec.specular_intensity,
                spec.frost_opacity,
                quality_lane(spec.quality),
            ),
            tint: linear_rgba(spec.tint),
            grain: Vec4::new(
                linear_rgba(spec.grain_ink).x,
                linear_rgba(spec.grain_ink).y,
                linear_rgba(spec.grain_ink).z,
                spec.grain_strength,
            ),
            element: Vec4::new(
                raster.origin_px.x,
                raster.origin_px.y,
                raster.size_px.x,
                raster.size_px.y,
            ),
            viewport: Vec4::new(
                raster.viewport_px.x,
                raster.viewport_px.y,
                geometry.corner_radius_px,
                raster.device_scale_factor,
            ),
            refraction: Vec2::new(params.ior, params.fresnel_base),
        })
    }
}

/// One production UI material sampled from the offscreen backdrop.
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub(crate) struct OpticalGlassMaterial {
    #[uniform(0)]
    pub(crate) uniform: OpticalGlassUniform,
    #[texture(1)]
    #[sampler(2)]
    pub(crate) backdrop: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub(crate) grain: Handle<Image>,
}

/// A production node that asks the binding to attach an optical material.
///
/// The shared decision must have returned `Optical` before a caller attaches
/// this marker. The observer resolves the shared grain handle and projects
/// the already-resolved physical geometry into one material asset.
#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) struct OpticalGlassSurface {
    pub(crate) spec: OpticalGlassSpec,
    pub(crate) geometry: OpticalGlassGeometry,
    pub(crate) raster: OpticalGlassRaster,
    pub(crate) backdrop: Handle<Image>,
}

/// Observable result of materializing an [`OpticalGlassSurface`].
#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) enum OpticalGlassBinding {
    /// The main world attached a material; GPU readiness is not implied.
    Attached(Handle<OpticalGlassMaterial>),
    /// The material is unavailable; the painted fallback remains.
    Fallback(OpticalGlassUnavailableReason),
}

impl UiMaterial for OpticalGlassMaterial {
    fn fragment_shader() -> ShaderRef {
        OPTICAL_GLASS_SHADER_HANDLE.into()
    }
}

/// Register the material, its embedded WESL shader, and the UI material
/// render pipeline. Material attachment alone does not establish renderer
/// capability or GPU readiness.
pub(crate) fn register(app: &mut bevy::app::App) {
    if !app.is_plugin_added::<RenderPlugin>() {
        app.init_asset::<Shader>();
    }
    load_internal_asset!(
        app,
        OPTICAL_GLASS_SHADER_HANDLE,
        "optical_glass.wesl",
        Shader::from_wesl
    );
    app.add_plugins(UiMaterialPlugin::<OpticalGlassMaterial>::default());
    app.add_observer(materialize_optical_glass);
    nesting::register(app);
    scene::register(app);
    backdrop_mirror::register(app);
    chain::register(app);
}

fn materialize_optical_glass(
    trigger: On<Add<OpticalGlassSurface>>,
    surfaces: Query<&OpticalGlassSurface>,
    noise: Option<Res<GlassNoise>>,
    mut materials: ResMut<bevy::asset::Assets<OpticalGlassMaterial>>,
    mut commands: Commands,
) {
    let entity = trigger.event().entity;
    let Ok(surface) = surfaces.get(entity) else {
        return;
    };
    let Some(grain) = noise.as_deref().and_then(GlassNoise::handle) else {
        commands
            .entity(entity)
            .try_insert(OpticalGlassBinding::Fallback(
                OpticalGlassUnavailableReason::MissingFeature(OpticalGlassFeature::Grain),
            ));
        return;
    };
    let Some(uniform) =
        OpticalGlassUniform::project(surface.spec, surface.geometry, surface.raster)
    else {
        commands
            .entity(entity)
            .try_insert(OpticalGlassBinding::Fallback(
                OpticalGlassUnavailableReason::InvalidSpec,
            ));
        return;
    };
    let material = materials.add(OpticalGlassMaterial {
        uniform,
        backdrop: surface.backdrop.clone(),
        grain,
    });
    commands.entity(entity).try_insert((
        MaterialNode(material.clone()),
        OpticalGlassBinding::Attached(material),
    ));
}

fn quality_lane(quality: OpticalGlassQuality) -> f32 {
    match quality {
        OpticalGlassQuality::Low => 0.0,
        OpticalGlassQuality::Balanced => 1.0,
        OpticalGlassQuality::High => 2.0,
    }
}

fn linear_rgba(color: Color) -> Vec4 {
    let linear = crate::palette::theme_color(color).to_linear();
    Vec4::new(linear.red, linear.green, linear.blue, color.a)
}

#[cfg(test)]
#[path = "../../tests/gui/optical_gpu.rs"]
mod gpu_tests;
