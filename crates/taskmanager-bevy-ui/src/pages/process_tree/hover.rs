//! The hover group hint (BEVY-130).
//!
//! Hovering a parent row marks the rows it owns: the subtree below the
//! hovered row gets a faint outline, so the affected group is visible before
//! anything is selected. The outline lives on a dedicated overlay node, not
//! on the row itself — the focus ring owns the row's own outline colour, and
//! a row that is not hovered must not fight it.

use std::collections::HashSet;

use bevy::color::{Alpha, Color};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, Res};
use bevy::picking::Pickable;
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{Node, Outline, PositionType, Val, px};

use crate::window::WindowPalette;

use super::ProcessTreeRowMarker;
use super::ProcessTreeRows;
use super::mount::TreeRowEntity;

/// The outline width of the hover group hint, in logical pixels.
const HOVER_OUTLINE_PX: f32 = 1.0;

/// The alpha of the hover group outline: a hint, never a selection.
const HOVER_GROUP_ALPHA: f32 = 0.42;

/// The overlay node that carries the hover group's outline.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TreeHoverFrame;

/// The overlay scene of one row: an invisible frame that can be outlined.
pub(super) fn hover_frame_scene() -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            right: px(0.0),
            bottom: px(0.0),
        }
        Outline { width: Val::Px(HOVER_OUTLINE_PX), offset: Val::Px(0.0), color: Color::NONE }
        TreeHoverFrame
        Pickable::IGNORE
    })
}

/// Outline the subtree of the hovered row.
///
/// The rows of the tree are in projection order, so the subtree of a row is
/// the run of rows below it that sit deeper in the hierarchy.
pub(super) fn paint_hover_group(
    palette: Res<WindowPalette>,
    roots: Query<&Children, With<ProcessTreeRows>>,
    rows: Query<&TreeRowEntity>,
    hovered: Query<&PickingInteraction, With<ProcessTreeRowMarker>>,
    mut frames: Query<(&ChildOf, &mut Outline), With<TreeHoverFrame>>,
) {
    let ink = palette.inner.accent.with_alpha(HOVER_GROUP_ALPHA);
    let mut group: HashSet<Entity> = HashSet::new();
    for children in &roots {
        let mut owner: Option<Entity> = None;
        let mut depths: Vec<(Entity, usize)> = Vec::new();
        for child in children.iter() {
            let Ok(row) = rows.get(*child) else {
                continue;
            };
            depths.push((*child, row.depth));
            if owner.is_none()
                && hovered
                    .get(*child)
                    .is_ok_and(|interaction| *interaction == PickingInteraction::Hovered)
            {
                owner = Some(*child);
            }
        }
        let Some(owner) = owner else {
            continue;
        };
        let Some(index) = depths.iter().position(|(entity, _)| *entity == owner) else {
            continue;
        };
        // The subtree of a row is the run of deeper rows that follows it.
        let owner_depth = depths[index].1;
        for (entity, depth) in depths.iter().skip(index + 1) {
            if *depth <= owner_depth {
                break;
            }
            group.insert(*entity);
        }
    }
    for (parent, mut outline) in &mut frames {
        let wanted = if group.contains(&parent.parent()) {
            ink
        } else {
            Color::NONE
        };
        if outline.color != wanted {
            outline.color = wanted;
        }
    }
}
