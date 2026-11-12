//! test-intent: behavior
//!
//! The theme owns the degraded glass material. These tests prove that the
//! Bevy binding CARRIES that material and PAINTS it, instead of dropping it
//! and painting every degraded surface the same way:
//!
//! - the painted fallback owns the tier's own material and samples nothing;
//! - the opaque fallback flattens the fill, drops the sheen and keeps the
//!   tier's edge;
//! - the frosted fallback keeps the backdrop sampled and blurred.
//!
//! Every assertion reads the painted result — the fill, the edge, the
//! shadow, the sheen gradient, the material's uniform — so a branch that is
//! taken but not painted fails here.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetApp, AssetPlugin, Assets, Handle};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::Image;
use bevy::math::Vec2;
use bevy::render::render_resource::TextureFormat;
use bevy::ui::{BackgroundColor, BackgroundGradient, BorderColor, BoxShadow, Gradient, Node};
use bevy::ui_render::prelude::MaterialNode;
use taskmanager_theme::optical_glass::{
    GlassFallbackBackdrop, OpticalGlassContext, OpticalGlassDecision, OpticalGlassFallback,
    OpticalGlassUnavailableReason,
};
use taskmanager_theme::tokens::UiSize;
use taskmanager_theme::{GlassTier, Theme};

use super::super::{
    OPTICAL_GLASS_MAX_BLUR_PX, OpticalGlassBinding, OpticalGlassMaterial, OpticalGlassMaterialKind,
    OpticalGlassRaster, OpticalGlassSurface, OpticalGlassUniform, materialize_optical_glass,
};
use super::{OpticalGlassCard, SurfacePlan, plan_surface, sync_optical_backdrop_paint};
use crate::glass::GlassNoiseLayer;
use crate::glass::build_glass_noise;
use crate::palette::ui_palette;
use crate::window::WindowPalette;

/// One card with the paint system, its grain layer and its binding.
struct PaintedCard {
    entity: Entity,
    grain: Entity,
}

/// Mount the paint system over one card wearing `binding`.
fn painted_card(theme: &Theme, binding: OpticalGlassBinding) -> (App, PaintedCard) {
    let palette = ui_palette(theme, UiSize::Standard);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
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
            Node::default(),
            BackgroundColor(palette.glass_fill(GlassTier::Card)),
            BorderColor::all(palette.glass_edge(GlassTier::Card)),
            BoxShadow(palette.glass_shadow(GlassTier::Card)),
            binding,
        ))
        .id();
    let grain = app
        .world_mut()
        .spawn((
            GlassNoiseLayer::default(),
            Visibility::Visible,
            BackgroundGradient::default(),
            ChildOf(card),
        ))
        .id();
    app.world_mut()
        .run_system_once(sync_optical_backdrop_paint)
        .expect("the paint system runs");
    (
        app,
        PaintedCard {
            entity: card,
            grain,
        },
    )
}

/// The sheen gradient one painted card wears.
fn sheen_of(app: &App, card: &PaintedCard) -> Gradient {
    let gradient = app
        .world()
        .get::<BackgroundGradient>(card.grain)
        .expect("the grain layer carries the sheen");
    assert_eq!(gradient.0.len(), 1, "the sheen is one gradient");
    gradient.0[0].clone()
}

fn card_raster() -> OpticalGlassRaster {
    OpticalGlassRaster {
        origin_px: Vec2::new(220.0, 150.0),
        size_px: Vec2::new(520.0, 300.0),
        viewport_px: Vec2::new(1600.0, 900.0),
        device_scale_factor: 2.0,
    }
}

