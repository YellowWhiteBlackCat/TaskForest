//! The SECOND optical layer's backdrop compositor.
//!
//! The painted lower composite is a PROXY: it carries the product's real
//! surfaces at their PAINTED tier, because a surface that renders optical
//! has no painted fill to copy. An overlay that floats above such a surface
//! — a tooltip over an optical card, a popover over an optical panel — must
//! therefore refract the LOWER SURFACE'S OPTICAL APPEARANCE, not a stand-in
//! for it. Otherwise the overlay reads as a painted card pasted over a
//! glass one, and the two layers disagree about the light between them.
//!
//! This module closes that gap with a real second render pass:
//!
//! 1. The first capture renders the painted lower composite into the first
//!    target. The first optical layer samples that target.
//! 2. The chain capture renders THAT target as its base and re-renders the
//!    first optical layer's surface with the real optical material into the
//!    chain target. The chain target now holds the first layer's actual
//!    refracted, blurred, dispersed pixels.
//! 3. An overlay's optical material samples the chain target. Its backdrop
//!    therefore contains the first layer's real optical output.
//!
//! The chain supplies PIXELS, not a capability claim: the theme still gates
//! the optical decision, and only a surface whose measured nesting depth the
//! shared ceiling admits may go optical. No painted tier is relabelled
//! optical here.

use bevy::app::PostUpdate;
use bevy::asset::Handle;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, SystemParam};
use bevy::picking::Pickable;
use bevy::scene::{CommandsSceneExt, bsn};
use bevy::ui::prelude::{Node, PositionType, Val, percent, px};
use bevy::ui::widget::{ImageNode, NodeImageMode};
use bevy::ui::{
    ComputedNode, ComputedStackIndex, GlobalZIndex, UiGlobalTransform, UiTargetCamera, ZIndex,
};
use bevy::ui_render::prelude::MaterialNode;

use super::nesting::OpticalNestingDepths;
use super::{OpticalGlassMaterial, OpticalGlassRaster};
use crate::window::{AppShellRoot, BackdropChainTarget};

/// The chain's own base node: the first layer's own output, republished.
///
/// It is an `ImageNode` over the first target's image, so the chain
/// composite starts from the exact pixels the first layer sampled.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct BackdropChainBase;

/// A first-layer optical surface republished into the chain capture.
///
/// The republished node carries the SAME material handle as the live
/// surface, so the chain renders the first layer's real optical output
/// rather than a second approximation of it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct BackdropChainLayer;

/// The live first-layer surface a chain republication stands for.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BackdropChainSource(pub(crate) Option<Entity>);

/// The chain base's stacking band.
///
/// The base carries the painted lower composite the first layer sampled, so
/// it has to sit UNDER every republished layer. A dedicated band keeps that
/// order independent of the tree the republished layers happen to live in.
const CHAIN_BASE_Z: i32 = -1;

