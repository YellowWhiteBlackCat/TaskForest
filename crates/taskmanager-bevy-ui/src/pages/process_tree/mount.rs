//! The progressive mount of the tree rows (BEVY-128).
//!
//! Expanding a large tree must not freeze the frame. The tree keeps the rows
//! it already mounted, retires the rows that left the projection, and mounts
//! the missing rows in batches — one budget per frame. A retired row stops
//! drawing at once and is despawned in the frame's last flush, so no observer
//! ever meets a child that vanished in the same frame.

use std::collections::HashMap;

use bevy::a11y::AccessibilityNode;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use bevy::prelude::Visibility;
use bevy::scene::CommandsSceneExt;
use bevy::ui::widget::Text;

use crate::app::ShellTrack;
use crate::input_contract::{SemanticAddress, stable_semantic_address};
use crate::scene_attach::{attach_child, insert_into};
use crate::tooltip::TooltipText;
use crate::window::WindowPalette;

use super::aggregate::{ProcessTreeCellKind, ProcessTreeTextCell, plan_scene};
use super::chevron::TreeChevron;
use super::plan::{TreeRowPlan, build_plans};
use super::{ProcessTreeRows, project_items};

/// The rows one frame may mount.
const MOUNT_BUDGET: usize = 64;

/// The mounted identity of one tree row.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TreeRowEntity {
    /// The row's stable identity.
    pub(crate) semantic_key: String,
    /// The row's nesting depth in the hierarchy.
    pub(crate) depth: usize,
    /// The row's structural fingerprint.
    pub(crate) shape: u64,
}

/// The rows the tree shows, and the work the next frame owes.
#[derive(Resource, Default)]
pub(crate) struct TreeRowsState {
    /// The desired rows, in render order.
    pub(crate) plans: Vec<TreeRowPlan>,
    /// The projection changed: the plans have to be recomputed.
    pub(crate) dirty: bool,
    /// The root the current plans belong to.
    pub(crate) root: Option<Entity>,
}

