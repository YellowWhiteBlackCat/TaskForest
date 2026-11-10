//! Zero-based nesting depth for the live optical surfaces.
//!
//! An optical surface's nesting depth is the number of optical surfaces that
//! are painted UNDER it and share its measured rectangle. The depth is a
//! geometric fact of the composed UI, not an authored flag: a tooltip that
//! floats over an optical card is nested, and the same tooltip over bare page
//! canvas is not.
//!
//! The shared theme owns the fail-closed decision. It consumes the depth
//! through `OpticalGlassContext::with_nesting_depth` and refuses any layer
//! beyond the capability ceiling, so this module's only duty is to report the
//! depth honestly: one owner, one derived fact, no second ladder of booleans
//! that a page could disagree with.

use bevy::ecs::entity::{Entity, EntityHashMap};
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, ResMut};
use bevy::math::Rect;
use bevy::ui::{ComputedNode, ComputedStackIndex, UiGlobalTransform};

use super::{OpticalGlassCard, OpticalGlassRaster};
use crate::window::AppShellRoot;

/// The resolved nesting depth of every live optical surface.
///
/// The depths are published as a resource because the layout binding reads
/// them in the SAME frame it resolves the backdrop: a component written
/// through `Commands` would land one frame late and make an overlay sample
/// the painted composite for one frame after it appears.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct OpticalNestingDepths {
    depths: EntityHashMap<u8>,
}

impl OpticalNestingDepths {
    /// The resolved depth of one surface. A surface without a published depth
    /// is a top-level surface.
    pub(crate) fn depth_of(&self, entity: Entity) -> u8 {
        self.depths.get(&entity).copied().unwrap_or(0)
    }

    /// Whether any live optical surface sits above another optical surface.
    ///
    /// Only such a surface needs the second capture: a top-level surface
    /// samples the painted lower composite, which is all the product shows
    /// below it.
    pub(crate) fn has_nested_surface(&self) -> bool {
        self.depths.values().any(|depth| *depth > 0)
    }
}

/// One optical surface's measured place in the UI stack.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OpticalSurfacePlacement {
    /// The surface's entity.
    pub(crate) entity: Entity,
    /// The surface's back-to-front position in the UI stack.
    pub(crate) stack_index: u32,
    /// The surface's measured rectangle, in physical pixels.
    pub(crate) rect: Rect,
}

/// Resolve every surface's nesting depth from the stack order and the
/// rectangles they share.
///
/// The stack index is the UI's own back-to-front order, so a higher index is
/// painted over a lower one. A surface is nested under exactly those lower
/// optical surfaces whose measured rectangle shares area with it: an
/// overlay that floats beside a card is not stacked over it.
#[must_use]
pub(crate) fn optical_nesting_depths(placements: &[OpticalSurfacePlacement]) -> Vec<(Entity, u8)> {
    let mut ordered: Vec<&OpticalSurfacePlacement> = placements.iter().collect();
    ordered.sort_by_key(|placement| placement.stack_index);
    let mut depths = Vec::with_capacity(ordered.len());
    for (index, placement) in ordered.iter().enumerate() {
        let covered = ordered[..index]
            .iter()
            .filter(|lower| rects_overlap(lower.rect, placement.rect))
            .count();
        depths.push((placement.entity, covered.min(u8::MAX as usize) as u8));
    }
    depths
}

/// Whether two measured rectangles share any area.
fn rects_overlap(left: Rect, right: Rect) -> bool {
    left.min.x < right.max.x
        && right.min.x < left.max.x
        && left.min.y < right.max.y
        && right.min.y < left.max.y
}

/// Resolve and publish the nesting depth of every live optical surface.
///
/// The system runs after the UI stack is resolved, so the depth is measured
/// against the same frame the backdrop capture will render.
pub(crate) fn sync_optical_nesting_depths(
    roots: Query<(&ComputedNode, &UiGlobalTransform), With<AppShellRoot>>,
    cards: Query<(
        Entity,
        &OpticalGlassCard,
        &ComputedNode,
        &UiGlobalTransform,
        &ComputedStackIndex,
    )>,
    mut depths: ResMut<OpticalNestingDepths>,
) {
    let Ok((root, root_transform)) = roots.single() else {
        return;
    };
    let mut placements = Vec::new();
    for (entity, card, node, transform, stack) in &cards {
        if !card.enabled {
            continue;
        }
        let Some(raster) = OpticalGlassRaster::from_layout(node, transform, root, root_transform)
        else {
            continue;
        };
        if !raster.is_valid() {
            continue;
        }
        placements.push(OpticalSurfacePlacement {
            entity,
            stack_index: stack.0,
            rect: Rect::from_corners(raster.origin_px, raster.origin_px + raster.size_px),
        });
    }
    let resolved = optical_nesting_depths(&placements);
    let mut published = EntityHashMap::with_capacity(resolved.len());
    for (entity, depth) in resolved {
        published.insert(entity, depth);
    }
    if depths.depths != published {
        depths.depths = published;
    }
}

/// Publish the nesting-depth resource.
///
/// The resolver itself runs inside the optical system's chain, because the
/// layout binding has to read the depth in the same frame it resolves the
/// backdrop it will sample.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<OpticalNestingDepths>();
}

#[cfg(test)]
#[path = "../../../tests/headless/optical_nesting.rs"]
mod tests;
