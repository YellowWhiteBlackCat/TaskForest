//! Shared GPUI inventory-table geometry.
//!
//! Pages consume the root's measured page slot; they do not read window pixels
//! again. The trailing column is elastic so long commands truncate inside
//! their own bounded cell instead of spilling past the right edge.

use taskmanager_theme::tokens;

/// The vertical scrollbar's fixed hit strip.
pub const TABLE_RAIL_WIDTH: f32 = 12.0;

/// Fit the trailing column to the measured content width while preserving all
/// leading columns. The right rail and trailing gutter are charged first.
#[must_use]
pub fn elastic_last_column(leading_widths: &[f32], content_width: f32) -> f32 {
    let leading = leading_widths.iter().sum::<f32>();
    let floor = tokens::SPACE_8.0;
    (content_width - leading - TABLE_RAIL_WIDTH - 12.0).max(floor)
}

#[cfg(test)]
#[path = "../../../tests/headless/list_table_geometry.rs"]
mod tests;
