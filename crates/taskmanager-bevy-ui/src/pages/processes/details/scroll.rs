//! The bounded details scroll viewport.
//!
//! The official wheel and scrollbar keep continuous input, while the lower
//! viewport edge is advanced in measured row steps. This keeps the shared
//! process-detail projection and the atomic card lane untouched.

use bevy::ecs::query::With;
use bevy::ecs::system::Query;
use bevy::ui::{ComputedNode, ScrollPosition, UiGlobalTransform};

use crate::palette::{UiPalette, space_4, space_8};

use super::{OVERVIEW_FIELDS, ProcessDetailsRoot, ProcessDetailsRow};

const DETAILS_BOTTOM_SAFETY_CONTROLS: f32 = 2.0;
const DETAILS_SCROLL_EDGE_TOLERANCE_LOGICAL_PX: f32 = 0.5;

/// Leave two whole controls below the final detail row.
#[must_use]
pub(crate) fn details_bottom_inset_px(palette: &UiPalette) -> f32 {
    space_8() + palette.control_height_px() * DETAILS_BOTTOM_SAFETY_CONTROLS
}

#[must_use]
pub(crate) fn whole_row_scroll_offset(
    requested_scroll_y: f32,
    applied_scroll_y: f32,
    max_scroll_y: f32,
    viewport_bottom: f32,
    edge_clearance: f32,
    row_edges: &[(f32, f32)],
) -> f32 {
    if !requested_scroll_y.is_finite()
        || !applied_scroll_y.is_finite()
        || !max_scroll_y.is_finite()
        || !viewport_bottom.is_finite()
        || max_scroll_y <= 0.0
    {
        return 0.0;
    }
    let requested_scroll_y = requested_scroll_y.clamp(0.0, max_scroll_y);
    let layout_delta = requested_scroll_y - applied_scroll_y;
    let mut partial_row = None;
    for &(row_top, row_bottom) in row_edges {
        if !row_top.is_finite() || !row_bottom.is_finite() || row_bottom <= row_top {
            continue;
        }
        let predicted_top = row_top - layout_delta;
        let predicted_bottom = row_bottom - layout_delta;
        if predicted_top < viewport_bottom - DETAILS_SCROLL_EDGE_TOLERANCE_LOGICAL_PX
            && predicted_bottom > viewport_bottom + DETAILS_SCROLL_EDGE_TOLERANCE_LOGICAL_PX
        {
            partial_row = Some((predicted_top, predicted_bottom));
            break;
        }
    }
    let Some((row_top, row_bottom)) = partial_row else {
        return 0.0;
    };
    let include_row = row_bottom - viewport_bottom + edge_clearance;
    let exclude_row = row_top - viewport_bottom - edge_clearance;
    let preferred = if layout_delta > DETAILS_SCROLL_EDGE_TOLERANCE_LOGICAL_PX {
        [include_row, exclude_row]
    } else if layout_delta < -DETAILS_SCROLL_EDGE_TOLERANCE_LOGICAL_PX {
        [exclude_row, include_row]
    } else if include_row.abs() <= exclude_row.abs() {
        [include_row, exclude_row]
    } else {
        [exclude_row, include_row]
    };
    preferred
        .into_iter()
        .map(|correction| {
            (requested_scroll_y + correction).clamp(0.0, max_scroll_y) - requested_scroll_y
        })
        .find(|correction| {
            row_edges.iter().all(|&(top, bottom)| {
                let adjusted_top = top - (layout_delta + correction);
                let adjusted_bottom = bottom - (layout_delta + correction);
                adjusted_top >= viewport_bottom - DETAILS_SCROLL_EDGE_TOLERANCE_LOGICAL_PX
                    || adjusted_bottom <= viewport_bottom + DETAILS_SCROLL_EDGE_TOLERANCE_LOGICAL_PX
                    || adjusted_bottom <= adjusted_top
            })
        })
        .unwrap_or(0.0)
}

pub(crate) fn sync_details_whole_row_scroll(
    mut roots: Query<
        (&ComputedNode, &UiGlobalTransform, &mut ScrollPosition),
        With<ProcessDetailsRoot>,
    >,
    rows: Query<(&ComputedNode, &UiGlobalTransform), With<ProcessDetailsRow>>,
) {
    for (viewport, transform, mut scroll) in &mut roots {
        let inverse_scale_factor = viewport.inverse_scale_factor();
        let visible = viewport.size() * inverse_scale_factor;
        let content = viewport.content_size() * inverse_scale_factor;
        let max_scroll_y = (content.y - visible.y).max(0.0);
        let requested_scroll_y = scroll.0.y.clamp(0.0, max_scroll_y);
        let applied_scroll_y = viewport.scroll_position.y * inverse_scale_factor;
        let viewport_center = transform.translation.y * inverse_scale_factor;
        let viewport_bottom = viewport_center + visible.y * 0.5;
        let mut row_edges = [(0.0, 0.0); OVERVIEW_FIELDS.len()];
        let mut row_count = 0;
        for (node, row_transform) in &rows {
            if row_count == row_edges.len() {
                break;
            }
            if node.size().y <= 0.0 {
                continue;
            }
            let row_inverse_scale_factor = node.inverse_scale_factor();
            let half_height = node.size().y * row_inverse_scale_factor * 0.5;
            row_edges[row_count] = (
                row_transform.translation.y * row_inverse_scale_factor - half_height,
                row_transform.translation.y * row_inverse_scale_factor + half_height,
            );
            row_count += 1;
        }
        let correction = whole_row_scroll_offset(
            requested_scroll_y,
            applied_scroll_y,
            max_scroll_y,
            viewport_bottom,
            space_4(),
            &row_edges[..row_count],
        );
        if correction.abs() > DETAILS_SCROLL_EDGE_TOLERANCE_LOGICAL_PX {
            scroll.0.y = (requested_scroll_y + correction).clamp(0.0, max_scroll_y);
        }
    }
}