/// The chain compositor's inputs. One named bundle keeps the system under
/// Clippy's argument ceiling without hiding a whole `World`.
#[derive(SystemParam)]
pub(crate) struct BackdropChainAccess<'w, 's> {
    chain: Option<Res<'w, BackdropChainTarget>>,
    depths: Res<'w, OpticalNestingDepths>,
    roots: Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<AppShellRoot>>,
    sources: Query<
        'w,
        's,
        (
            Entity,
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static ComputedStackIndex,
            &'static super::OpticalGlassBinding,
        ),
        With<super::OpticalGlassSurface>,
    >,
    republished: Query<'w, 's, (Entity, &'static BackdropChainSource), With<BackdropChainLayer>>,
    materials: Query<'w, 's, &'static mut MaterialNode<OpticalGlassMaterial>>,
    nodes: Query<'w, 's, &'static mut Node>,
    orders: Query<'w, 's, &'static mut ZIndex>,
    commands: Commands<'w, 's>,
}

/// Republish the first optical layer into the second layer's backdrop.
///
/// The system runs after the UI layout, so every republished node sits at
/// the source's MEASURED physical rectangle rather than an authored one.
/// A surface whose material fell back to the painted tier is not
/// republished: there is no optical output to carry, and a painted copy
/// would be the very proxy this module exists to remove. A NESTED surface is
/// excluded as well: it is painted above the first layer, so republishing it
/// would put the overlay inside its own backdrop.
pub(crate) fn sync_backdrop_chain(access: BackdropChainAccess<'_, '_>) {
    let BackdropChainAccess {
        chain,
        depths,
        roots,
        sources,
        republished,
        mut materials,
        mut nodes,
        mut orders,
        mut commands,
    } = access;
    let Some(chain) = chain else {
        return;
    };
    let Ok((root, root_transform)) = roots.single() else {
        return;
    };
    let mut candidates: Vec<(
        Entity,
        u32,
        OpticalGlassRaster,
        Handle<OpticalGlassMaterial>,
    )> = Vec::new();
    for (source, node, transform, stack, binding) in &sources {
        let super::OpticalGlassBinding::Attached(handle) = binding else {
            continue;
        };
        if depths.depth_of(source) > 0 {
            continue;
        }
        let Some(raster) = OpticalGlassRaster::from_layout(node, transform, root, root_transform)
        else {
            continue;
        };
        if !raster.is_valid() {
            continue;
        }
        candidates.push((source, stack.0, raster, handle.clone()));
    }
    // The real paint order decides the order inside the chain composite, so
    // two overlapping first-layer surfaces keep the order the window shows.
    candidates.sort_by_key(|(_, stack_index, _, _)| *stack_index);
    let mut live: Vec<Entity> = Vec::new();
    for (rank, (source, _, raster, handle)) in candidates.iter().enumerate() {
        live.push(*source);
        let inverse_scale = raster.device_scale_factor.recip();
        let origin = raster.origin_px * inverse_scale;
        let size = raster.size_px * inverse_scale;
        let existing = republished
            .iter()
            .find(|(_, slot)| slot.0 == Some(*source))
            .map(|(entity, _)| entity);
        match existing {
            Some(entity) => {
                if let Ok(mut node) = nodes.get_mut(entity) {
                    let left = Val::Px(origin.x);
                    let top = Val::Px(origin.y);
                    let width = Val::Px(size.x);
                    let height = Val::Px(size.y);
                    if node.left != left || node.top != top {
                        node.left = left;
                        node.top = top;
                    }
                    if node.width != width || node.height != height {
                        node.width = width;
                        node.height = height;
                    }
                }
                if let Ok(mut order) = orders.get_mut(entity) {
                    let next = rank as i32;
                    if order.0 != next {
                        order.0 = next;
                    }
                }
                if let Ok(mut material) = materials.get_mut(entity) {
                    material.0 = handle.clone();
                }
            }
            None => {
                commands.spawn_scene(bsn! {
                    Node {
                        position_type: PositionType::Absolute,
                        left: { Val::Px(origin.x) },
                        top: { Val::Px(origin.y) },
                        width: { Val::Px(size.x) },
                        height: { Val::Px(size.y) },
                    }
                    ZIndex({ rank as i32 })
                    MaterialNode::<OpticalGlassMaterial>({ handle.clone() })
                    Pickable::IGNORE
                    UiTargetCamera({ chain.camera })
                    BackdropChainLayer
                    BackdropChainSource({ Some(*source) })
                });
            }
        }
    }
    for (entity, slot) in &republished {
        if !slot.0.is_some_and(|source| live.contains(&source)) {
            commands.entity(entity).despawn();
        }
    }
}

/// Publish the chain's base node: the first layer's own image, full-window.
///
/// The base is created once. It is not a mirror of a painted tier — it is
/// the first optical layer's real output image, republished verbatim.
pub(crate) fn spawn_backdrop_chain_base(
    chain: Option<Res<BackdropChainTarget>>,
    bases: Query<Entity, With<BackdropChainBase>>,
    mut commands: Commands,
) {
    let Some(chain) = chain else {
        return;
    };
    if !bases.is_empty() {
        return;
    }
    commands.spawn_scene(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            min_width: px(0.0),
            min_height: px(0.0),
        }
        GlobalZIndex({ CHAIN_BASE_Z })
        ImageNode {
            image: { chain.source.clone() },
            image_mode: NodeImageMode::Stretch,
        }
        Pickable::IGNORE
        UiTargetCamera({ chain.camera })
        BackdropChainBase
    });
}

/// Register the chain compositor's systems.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.add_systems(
        PostUpdate,
        (spawn_backdrop_chain_base, sync_backdrop_chain)
            .chain()
            .after(bevy::ui::UiSystems::PostLayout)
            .after(bevy::ui::UiSystems::Stack)
            .after(super::nesting::sync_optical_nesting_depths),
    );
}
