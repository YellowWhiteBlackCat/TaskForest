//! Production layout and backdrop wiring for optical glass.
//!
//! The page marks one candidate card and its chart underlay. This module
//! resolves the card against Bevy's computed layout and mirrors its chart grid
//! into an inactive capture scaffold. The theme decision keeps the fallback
//! visible while optical capability is unsupported; the mirror is not a real
//! lower composite.

use bevy::app::{App, PostUpdate};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::{Changed, Has, With, Without};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use bevy::picking::Pickable;
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use bevy::scene::{CommandsSceneExt, bsn};
use bevy::ui::UiTargetCamera;
use bevy::ui::{
    BackgroundColor, BorderColor, BoxShadow, ComputedNode, Node, PositionType, ShadowStyle,
    UiGlobalTransform, UiSystems, Val,
};
use bevy::ui_render::prelude::MaterialNode;
use taskmanager_theme::GlassTier;
use taskmanager_theme::optical_glass::{
    OpticalGlassContext, OpticalGlassDecision, OpticalGlassUnavailableReason,
};

use super::nesting::{OpticalNestingDepths, sync_optical_nesting_depths};
use super::{
    OpticalGlassBinding, OpticalGlassMaterial, OpticalGlassRaster, OpticalGlassSurface,
    OpticalGlassUniform, optical_glass_capability,
};
use crate::glass::GlassNoiseLayer;
use crate::palette::UiPalette;
use crate::widgets::chart::canvas::CanvasGrid;
use crate::widgets::chart::painter::paint_chart_canvases;
use crate::widgets::chart::{ChartCanvas, ChartPaintState, ChartStyle};
use crate::window::{AppShellRoot, BackdropCaptureTarget, BackdropChainTarget, WindowPalette};

/// One page card eligible for the production optical path.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OpticalGlassCard {
    pub(crate) tier: GlassTier,
    pub(crate) enabled: bool,
}

impl Default for OpticalGlassCard {
    fn default() -> Self {
        Self {
            tier: GlassTier::Card,
            enabled: false,
        }
    }
}

/// The chart layer that is rendered into the offscreen backdrop.
///
/// The grid is retained here so the primary chart can omit its crisp copy
/// only while the optical material owns that layer; an unsupported or failed
/// binding restores the exact painted grid.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct OpticalBackdropSource {
    pub(crate) enabled: bool,
    pub(crate) grid: Option<CanvasGrid>,
}

/// The one offscreen chart mirror owned by this slice.
#[derive(Component, Clone, Copy, Debug, Default)]
struct OpticalBackdropChartMirror;

type OpticalBackdropMirrorQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut Node, &'static mut ChartCanvas),
    (
        With<OpticalBackdropChartMirror>,
        Without<OpticalBackdropSource>,
    ),
>;

type OpticalGlassCardQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static OpticalGlassCard,
        &'static ComputedNode,
        &'static UiGlobalTransform,
        Option<&'static mut OpticalGlassSurface>,
        Option<&'static OpticalGlassBinding>,
        Has<MaterialNode<OpticalGlassMaterial>>,
    ),
>;

/// The optical layout resolver's inputs. One named bundle keeps the system
/// under Clippy's argument ceiling without hiding a whole `World`.
///
/// The adapter input is `render_adapter: Option<Res<RenderAdapterInfo>>`: a
/// live renderer publishes the adapter, and no adapter means the shared theme
/// must fail closed. The lifetime-annotated field below is the same fact.
#[derive(SystemParam)]
struct OpticalLayoutAccess<'w, 's> {
    palette: Option<Res<'w, WindowPalette>>,
    target: Option<Res<'w, BackdropCaptureTarget>>,
    chain: Option<Res<'w, BackdropChainTarget>>,
    depths: Res<'w, OpticalNestingDepths>,
    render_device: Option<Res<'w, RenderDevice>>,
    render_adapter: Option<Res<'w, RenderAdapterInfo>>,
    roots: Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<AppShellRoot>>,
    cards: OpticalGlassCardQuery<'w, 's>,
    commands: Commands<'w, 's>,
}