/// Materialize the degraded material the way production does.
///
/// The material layer is mounted on its own: the layout resolver answers a
/// headless render honestly, so the degraded material is exercised through
/// the observer that attaches it.
fn frosted_material(theme: &Theme) -> (App, OpticalGlassBinding, Handle<Image>) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.init_resource::<Assets<Image>>();
    app.init_asset::<OpticalGlassMaterial>();
    app.add_observer(materialize_optical_glass);
    app.world_mut()
        .run_system_once(build_glass_noise)
        .expect("the grain is built");
    let backdrop = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            1600,
            900,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let spec = theme
        .optical_glass_spec(GlassTier::Card)
        .expect("the card has optical design intent");
    let geometry = spec
        .resolve(OpticalGlassContext::new(theme.root_rem_size, 2.0))
        .expect("the card geometry resolves");
    let reason = OpticalGlassUnavailableReason::NestedLayerLimitExceeded;
    let entity = app
        .world_mut()
        .spawn(OpticalGlassSurface {
            kind: OpticalGlassMaterialKind::Frosted { reason },
            spec,
            geometry,
            raster: card_raster(),
            backdrop: backdrop.clone(),
        })
        .id();
    app.update();
    let binding = app
        .world()
        .get::<OpticalGlassBinding>(entity)
        .expect("binding state is observable")
        .clone();
    (app, binding, backdrop)
}

/// The painted fallback owns the tier's own material and samples nothing.
#[test]
fn a_painted_fallback_paints_the_tier_material_and_samples_nothing() {
    let theme = Theme::dark();
    let palette = ui_palette(&theme, UiSize::Standard);
    let binding = OpticalGlassBinding::Fallback {
        reason: OpticalGlassUnavailableReason::Headless,
        material: OpticalGlassFallback::Painted,
    };
    let (app, card) = painted_card(&theme, binding.clone());
    let world = app.world();

    assert_eq!(
        world.get::<OpticalGlassBinding>(card.entity),
        Some(&binding),
        "the binding carries the theme's material"
    );
    let surface = palette.glass_fallback_surface(GlassTier::Card, OpticalGlassFallback::Painted);
    assert_eq!(
        surface.backdrop,
        GlassFallbackBackdrop::None,
        "a painted fallback samples nothing behind it"
    );
    assert_eq!(
        world
            .get::<BackgroundColor>(card.entity)
            .expect("card fill")
            .0,
        surface.fill,
        "the painted fallback paints the theme's own fill"
    );
    assert_eq!(
        world
            .get::<BorderColor>(card.entity)
            .expect("card edge")
            .top,
        surface.edge,
        "the painted fallback keeps the tier's edge"
    );
    assert_eq!(
        world.get::<BoxShadow>(card.entity).expect("card shadow").0,
        surface.shadow,
        "the painted fallback keeps the tier's layered shadow"
    );
    assert_eq!(
        world.get::<Visibility>(card.grain).copied(),
        Some(Visibility::Visible),
        "the painted fallback keeps its grain"
    );
    let Gradient::Linear(sheen) = sheen_of(&app, &card) else {
        panic!("the sheen must stay linear");
    };
    assert_eq!(
        sheen.stops[0].color, palette.glass_sheen,
        "the painted fallback keeps the tier's sheen"
    );
    assert!(
        world
            .get::<MaterialNode<OpticalGlassMaterial>>(card.entity)
            .is_none(),
        "a painted fallback attaches no material, so no backdrop is sampled"
    );
}

/// The opaque fallback flattens the fill and drops the sheen.
#[test]
fn an_opaque_fallback_paints_a_flat_opaque_fill_without_the_sheen() {
    let theme = Theme::dark();
    let palette = ui_palette(&theme, UiSize::Standard);
    let binding = OpticalGlassBinding::Fallback {
        reason: OpticalGlassUnavailableReason::HighContrast,
        material: OpticalGlassFallback::Opaque,
    };
    let (app, card) = painted_card(&theme, binding.clone());
    let world = app.world();

    assert_eq!(
        world.get::<OpticalGlassBinding>(card.entity),
        Some(&binding),
        "the binding carries the theme's material"
    );
    let surface = palette.glass_fallback_surface(GlassTier::Card, OpticalGlassFallback::Opaque);
    let painted = palette.glass_fallback_surface(GlassTier::Card, OpticalGlassFallback::Painted);
    assert_eq!(
        surface.backdrop,
        GlassFallbackBackdrop::None,
        "an opaque fallback samples nothing behind it"
    );
    let fill = world
        .get::<BackgroundColor>(card.entity)
        .expect("card fill")
        .0;
    assert_eq!(
        fill, surface.fill,
        "the opaque fallback paints its own fill"
    );
    assert_eq!(
        fill.to_srgba().alpha,
        1.0,
        "the opaque fallback paints a flat opaque surface"
    );
    assert_ne!(
        fill, painted.fill,
        "the opaque fill is the tier's own colour, not the canvas-blended one"
    );
    assert_eq!(
        world
            .get::<BorderColor>(card.entity)
            .expect("card edge")
            .top,
        surface.edge,
        "the opaque fallback keeps the tier's edge"
    );
    assert_eq!(
        world.get::<BoxShadow>(card.entity).expect("card shadow").0,
        surface.shadow,
        "the opaque fallback keeps the tier's layered shadow"
    );
    let Gradient::Linear(sheen) = sheen_of(&app, &card) else {
        panic!("the sheen must stay linear");
    };
    assert_eq!(
        sheen.stops[0].color.to_srgba().alpha,
        0.0,
        "the opaque fallback drops the sheen"
    );
    assert_eq!(
        sheen.stops[1].color.to_srgba().alpha,
        0.0,
        "the opaque fallback drops the sheen"
    );
    assert_eq!(
        world.get::<Visibility>(card.grain).copied(),
        Some(Visibility::Visible),
        "the opaque fallback keeps its grain"
    );
    assert!(
        world
            .get::<MaterialNode<OpticalGlassMaterial>>(card.entity)
            .is_none(),
        "an opaque fallback attaches no material, so no backdrop is sampled"
    );
}

