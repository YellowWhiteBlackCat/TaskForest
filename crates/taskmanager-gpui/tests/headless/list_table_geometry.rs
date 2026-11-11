//! Pure right-edge proof for the elastic Startup command column.

use super::{TABLE_RAIL_WIDTH, elastic_last_column};
use crate::gpui_app::list_view::row_tooltip_anchor;
use gpui::{point, px};
use taskmanager_theme::tokens;

#[test]
fn the_elastic_column_spends_the_measured_right_edge_budget() {
    let leading = [80.0, 280.0, 110.0, 140.0];
    let content_width = 688.0;
    let command = elastic_last_column(&leading, content_width);
    let total = leading.iter().sum::<f32>() + command + TABLE_RAIL_WIDTH + 12.0;
    assert!(
        (total - content_width).abs() < 0.01,
        "all columns plus the rail and gutter must fit the measured slot: {total} vs {content_width}"
    );
}

#[test]
fn an_extremely_narrow_slot_keeps_the_command_column_whole() {
    let leading = [80.0, 280.0, 110.0, 140.0];
    assert!(
        elastic_last_column(&leading, 320.0) >= tokens::SPACE_8.0,
        "the last column must remain a readable whole group"
    );
}

#[test]
fn inventory_row_tooltip_starts_below_the_hovered_text() {
    let anchor = point(px(120.0), px(220.0));
    let shifted = row_tooltip_anchor(anchor);
    assert_eq!(shifted.x, anchor.x);
    assert_eq!(shifted.y, anchor.y + px(tokens::SPACE_16.0));
}