pub(crate) fn register(app: &mut App) {
    app.add_systems(
        PostUpdate,
        sync_optical_backdrop_chart
            .after(UiSystems::PostLayout)
            .before(paint_chart_canvases),
    )
    .add_systems(
        PostUpdate,
        (
            sync_optical_nesting_depths,
            sync_optical_glass_layout,
            sync_optical_glass_materials,
            sync_optical_backdrop_paint,
        )
            .chain()
            .after(UiSystems::PostLayout)
            .after(UiSystems::Stack)
            .before(paint_chart_canvases),
    );
}

/// The painted/optical ownership system's declared access set.
#[derive(SystemParam)]
pub(crate) struct OpticalPaintAccess<'w, 's> {
    palette: Option<Res<'w, WindowPalette>>,
    sources: Query<
        'w,
        's,
        (
            Entity,
            &'static OpticalBackdropSource,
            &'static mut ChartCanvas,
        ),
    >,
    parents: Query<'w, 's, &'static ChildOf>,
    children: Query<'w, 's, &'static Children>,
    background: Query<'w, 's, (&'static mut BackgroundColor, &'static mut BorderColor)>,
    shadows: Query<'w, 's, &'static mut BoxShadow>,
    grain_layers: Query<'w, 's, &'static mut Visibility, With<GlassNoiseLayer>>,
}