/// The frosted fallback keeps the backdrop sampled and blurred.
#[test]
fn a_frosted_fallback_keeps_the_backdrop_sampled_and_blurred() {
    let theme = Theme::dark();
    let palette = ui_palette(&theme, UiSize::Standard);
    let reason = OpticalGlassUnavailableReason::NestedLayerLimitExceeded;
    let (app, binding, backdrop) = frosted_material(&theme);
    let world = app.world();
    let OpticalGlassBinding::Frosted {
        handle,
        reason: bound,
    } = &binding
    else {
        panic!("a frosted decision must bind the degraded material");
    };
    assert_eq!(*bound, reason, "the binding carries the theme's reason");
    assert!(
        binding.attached_material().is_some(),
        "a frosted material samples the backdrop, so the capture camera wakes"
    );

    let material = world
        .resource::<Assets<OpticalGlassMaterial>>()
        .get(handle)
        .expect("the material asset resolves");
    assert_eq!(material.backdrop, backdrop);
    assert!(
        material.uniform.optics.x > 0.0,
        "the frosted material still blurs the backdrop it samples"
    );
    assert_eq!(
        material.uniform.optics.y, 0.0,
        "the frosted material does not refract"
    );
    assert_eq!(
        material.uniform.optics.z, 0.0,
        "the frosted material carries no refraction band"
    );
    assert_eq!(
        material.uniform.optics.w, 0.0,
        "the frosted material does not disperse"
    );
    assert_eq!(
        material.uniform.finish.y, 0.0,
        "the frosted material has no specular rim"
    );
    assert_eq!(
        material.uniform.grain.w, 0.0,
        "the frosted material has no shader grain"
    );

    // The painted card wears the same material.
    let (paint_app, card) = painted_card(&theme, binding.clone());
    let world = paint_app.world();
    let surface = palette.glass_fallback_surface(GlassTier::Card, OpticalGlassFallback::Frosted);
    assert_eq!(
        surface.backdrop,
        GlassFallbackBackdrop::Blurred,
        "a frosted fallback keeps the backdrop"
    );
    assert_eq!(
        world
            .get::<BackgroundColor>(card.entity)
            .expect("card fill")
            .0,
        Color::NONE,
        "the sampled backdrop paints the fill, so the tier paints none"
    );
    assert_eq!(
        world
            .get::<BorderColor>(card.entity)
            .expect("card edge")
            .top,
        surface.edge,
        "the frosted fallback keeps the tier's edge"
    );
    assert_eq!(
        world.get::<BoxShadow>(card.entity).expect("card shadow").0,
        surface.shadow,
        "the frosted fallback keeps the tier's layered shadow"
    );
    assert_eq!(
        world.get::<Visibility>(card.grain).copied(),
        Some(Visibility::Visible),
        "the frosted fallback keeps its grain"
    );
    let Gradient::Linear(sheen) = sheen_of(&paint_app, &card) else {
        panic!("the sheen must stay linear");
    };
    assert_eq!(
        sheen.stops[0].color, palette.glass_sheen,
        "the frosted fallback keeps the tier's sheen"
    );
}

