//! The collateral process tree of an affected root (BEVY-131).
//!
//! Ending a process tree affects the root and every process below it. This
//! module owns the dense, indented rendering of that closure: the frozen
//! descendant set comes from the shared core authority, and the rows are
//! laid out compactly so a whole subtree fits next to the action it belongs
//! to.
//!
//! The hierarchy is NOT re-derived here. The shared core owns the ONE
//! `parent_pid` traversal, and it publishes the closure twice: as the
//! execution set and as the frozen collateral view (the root plus every
//! affected descendant with its depth). This frontend reads the view, so
//! the drawn hierarchy can never disagree with what an action reaches.
//!
//! The wiring into the danger confirmation surface belongs to the surface's
//! owner: the dialog is shared with the process table, so this module stays
//! the reusable component and the owner mounts it.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{AlignItems, FlexDirection, Node, UiRect, Val, percent, px, rem};
use bevy::ui::widget::Text;

use taskforest_core::core::process::{
    FrozenProcessCollateralMember, ProcessBatchAction, ProcessBatchIntent, ProcessItem,
    ProcessLiveKey,
};

use crate::palette::{UiPalette, space_8};
use crate::window::{Role, TextRole};

/// The height of a collateral row, relative to the standard control height.
///
/// The collateral tree is a compact list: the rows are denser than the
/// process table's rows so a whole subtree stays readable in one glance.
const COLLATERAL_ROW_SCALE: f32 = 0.8;

/// One row of the collateral tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CollateralRow {
    /// The frozen identity of the process, when it carries one.
    pub(crate) identity: Option<ProcessLiveKey>,
    /// The process id.
    pub(crate) pid: u32,
    /// The process name.
    pub(crate) name: String,
    /// The nesting depth below the affected root.
    pub(crate) depth: usize,
}

/// Marker naming one mounted collateral row.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TreeCollateralRow {
    /// The nesting depth below the affected root.
    pub(crate) depth: usize,
    /// The process id of the affected process.
    pub(crate) pid: u32,
}

/// Marker naming the collateral tree itself.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TreeCollateralTree;

/// The frozen collateral rows of one affected root.
///
/// The frozen closure is the shared core authority: it names every process
/// below the root that the current snapshot knows about, so the dialog never
/// guesses which processes an action would reach.
pub(crate) fn collateral_rows(
    processes: &[ProcessItem],
    root: ProcessLiveKey,
) -> Vec<CollateralRow> {
    // The core owns the ONE traversal: freezing the tree publishes the
    // execution set and the hierarchical view from the same walk, so the
    // preview can never disagree with what an action would reach.
    let intent =
        ProcessBatchIntent::freeze_tree(processes, root, ProcessBatchAction::EndProcessTree);
    frozen_collateral_rows(intent.collateral_rows())
}

/// The collateral rows of a FROZEN collateral set.
///
/// The rows come from the intent that was frozen at arming time: a later
/// refresh of the live table cannot change them, which is exactly what a
/// confirmation surface must show.
pub(crate) fn frozen_collateral_rows(
    members: &[FrozenProcessCollateralMember],
) -> Vec<CollateralRow> {
    members
        .iter()
        .map(|member| CollateralRow {
            identity: member.identity().live_key(),
            pid: member.pid(),
            name: member.name().to_owned(),
            depth: member.depth() as usize,
        })
        .collect()
}

/// The dense, indented collateral tree of one affected root.
pub(crate) fn collateral_scene(rows: &[CollateralRow], palette: &UiPalette) -> Box<dyn Scene> {
    let children: Vec<Box<dyn Scene>> = rows
        .iter()
        .map(|row| collateral_row_scene(row, palette))
        .collect();
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(0.0),
        }
        TreeCollateralTree
        Pickable::IGNORE
        Children [{ children }]
    })
}

/// One dense row of the collateral tree.
fn collateral_row_scene(row: &CollateralRow, palette: &UiPalette) -> Box<dyn Scene> {
    let indent = space_8() * (row.depth as f32 + 1.0);
    let label = format!("{} · {}", row.name, row.pid);
    let depth = row.depth;
    let pid = row.pid;
    Box::new(bsn! {
        Node {
            height: rem(palette.control_height_rem * COLLATERAL_ROW_SCALE),
            padding: UiRect::left(Val::Px(indent)),
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            overflow: bevy::ui::Overflow::clip_x(),
        }
        TreeCollateralRow {
            depth: { depth },
            pid: { pid },
        }
        Pickable::IGNORE
        Children [
            Text({ label }) TextRole(Role::Caption) Pickable::IGNORE
        ]
    })
}
