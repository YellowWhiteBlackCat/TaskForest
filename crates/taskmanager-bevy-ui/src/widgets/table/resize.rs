//! Column-width drag resize for the table family.
//!
//! The reference interaction is the shared table family's `ColumnDragResize`
//! (see `taskmanager-ui/src/data/table/resize.rs`): a gutter at the header
//! cell's trailing edge, a pointer drag that changes the column width, and
//! the ANCHORED measurement — the first pointer sample of a session anchors
//! it and every later sample is measured from that fixed point, so a moving
//! layout bound can never introduce cumulative drift.
//!
//! This module is the Bevy materialization of that contract: the pointer
//! channels are Bevy's picking events, the width policy is the shared clamp,
//! and the live widths live in one resource the table renders from. The drag
//! updates the affected cells IN PLACE (the header cell and every body cell
//! of that column), so the gutter under the pointer is never rebuilt
//! mid-drag.

use std::collections::HashMap;

use bevy::ecs::component::Component;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::picking::cursor::EntityCursor;
use bevy::picking::events::{PointerDrag, PointerDragEnd, PointerPress, PointerRelease};
use bevy::scene::{Scene, bsn, on};
use bevy::ui::prelude::{Node, PositionType, px};
use bevy::window::SystemCursorIcon;
use taskmanager_ui_contract::{PROCESS_COLUMNS, ProcessColumnSpec};

use crate::palette::space_8;

use super::{LAYOUT_MIN_COLUMN_WIDTH_PX, clamp_column_width};

/// The table's live column widths: the user's per-column overrides over the
/// contract defaults. One authority — the header and every body row read the
/// same map, so a dragged width can never disagree between the two.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct ColumnWidthOverrides {
    pub(crate) widths: HashMap<String, f32>,
}

/// Marker on the resize gutter of one header cell.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ColumnResizeHandle(pub(crate) &'static str);

/// Marker on a cell node whose width belongs to one column: the header cell
/// and every body cell of that column. A drag writes the new width straight
/// into these nodes.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ColumnWidthNode(pub(crate) &'static str);

/// The live drag session: the column being resized, the width the session
/// started from, and the anchor sample every later measurement is taken
/// from. `None` when no drag is active.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct ColumnResizeSession {
    pub(crate) column: Option<&'static str>,
    pub(crate) start_width: f32,
    pub(crate) anchor_x: Option<f32>,
}

/// The anchored drag measurement (the shared reference semantics): the first
/// sample anchors the session, later samples are measured from that fixed
/// point. Pure, so the policy is testable without a world.
#[must_use]
pub(crate) fn width_from_drag(start_width: f32, anchor_x: &mut Option<f32>, cursor_x: f32) -> f32 {
    let anchor = *anchor_x.get_or_insert(cursor_x);
    start_width + (cursor_x - anchor)
}

/// The contract default width of one column, clamped to the supported range.
#[must_use]
pub(crate) fn default_column_width(column_id: &str) -> f32 {
    PROCESS_COLUMNS
        .iter()
        .find(|spec| spec.id == column_id)
        .map_or(LAYOUT_MIN_COLUMN_WIDTH_PX, |spec| {
            clamp_column_width(spec.default_width)
        })
}

/// The column's live width: the user's override when there is one, else the
/// contract default.
#[must_use]
pub(crate) fn live_column_width(column_id: &str, overrides: &ColumnWidthOverrides) -> f32 {
    overrides.widths.get(column_id).map_or_else(
        || default_column_width(column_id),
        |width| clamp_column_width(*width),
    )
}

/// The widths of the visible columns, honoring the user's overrides.
#[must_use]
pub(crate) fn live_column_widths(
    columns: &[&ProcessColumnSpec],
    overrides: &ColumnWidthOverrides,
) -> Vec<f32> {
    columns
        .iter()
        .map(|spec| live_column_width(spec.id, overrides))
        .collect()
}

/// The resize gutter scene: a transparent strip at the header cell's trailing
/// edge. The cursor hint is the system's column-resize shape, so the affordance
/// is visible before the drag starts.
pub(crate) fn resize_handle_scene(column_id: &'static str) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            right: px(0.0),
            top: px(0.0),
            bottom: px(0.0),
            width: px(space_8()),
        }
        ColumnResizeHandle({ column_id })
        EntityCursor::System({ SystemCursorIcon::ColResize })
        on(on_resize_handle_pressed)
        on(on_resize_handle_dragged)
        on(on_resize_handle_released)
        on(on_resize_handle_drag_ended)
    })
}

/// Observer: the gutter starts a drag session. The press never reaches the
/// header cell's own button, so a resize click never sorts the column.
pub(crate) fn on_resize_handle_pressed(
    mut press: On<PointerPress>,
    handles: Query<&ColumnResizeHandle>,
    overrides: Option<Res<ColumnWidthOverrides>>,
    mut session: ResMut<ColumnResizeSession>,
) {
    press.propagate(false);
    let Ok(handle) = handles.get(press.entity) else {
        return;
    };
    let column = handle.0;
    session.column = Some(column);
    session.start_width = overrides.as_deref().map_or_else(
        || default_column_width(column),
        |overrides| live_column_width(column, overrides),
    );
    session.anchor_x = None;
}

/// Observer: the drag ends when the pointer is released over the gutter. The
/// session closes here, so the next rebuild owns the header again (and the
/// committed width lives in the override map from now on).
///
/// The release is NOT stopped: the header cell's own button still has to see
/// it, so the cell never stays in its pressed state.
pub(crate) fn on_resize_handle_released(
    _release: On<PointerRelease>,
    mut session: ResMut<ColumnResizeSession>,
) {
    session.column = None;
    session.anchor_x = None;
}

/// Observer: the drag also ends when the pointer is released AWAY from the
/// gutter (the release then lands on another entity). The dragged entity
/// still receives the drag's end, so the session closes there too.
pub(crate) fn on_resize_handle_drag_ended(
    _drag_end: On<PointerDragEnd>,
    mut session: ResMut<ColumnResizeSession>,
) {
    session.column = None;
    session.anchor_x = None;
}

/// Observer: the drag measures from the session anchor and writes the new
/// width into the header cell and every body cell of that column.
pub(crate) fn on_resize_handle_dragged(
    mut drag: On<PointerDrag>,
    mut session: ResMut<ColumnResizeSession>,
    mut overrides: Option<ResMut<ColumnWidthOverrides>>,
    mut nodes: Query<(&ColumnWidthNode, &mut Node)>,
) {
    drag.propagate(false);
    let Some(column) = session.column else {
        return;
    };
    let Some(overrides) = overrides.as_deref_mut() else {
        return;
    };
    let mut anchor = session.anchor_x;
    let width = clamp_column_width(width_from_drag(
        session.start_width,
        &mut anchor,
        drag.pointer.position.x,
    ));
    session.anchor_x = anchor;
    overrides.widths.insert(column.to_string(), width);
    for (marker, mut node) in &mut nodes {
        if marker.0 == column {
            node.width = px(width);
        }
    }
    // The committed width lives in the override map, so the next data refresh
    // re-renders the table with the same width — the drag itself never
    // rebuilds the surface under the pointer.
}