/// The theme's decision resolves to the material it named.
#[test]
fn the_layout_resolves_the_material_the_theme_named() {
    let theme = Theme::dark();
    let context = OpticalGlassContext::new(theme.root_rem_size, 2.0);
    let backdrop = Handle::<Image>::default();

    let frosted = plan_surface(
        theme,
        GlassTier::Card,
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Frosted,
            reason: OpticalGlassUnavailableReason::NestedLayerLimitExceeded,
        },
        context,
        Some(card_raster()),
        Some(backdrop.clone()),
    );
    let SurfacePlan::Material {
        kind: OpticalGlassMaterialKind::Frosted { reason },
        ..
    } = frosted
    else {
        panic!("a frosted decision must resolve to the degraded material");
    };
    assert_eq!(
        reason,
        OpticalGlassUnavailableReason::NestedLayerLimitExceeded
    );

    let painted = plan_surface(
        theme,
        GlassTier::Card,
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Painted,
            reason: OpticalGlassUnavailableReason::Headless,
        },
        context,
        Some(card_raster()),
        Some(backdrop.clone()),
    );
    assert!(matches!(
        painted,
        SurfacePlan::Painted {
            material: OpticalGlassFallback::Painted,
            reason: OpticalGlassUnavailableReason::Headless,
        }
    ));

    let opaque = plan_surface(
        theme,
        GlassTier::Card,
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Opaque,
            reason: OpticalGlassUnavailableReason::HighContrast,
        },
        context,
        Some(card_raster()),
        Some(backdrop),
    );
    assert!(matches!(
        opaque,
        SurfacePlan::Painted {
            material: OpticalGlassFallback::Opaque,
            reason: OpticalGlassUnavailableReason::HighContrast,
        }
    ));
}

/// A frosted fallback that cannot sample must not pretend to.
#[test]
fn a_frosted_fallback_without_a_backdrop_paints_instead_of_pretending() {
    let theme = Theme::dark();
    let context = OpticalGlassContext::new(theme.root_rem_size, 2.0);
    let plan = plan_surface(
        theme,
        GlassTier::Card,
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Frosted,
            reason: OpticalGlassUnavailableReason::BlurLimitExceeded,
        },
        context,
        Some(card_raster()),
        None,
    );
    assert!(
        matches!(
            plan,
            SurfacePlan::Painted {
                material: OpticalGlassFallback::Painted,
                reason: OpticalGlassUnavailableReason::BlurLimitExceeded,
            }
        ),
        "a frosted fallback with no backdrop to sample paints the tier instead"
    );
}

/// The degraded material keeps its blur inside the renderer's proven
/// ceiling instead of losing the material altogether.
#[test]
fn a_degraded_material_bounds_its_blur_to_the_proven_ceiling() {
    let theme = Theme::dark();
    let spec = theme
        .optical_glass_spec(GlassTier::Card)
        .expect("the card has optical design intent");
    let geometry = spec
        .resolve(OpticalGlassContext::new(theme.root_rem_size, 8.0))
        .expect("the card geometry resolves");
    assert!(
        geometry.blur_radius_px > OPTICAL_GLASS_MAX_BLUR_PX as f32,
        "the fixture is over the renderer's ceiling"
    );

    assert!(
        OpticalGlassUniform::project(
            spec,
            geometry,
            card_raster(),
            OpticalGlassMaterialKind::Optical,
        )
        .is_none(),
        "the complete pipeline still fails closed on an over-ceiling radius"
    );

    let frosted = OpticalGlassUniform::project(
        spec,
        geometry,
        card_raster(),
        OpticalGlassMaterialKind::Frosted {
            reason: OpticalGlassUnavailableReason::BlurLimitExceeded,
        },
    )
    .expect("the degraded material keeps a bounded blur");
    assert_eq!(
        frosted.optics.x, OPTICAL_GLASS_MAX_BLUR_PX as f32,
        "the degraded blur stays inside the proven ceiling"
    );
    assert!(frosted.optics.x > 0.0, "the degraded material still blurs");
}
