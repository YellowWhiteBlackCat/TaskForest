//! Painted lower-composite mirroring for the offscreen optical backdrop.
//!
//! Bevy's UI binds one node to one camera. The interactive shell owns the
//! primary window; the isolated capture root owns the offscreen target the
//! optical shader samples. The shell tree therefore cannot render twice,
//! and without help the shader only ever sees the flat page canvas.
//!
//! This module closes that gap honestly. A marked surface is MIRRORED into
//! the capture root as its PAINTED tier — fill, edge, radius and the shared
//! layered shadow — at its measured physical rectangle. The shader then
//! refracts the product's real surfaces instead of a flat color, and an
//! overlay floating over the lane refracts the cards beneath it.
//!
//! The mirror is painted, never optical: it is a proxy of the lower
//! composite, not the composite itself. The renderer therefore still claims
//! exactly ONE optical layer, and a glass surface inside another glass
//! surface stays an explicit boundary instead of a promise.

use bevy::app::PostUpdate;
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use bevy::picking::Pickable;
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::ui::prelude::{
    BackgroundColor, BorderColor, BorderRadius, BoxShadow, Node, PositionType, ShadowStyle, UiRect,
    Val,
};
use bevy::ui::{ComputedNode, UiGlobalTransform, UiSystems, UiTargetCamera};
use taskmanager_theme::GlassTier;

use super::OpticalGlassRaster;
use crate::palette::UiPalette;
use crate::window::{AppShellRoot, BackdropCaptureTarget, BackdropRevision, WindowPalette};

/// Marks a painted surface whose appearance belongs in the offscreen lower
/// composite the optical shader samples.
///
/// The marker carries the surface's GLASS TIER, not its live fill: a surface
/// that currently renders optical has no painted fill to copy, and the tier
/// is the theme's own statement of what that surface looks like when it is
/// painted. The mirror therefore always paints the tier's fallback
/// appearance, which is exactly the appearance the optical tier replaces.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BackdropMirror {
    /// The glass tier the mirror paints.
    pub(crate) tier: GlassTier,
}

impl Default for BackdropMirror {
    fn default() -> Self {
        Self {
            tier: GlassTier::Card,
        }
    }
}

/// The mirror node's link back to the surface it mirrors.
///
/// The slot is the mirror's own identity: a source without a slot has no
/// mirror yet, and a slot without a live source is stale.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BackdropMirrorSlot(pub(crate) Option<Entity>);

