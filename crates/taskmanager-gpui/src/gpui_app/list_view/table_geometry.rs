//! Shared GPUI inventory-table geometry.
//!
//! Pages consume the root's measured page slot; they do not read window pixels
//! again. The trailing column is elastic so long commands truncate inside
//! their own bounded cell instead of spilling past the right edge.

use taskmanager_theme::tokens;

use gpui::{Pixels, Point, Size, point, px};

/// The vertical scrollbar's fixed hit strip.
pub const TABLE_RAIL_WIDTH: f32 = 12.0;
pub(crate) const ROW_TOOLTIP_EDGE_MARGIN: f32 = 8.0;

const ROW_TOOLTIP_UPPER_REGION: f32 = 0.66;

/// Fit the trailing column to the measured content width while preserving all
/// leading columns. The right rail and trailing gutter are charged first.
#[must_use]
pub fn elastic_last_column(leading_widths: &[f32], content_width: f32) -> f32 {
    let leading = leading_widths.iter().sum::<f32>();
    let floor = tokens::SPACE_8.0;
    (content_width - leading - TABLE_RAIL_WIDTH - 12.0).max(floor)
}

/// Park row details at the viewport edge farthest from the pointer. Inventory
/// rows are densely stacked, so a cursor-adjacent plate can cover the next
/// row even when its anchor is shifted below the text.
#[must_use]
pub(crate) fn row_tooltip_anchor(cursor: Point<Pixels>, viewport: Size<Pixels>) -> Point<Pixels> {
    let edge = px(ROW_TOOLTIP_EDGE_MARGIN);
    let lower_edge = if f32::from(cursor.y) <= f32::from(viewport.height) * ROW_TOOLTIP_UPPER_REGION
    {
        viewport.height - edge
    } else {
        edge
    };
    point(cursor.x, lower_edge)
}

#[cfg(test)]
#[path = "../../../tests/headless/list_table_geometry.rs"]
mod tests;
