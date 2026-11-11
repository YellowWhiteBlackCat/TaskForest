//! Pure right-edge proof for the elastic Startup command column.

use super::{ROW_TOOLTIP_EDGE_MARGIN, TABLE_RAIL_WIDTH, elastic_last_column, row_tooltip_anchor};
use gpui::{point, px, size};
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
fn inventory_row_tooltip_uses_the_far_vertical_viewport_edge() {
    let viewport = size(px(720.0), px(480.0));
    let upper_cursor = point(px(120.0), px(220.0));
    let lower_cursor = point(px(640.0), px(400.0));

    let lower_edge = row_tooltip_anchor(upper_cursor, viewport);
    let upper_edge = row_tooltip_anchor(lower_cursor, viewport);

    assert_eq!(lower_edge.x, upper_cursor.x);
    assert_eq!(lower_edge.y, viewport.height - px(ROW_TOOLTIP_EDGE_MARGIN));
    assert_eq!(upper_edge.x, lower_cursor.x);
    assert_eq!(upper_edge.y, px(ROW_TOOLTIP_EDGE_MARGIN));
}