/// The measured logical-pixel rectangle one mirror occupies.
#[derive(Clone, Copy, Debug, PartialEq)]
struct MirrorGeometry {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

/// The painted appearance of one glass tier.
#[derive(Clone, Debug)]
struct MirrorAppearance {
    fill: Color,
    edge: Color,
    radius: Val,
    edge_width: Val,
    shadows: Vec<ShadowStyle>,
}

/// The measured system's declared access set. One named bundle keeps the
/// system under Clippy's argument ceiling without hiding a whole `World`.
#[derive(SystemParam)]
pub(crate) struct BackdropMirrorAccess<'w, 's> {
    palette: Option<Res<'w, WindowPalette>>,
    target: Option<Res<'w, BackdropCaptureTarget>>,
    revision: Option<ResMut<'w, BackdropRevision>>,
    roots: Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<AppShellRoot>>,
    sources: Query<
        'w,
        's,
        (
            Entity,
            &'static BackdropMirror,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
    >,
    slots: Query<'w, 's, (Entity, &'static BackdropMirrorSlot)>,
    mirrors: Query<
        'w,
        's,
        (
            &'static mut Node,
            &'static mut BackgroundColor,
            &'static mut BorderColor,
            &'static mut BoxShadow,
        ),
    >,
}

/// Keep the offscreen lower composite in step with the marked surfaces.
///
/// The system runs after the UI layout, so every mirror is placed at the
/// surface's MEASURED physical rectangle rather than an authored one. A
/// mirror that appears, moves or disappears bumps the backdrop revision, so
/// the capture controller renders exactly one frame per real change instead
/// of keeping a stale composite alive.
pub(crate) fn sync_backdrop_mirrors(access: BackdropMirrorAccess<'_, '_>, mut commands: Commands) {
    let BackdropMirrorAccess {
        palette,
        target,
        revision,
        roots,
        sources,
        slots,
        mut mirrors,
    } = access;
    let Some(palette) = palette else {
        return;
    };
    let Some(target) = target else {
        return;
    };
    let Ok((root, root_transform)) = roots.single() else {
        return;
    };
    let mut changed = false;
    let mut live: Vec<Entity> = Vec::new();
    for (source, mirror, node, transform) in &sources {
        let Some(raster) = OpticalGlassRaster::from_layout(node, transform, root, root_transform)
        else {
            continue;
        };
        if !raster.is_valid() {
            continue;
        }
        live.push(source);
        let scale = raster.device_scale_factor;
        let geometry = MirrorGeometry {
            left: raster.origin_px.x / scale,
            top: raster.origin_px.y / scale,
            width: raster.size_px.x / scale,
            height: raster.size_px.y / scale,
        };
        let appearance = painted_tier(mirror.tier, &palette.inner);
        let existing = slots
            .iter()
            .find(|(_, slot)| slot.0 == Some(source))
            .map(|(entity, _)| entity);
        match existing {
            Some(entity) => {
                if apply_mirror(&mut mirrors, entity, geometry, &appearance) {
                    changed = true;
                }
            }
            None => {
                let scene = mirror_scene(target.camera, source, geometry, appearance);
                commands.spawn_scene(scene);
                changed = true;
            }
        }
    }
    for (entity, slot) in &slots {
        if !slot.0.is_some_and(|source| live.contains(&source)) {
            commands.entity(entity).despawn();
            changed = true;
        }
    }
    if changed && let Some(mut revision) = revision {
        revision.bump();
    }
}

/// Resolve one tier's painted appearance from the palette.
fn painted_tier(tier: GlassTier, palette: &UiPalette) -> MirrorAppearance {
    MirrorAppearance {
        fill: palette.glass_fill(tier),
        edge: palette.glass_edge(tier),
        radius: Val::Px(palette.panel_radius_px),
        edge_width: Val::Rem(tier.edge_width_rem()),
        shadows: palette.glass_shadow(tier),
    }
}

/// Apply the measured geometry and appearance to one live mirror.
fn apply_mirror(
    mirrors: &mut Query<(
        &mut Node,
        &mut BackgroundColor,
        &mut BorderColor,
        &mut BoxShadow,
    )>,
    entity: Entity,
    geometry: MirrorGeometry,
    appearance: &MirrorAppearance,
) -> bool {
    let Ok((mut node, mut fill, mut edge, mut shadows)) = mirrors.get_mut(entity) else {
        return false;
    };
    let mut changed = false;
    let next = (
        Val::Px(geometry.left),
        Val::Px(geometry.top),
        Val::Px(geometry.width),
        Val::Px(geometry.height),
    );
    if (node.left, node.top, node.width, node.height) != next {
        node.left = next.0;
        node.top = next.1;
        node.width = next.2;
        node.height = next.3;
        changed = true;
    }
    if fill.0 != appearance.fill {
        fill.0 = appearance.fill;
        changed = true;
    }
    if edge.left != appearance.edge
        || edge.top != appearance.edge
        || edge.right != appearance.edge
        || edge.bottom != appearance.edge
    {
        *edge = BorderColor::all(appearance.edge);
        changed = true;
    }
    let next_radius = BorderRadius::all(appearance.radius);
    if node.border_radius != next_radius {
        node.border_radius = next_radius;
        changed = true;
    }
    if shadows.0 != appearance.shadows {
        shadows.0 = appearance.shadows.clone();
        changed = true;
    }
    changed
}

/// One mirror node in the capture root: the measured rectangle, the tier's
/// painted fill, its edge, radius and layered shadow, and the capture
/// camera as its explicit UI target.
fn mirror_scene(
    camera: Entity,
    source: Entity,
    geometry: MirrorGeometry,
    appearance: MirrorAppearance,
) -> impl Scene {
    let left = Val::Px(geometry.left);
    let top = Val::Px(geometry.top);
    let width = Val::Px(geometry.width);
    let height = Val::Px(geometry.height);
    let edge_width = appearance.edge_width;
    let radius = appearance.radius;
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: {left},
            top: {top},
            width: {width},
            height: {height},
            border: UiRect::all(edge_width),
            border_radius: BorderRadius::all(radius),
        }
        BackgroundColor({ appearance.fill })
        BorderColor::all(appearance.edge)
        BoxShadow({ appearance.shadows })
        Pickable::IGNORE
        UiTargetCamera({ camera })
        BackdropMirrorSlot({ Some(source) })
    }
}

/// Register the mirror system after the UI layout, so every mirror is
/// placed at the surface's measured rectangle.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.add_systems(
        PostUpdate,
        sync_backdrop_mirrors.after(UiSystems::PostLayout),
    );
}

#[cfg(test)]
#[path = "../../../tests/headless/optical_backdrop_mirror.rs"]
mod tests;
