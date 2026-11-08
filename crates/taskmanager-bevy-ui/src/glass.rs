//! Frosted-glass render bridge for Bevy.
//!
//! The theme owns every material value. This module owns only the two toolkit
//! facts Bevy needs beyond a color: the runtime noise texture and the tiled
//! `ImageNode` overlay that paints it. The overlay is a child of the glass
//! panel, below its content, so the sheen and grain add optical texture without
//! owning layout or pointer input.

use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::color::{Alpha, Color};
use bevy::ecs::component::Component;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::image::{Image, ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::picking::Pickable;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::scene::{EntityWorldMutSceneExt, Scene, bsn};
use bevy::ui::prelude::{
    BackgroundColor, BackgroundGradient, BorderRadius, ColorStop, LinearGradient, Node,
    PositionType, ShadowStyle, Val, percent,
};
use bevy::ui::widget::{ImageNode, NodeImageMode};
use taskmanager_assets::{GLASS_NOISE_RGBA_SIZE, glass_noise_rgba};
use taskmanager_theme::GlassTier;

use crate::palette::UiPalette;

pub(crate) mod optical;
pub(crate) use optical::{OpticalBackdropSource, OpticalGlassCard};
pub(crate) mod optical_wesl;

/// Decode the shared neutral grain into a runtime image. The packaging test
/// owns the plate contract; a wrong-sized blob is an honest absence rather
/// than malformed GPU input.
fn noise_image() -> Option<Image> {
    let rgba = glass_noise_rgba();
    let expected = (GLASS_NOISE_RGBA_SIZE * GLASS_NOISE_RGBA_SIZE * 4) as usize;
    if rgba.len() != expected {
        return None;
    }
    let mut image = Image::new(
        Extent3d {
            width: GLASS_NOISE_RGBA_SIZE,
            height: GLASS_NOISE_RGBA_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba.to_vec(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    Some(image)
}

/// Handle store for the one shared glass grain. Built once at startup, like
/// the semantic icon plates, so every glass surface reuses one texture.
#[derive(Resource, Default)]
pub(crate) struct GlassNoise {
    handle: Option<Handle<Image>>,
}

impl GlassNoise {
    /// Build from the image store; a headless composition without an image
    /// store simply has no grain overlay.
    #[must_use]
    pub(crate) fn build(images: &mut Assets<Image>) -> Self {
        Self {
            handle: noise_image().map(|image| images.add(image)),
        }
    }

    #[must_use]
    pub(crate) fn handle(&self) -> Option<Handle<Image>> {
        self.handle.clone()
    }
}

/// Marker on the tiled grain child of a glass surface. `Default` is the
/// `bsn!` template seed only; the palette overwrites it at spawn time.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct GlassNoiseLayer {
    pub(crate) strength: f32,
    pub(crate) ink: Color,
}

/// One panel tier projected into the Bevy material components. The theme and
/// palette remain the authorities; this struct only bundles their output so
/// page scenes cannot drift on fill, edge, radius, shadow, or grain.
pub(crate) struct GlassPanelMaterial {
    pub(crate) fill: Color,
    pub(crate) edge: Color,
    pub(crate) shadows: Vec<ShadowStyle>,
    pub(crate) edge_width_rem: f32,
    pub(crate) radius_rem: f32,
    pub(crate) grain: Box<dyn Scene>,
}

/// Resolve one panel tier from the active palette.
pub(crate) fn panel_material(tier: GlassTier, palette: &UiPalette) -> GlassPanelMaterial {
    let radius_rem = palette.panel_radius_rem();
    GlassPanelMaterial {
        fill: palette.glass_fill(tier),
        edge: palette.glass_edge(tier),
        shadows: palette.glass_shadow(tier),
        edge_width_rem: tier.edge_width_rem(),
        radius_rem,
        grain: Box::new(noise_overlay_scene(tier, palette, radius_rem)),
    }
}

/// Resolve the shared floating-card tier.
pub(crate) fn card_material(palette: &UiPalette) -> GlassPanelMaterial {
    panel_material(GlassTier::Card, palette)
}

/// One tiled grain overlay, absolutely filling its glass parent.
pub(crate) fn noise_overlay_scene(
    tier: GlassTier,
    palette: &UiPalette,
    radius_rem: f32,
) -> impl Scene + use<> {
    let strength = tier.noise_strength();
    let ink = palette.card_shadow;
    let sheen = if tier == GlassTier::Scrim {
        Color::NONE
    } else {
        palette.glass_sheen
    };
    let gradient = BackgroundGradient::from(LinearGradient::to_bottom(vec![
        ColorStop::auto(sheen),
        ColorStop::auto(sheen.with_alpha(0.0)),
    ]));
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: percent(0),
            top: percent(0),
            border_radius: BorderRadius::all(Val::Rem(radius_rem)),
        }
        BackgroundColor(Color::NONE)
        BackgroundGradient({ gradient.0 })
        Pickable::IGNORE
        GlassNoiseLayer { strength, ink }
        ImageNode
    }
}

/// Observer: resolve the shared grain handle and paint it with the tier's
/// opacity. The image itself is near-white, so multiplying its luminance by
/// the theme's soft shadow ink gives a neutral texture that works on both
/// light and dark skins.
pub(crate) fn stamp_glass_noise(
    trigger: On<Add<GlassNoiseLayer>>,
    noise: Option<Res<GlassNoise>>,
    layers: Query<&GlassNoiseLayer>,
    mut commands: Commands,
) {
    let entity = trigger.event().entity;
    let Ok(layer) = layers.get(entity) else {
        return;
    };
    let handle = noise.as_ref().and_then(|noise| noise.handle());
    let tint = layer.ink.with_alpha(layer.ink.alpha() * layer.strength);
    let image = handle.unwrap_or_default();
    commands
        .entity(entity)
        .queue_silenced(move |mut entity: bevy::ecs::world::EntityWorldMut| {
            let _ = entity.apply_scene(bsn! {
                ImageNode {
                    color: { tint },
                    image: { image },
                    image_mode: NodeImageMode::Tiled {
                        tile_x: true,
                        tile_y: true,
                        stretch_value: 1.0,
                    },
                }
            });
        });
}

/// Startup: decode the grain once into the shared image store.
pub(crate) fn build_glass_noise(images: Option<ResMut<Assets<Image>>>, mut commands: Commands) {
    let Some(mut images) = images else {
        return;
    };
    commands.insert_resource(GlassNoise::build(&mut images));
}

/// Register the one observer that turns the declared grain node into a tiled
/// image.
pub(crate) fn register(app: &mut bevy::app::App) {
    optical::register(app);
    app.add_observer(stamp_glass_noise);
}

#[cfg(test)]
#[path = "../tests/headless/glass.rs"]
mod tests;