/// The mounted rows of the tree, as the mount system reads them.
#[derive(SystemParam)]
pub(super) struct TreeRowsQuery<'w, 's> {
    /// The roots that own the mounted rows.
    pub(super) roots: Query<'w, 's, (Entity, &'static Children), With<ProcessTreeRows>>,
    /// The mounted row identities.
    pub(super) mounted: Query<'w, 's, &'static TreeRowEntity>,
    /// The visibility of the mounted rows.
    pub(super) visibilities: Query<'w, 's, &'static Visibility>,
    /// The bounded cells of the mounted rows.
    pub(super) cells: Query<'w, 's, (Entity, &'static ProcessTreeTextCell, &'static Children)>,
    /// The text nodes of the mounted cells.
    pub(super) texts: Query<'w, 's, &'static mut Text>,
    /// The disclosure plates of the mounted rows.
    pub(super) chevrons: Query<'w, 's, (&'static mut TreeChevron, &'static Visibility)>,
    /// The tooltip and accessible summary of the mounted rows.
    pub(super) rows_meta: Query<
        'w,
        's,
        (
            Entity,
            &'static mut TooltipText,
            &'static mut AccessibilityNode,
        ),
    >,
}

/// Bring the mounted rows in line with the projection.
///
/// The system owns the whole rebuild: it recomputes the plans when the
/// projection changed, retires the rows that left it, refreshes the rows
/// that stay, and mounts the missing ones within the frame's budget.
pub(super) fn sync_tree_rows(
    track: ShellTrack,
    palette: Res<WindowPalette>,
    mut state: ResMut<TreeRowsState>,
    mut rows: TreeRowsQuery,
    mut commands: Commands,
) {
    let TreeRowsQuery {
        roots,
        mounted,
        visibilities,
        cells,
        texts,
        chevrons,
        rows_meta,
    } = &mut rows;
    for (root, children) in roots.iter() {
        let fresh_root = state.root != Some(root);
        if fresh_root {
            state.root = Some(root);
        }
        let refreshed = state.dirty || fresh_root;
        if refreshed {
            state.plans = compute_plans(&track);
            state.dirty = false;
        }
        let visible: Vec<(Entity, &TreeRowEntity)> = children
            .iter()
            .filter_map(|entity| {
                let entity = *entity;
                if is_hidden(entity, visibilities) {
                    return None;
                }
                mounted.get(entity).ok().map(|row| (entity, row))
            })
            .collect();
        let keep = common_prefix(&visible, &state.plans);
        for (entity, _) in visible.iter().skip(keep) {
            insert_into(&mut commands, *entity, Visibility::Hidden);
        }
        if refreshed {
            refresh_rows(&state.plans, &visible, keep, cells, texts, rows_meta);
            refresh_chevrons(&state.plans, chevrons);
        }
        let mut budget = MOUNT_BUDGET;
        for plan in state.plans.iter().skip(keep) {
            if budget == 0 {
                break;
            }
            let child = commands.spawn_scene(plan_scene(plan, &palette.inner)).id();
            attach_child(&mut commands, root, child);
            budget -= 1;
        }
    }
}

/// Despawn the rows a rebuild retired.
///
/// The rows were hidden in the frame that retired them, so the despawn never
/// meets a visible node that an observer still expects.
pub(super) fn flush_hidden_rows(
    rows: Query<(Entity, &Visibility), With<TreeRowEntity>>,
    mut commands: Commands,
) {
    for (entity, visibility) in &rows {
        if *visibility == Visibility::Hidden {
            commands.entity(entity).despawn();
        }
    }
}

/// The desired rows of the current projection.
fn compute_plans(track: &ShellTrack<'_>) -> Vec<TreeRowPlan> {
    let items: Vec<_> = track
        .shell()
        .visible_processes()
        .into_iter()
        .cloned()
        .collect();
    let observed_at_ms = track.shell().projection().processes_observed_at_ms;
    let rows = project_items(&items, track.process_tree_expansion(), observed_at_ms);
    build_plans(&rows)
}

/// The number of leading rows the mounted tree and the desired plans share.
///
/// A row whose identity and shape match keeps its entity; the first row that
/// differs ends the shared prefix, and everything from there on is remounted.
fn common_prefix(mounted: &[(Entity, &TreeRowEntity)], plans: &[TreeRowPlan]) -> usize {
    mounted
        .iter()
        .zip(plans.iter())
        .take_while(|((_, row), plan)| {
            row.semantic_key == plan.semantic_key && row.shape == plan.shape
        })
        .count()
}

/// Write the new readings of the kept rows into the mounted cells.
fn refresh_rows(
    plans: &[TreeRowPlan],
    visible: &[(Entity, &TreeRowEntity)],
    keep: usize,
    cells: &Query<(Entity, &ProcessTreeTextCell, &Children)>,
    texts: &mut Query<&mut Text>,
    rows_meta: &mut Query<(Entity, &mut TooltipText, &mut AccessibilityNode)>,
) {
    let mut index: HashMap<&str, Vec<(ProcessTreeCellKind, Entity)>> = HashMap::new();
    for (_, cell, children) in cells.iter() {
        let Some(text_entity) = children.first().copied() else {
            continue;
        };
        index
            .entry(cell.0.as_str())
            .or_default()
            .push((cell.1, text_entity));
    }
    for (row, (entity, _)) in visible.iter().take(keep).enumerate() {
        let plan = &plans[row];
        if let Some(row_cells) = index.get(plan.semantic_key.as_str()) {
            for (kind, text) in &plan.cells {
                let Some((_, text_entity)) =
                    row_cells.iter().find(|(cell_kind, _)| cell_kind == kind)
                else {
                    continue;
                };
                let Ok(mut current) = texts.get_mut(*text_entity) else {
                    continue;
                };
                if current.0 != *text {
                    current.0 = text.clone();
                }
            }
        }
        let Ok((_, mut tooltip, mut node)) = rows_meta.get_mut(*entity) else {
            continue;
        };
        if tooltip.0 != plan.summary {
            let semantic =
                SemanticAddress(stable_semantic_address("process-tree", &plan.semantic_key));
            tooltip.0 = plan.summary.clone();
            *node = AccessibilityNode(crate::semantic::process_row_node(
                &plan.summary,
                semantic.0.as_str(),
            ));
        }
    }
}

/// Write the new disclosure state into the mounted chevrons.
///
/// The chevron keeps its entity across the rebuild, so the turn that follows
/// an expansion starts from the angle the plate already reached.
fn refresh_chevrons(plans: &[TreeRowPlan], chevrons: &mut Query<(&mut TreeChevron, &Visibility)>) {
    let expanded: HashMap<&str, bool> = plans
        .iter()
        .map(|plan| (plan.semantic_key.as_str(), plan.expanded))
        .collect();
    for (mut chevron, visibility) in chevrons.iter_mut() {
        if *visibility == Visibility::Hidden {
            continue;
        }
        if let Some(&state) = expanded.get(chevron.semantic_key.as_str())
            && chevron.expanded != state
        {
            chevron.expanded = state;
        }
    }
}

/// Whether a row was retired by a rebuild.
fn is_hidden(entity: Entity, visibilities: &Query<&Visibility>) -> bool {
    visibilities
        .get(entity)
        .is_ok_and(|visibility| *visibility == Visibility::Hidden)
}