/// Preserve the painted card until GPU readiness can be observed.
pub(crate) fn sync_optical_backdrop_paint(
    cards: Query<(Entity, &OpticalGlassCard, Option<&OpticalGlassBinding>)>,
    mut access: OpticalPaintAccess<'_, '_>,
) {
    let Some(palette) = access.palette else {
        return;
    };
    let OpticalPaintAccess {
        ref mut sources,
        ref parents,
        ref children,
        ref mut background,
        ref mut shadows,
        ref mut grain_layers,
        ..
    } = access;
    for (card_entity, card, binding) in &cards {
        let is_attached = matches!(binding, Some(OpticalGlassBinding::Attached(_)));
        if let Ok((mut fill, mut border)) = background.get_mut(card_entity) {
            let (next_fill, next_edge) = if is_attached {
                (Color::NONE, Color::NONE)
            } else {
                (
                    palette.inner.glass_fill(card.tier),
                    palette.inner.glass_edge(card.tier),
                )
            };
            if fill.0 != next_fill {
                fill.0 = next_fill;
            }
            if !border_matches(&border, next_edge) {
                *border = BorderColor::all(next_edge);
            }
        }
        if let Ok(mut shadows) = shadows.get_mut(card_entity) {
            let next_shadows = optical_shadow(&palette.inner, card.tier, is_attached);
            if shadows.0 != next_shadows {
                shadows.0 = next_shadows;
            }
        }
        if let Ok(card_children) = children.get(card_entity) {
            let next_vis = if is_attached {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
            for child in card_children.iter() {
                if let Ok(mut visibility) = grain_layers.get_mut(*child)
                    && *visibility != next_vis
                {
                    *visibility = next_vis;
                }
            }
        }

        for (entity, source, mut canvas) in sources.iter_mut() {
            if !is_descendant(entity, card_entity, parents) {
                continue;
            }
            if canvas.style.grid != source.grid {
                canvas.style.grid = source.grid;
            }
        }
    }
}

/// Mirror the visible source chart into the offscreen camera.
///
/// The mirror carries only the grid that the optical material consumes, never
/// the curve wash, line, hover state, text or glass card. The primary chart
/// keeps its wash and crisp curve above the glass, so the backdrop cannot
/// double-composite the same translucent fill.
fn sync_optical_backdrop_chart(
    target: Option<Res<BackdropCaptureTarget>>,
    roots: Query<(&ComputedNode, &UiGlobalTransform), With<AppShellRoot>>,
    sources: Query<(
        &OpticalBackdropSource,
        &ChartCanvas,
        &ComputedNode,
        &UiGlobalTransform,
    )>,
    mut mirrors: OpticalBackdropMirrorQuery,
    mut commands: Commands,
) {
    let Some(target) = target else {
        return;
    };
    let Ok((root, root_transform)) = roots.single() else {
        return;
    };
    let source = sources
        .iter()
        .find(|(source, _, node, _)| source.enabled && !node.is_empty() && node.size().is_finite());
    let Some((source_marker, source_canvas, source_node, source_transform)) = source else {
        for (entity, _, _) in &mirrors {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some(raster) =
        OpticalGlassRaster::from_layout(source_node, source_transform, root, root_transform)
    else {
        return;
    };
    let mirror_canvas = ChartCanvas {
        content: source_canvas.content.clone(),
        style: backdrop_chart_style(source_canvas.style, source_marker.grid),
    };
    let inverse_scale = raster.device_scale_factor.recip();
    let origin = raster.origin_px * inverse_scale;
    let size = raster.size_px * inverse_scale;
    let left = Val::Px(origin.x);
    let top = Val::Px(origin.y);
    let width = Val::Px(size.x);
    let height = Val::Px(size.y);

    let mut current = mirrors.iter_mut();
    if let Some((_entity, mut node, mut canvas)) = current.next() {
        if node.left != left || node.top != top || node.width != width || node.height != height {
            node.left = left;
            node.top = top;
            node.width = width;
            node.height = height;
        }
        if *canvas != mirror_canvas {
            *canvas = mirror_canvas;
        }
        for (extra, _, _) in current {
            commands.entity(extra).despawn();
        }
        return;
    }

    commands.spawn_scene(bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: {left},
            top: {top},
            width: {width},
            height: {height},
        }
        ChartCanvas {
            content: {mirror_canvas.content.clone()},
            style: {mirror_canvas.style},
        }
        ChartPaintState
        Pickable::IGNORE
        UiTargetCamera({target.camera})
        OpticalBackdropChartMirror
    });
}

/// Resolve the marked card from computed physical geometry and bind the
/// backdrop material. Re-solving on layout changes updates the GPU uniform
/// without rebuilding the page tree.
fn sync_optical_glass_layout(access: OpticalLayoutAccess<'_, '_>) {
    let OpticalLayoutAccess {
        palette,
        target,
        chain,
        depths,
        render_device,
        render_adapter,
        roots,
        mut cards,
        mut commands,
    } = access;
    let Some(palette) = palette else {
        return;
    };
    let root = roots.single().ok();
    for (entity, card, node, transform, surface, binding, has_material) in &mut cards {
        if !card.enabled {
            if surface.is_some() || binding.is_some() || has_material {
                commands.entity(entity).try_remove::<(
                    OpticalGlassSurface,
                    OpticalGlassBinding,
                    MaterialNode<OpticalGlassMaterial>,
                )>();
            }
            continue;
        }
        let raster = root.and_then(|(root, root_transform)| {
            OpticalGlassRaster::from_layout(node, transform, root, root_transform)
        });
        let scale_factor = raster
            .map(|raster| raster.device_scale_factor)
            .or_else(|| target.as_ref().map(|target| target.scale_factor))
            .unwrap_or(1.0);
        // The nesting depth is the theme's fail-closed input: it is the
        // number of optical surfaces painted under this one, measured from
        // the UI stack and the rectangles they share.
        let depth = depths.depth_of(entity);
        let context =
            OpticalGlassContext::new(palette.inner.rem_px, scale_factor).with_nesting_depth(depth);
        let capability = optical_glass_capability(render_adapter.as_deref());
        let context = if render_device.is_none() || target.is_none() {
            context.headless()
        } else {
            context
        };
        let decision = palette
            .inner
            .optical_theme
            .optical_glass_decision(card.tier, capability, context);
        // A nested surface samples the CHAIN target: its backdrop then holds
        // the lower surface's real optical output. A top-level surface
        // samples the painted lower composite, which is all the product
        // shows below it.
        let backdrop = match depth {
            0 => target.as_ref().map(|target| target.image.clone()),
            _ => chain.as_ref().map(|chain| chain.image.clone()),
        };
        let (spec, geometry, raster, backdrop) = match (decision, raster, backdrop) {
            (OpticalGlassDecision::Optical { spec, geometry }, Some(raster), Some(backdrop))
                if raster.is_valid() =>
            {
                (spec, geometry, raster, backdrop)
            }
            (decision, _, _) => {
                let reason = match decision {
                    OpticalGlassDecision::Fallback { reason, .. } => reason,
                    OpticalGlassDecision::Optical { .. } => {
                        OpticalGlassUnavailableReason::InvalidSpec
                    }
                };
                if surface.is_some() || has_material {
                    commands
                        .entity(entity)
                        .try_remove::<(OpticalGlassSurface, MaterialNode<OpticalGlassMaterial>)>();
                }
                let fallback = OpticalGlassBinding::Fallback(reason);
                if binding != Some(&fallback) {
                    commands.entity(entity).try_insert(fallback);
                }
                continue;
            }
        };
        match surface {
            Some(mut surface) => {
                if surface.spec != spec
                    || surface.geometry != geometry
                    || surface.raster != raster
                    || surface.backdrop != backdrop
                {
                    *surface = OpticalGlassSurface {
                        spec,
                        geometry,
                        raster,
                        backdrop: backdrop.clone(),
                    };
                }
            }
            None => {
                commands.entity(entity).try_insert(OpticalGlassSurface {
                    spec,
                    geometry,
                    raster,
                    backdrop,
                });
            }
        }
    }
}

/// Refresh the material asset when layout or DPI changes the surface.
fn sync_optical_glass_materials(
    surfaces: Query<(Entity, &OpticalGlassSurface), Changed<OpticalGlassSurface>>,
    bindings: Query<&OpticalGlassBinding>,
    mut materials: ResMut<bevy::asset::Assets<OpticalGlassMaterial>>,
) {
    for (entity, surface) in &surfaces {
        let Ok(OpticalGlassBinding::Attached(handle)) = bindings.get(entity) else {
            continue;
        };
        let Some(uniform) =
            OpticalGlassUniform::project(surface.spec, surface.geometry, surface.raster)
        else {
            continue;
        };
        if let Some(mut material) = materials.get_mut(handle) {
            material.uniform = uniform;
            material.backdrop = surface.backdrop.clone();
        }
    }
}

fn backdrop_chart_style(source: ChartStyle, grid: Option<CanvasGrid>) -> ChartStyle {
    ChartStyle {
        line: Color::NONE,
        thickness: source.thickness,
        fill: None,
        grid,
        flow: None,
        format: None,
    }
}

/// Keep the theme's layered elevation visible while the optical material owns
/// the panel's inner refraction and rim. Both paths share the same ambient /
/// contact geometry; the optical path discards only the contact layer's ink,
/// which the WESL Fresnel rim replaces at the edge.
fn optical_shadow(palette: &UiPalette, tier: GlassTier, attached: bool) -> Vec<ShadowStyle> {
    let mut shadows = palette.glass_shadow(tier);
    if attached && let Some(contact) = shadows.last_mut() {
        contact.color = Color::NONE;
    }
    shadows
}

fn border_matches(border: &BorderColor, color: Color) -> bool {
    border.left == color && border.top == color && border.right == color && border.bottom == color
}

fn is_descendant(entity: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    let mut current = entity;
    for _ in 0..64 {
        if current == ancestor {
            return true;
        }
        let Ok(parent) = parents.get(current) else {
            return false;
        };
        current = parent.parent();
    }
    false
}

#[cfg(test)]
#[path = "../../../tests/headless/optical_scene.rs"]
mod tests;
