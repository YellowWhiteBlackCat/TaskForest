// test-intent: behavior
//! The virtual-table pre-render band: the rows the body materializes must
//! always contain the rows the user actually sees, even when the reported
//! scroll offset lags the widget by a fast wheel flick.

use super::window::{
    FLICK_TOLERANCE_ROWS, MATERIALIZED_BAND_ROWS, OVERSCAN_ROWS, resolve_table_window,
};
use zhuge_ui_core::virtual_list::{DEFAULT_OVERSCAN, VirtualListConfig};

/// The band the product promises: the shared geometry kit's default band plus
/// this product's own pre-render rows.
const PRODUCT_BAND_ROWS: usize = DEFAULT_OVERSCAN + OVERSCAN_ROWS;

/// The visible band of a list, resolved from the shared geometry kit.
fn visible_at(total_rows: usize, offset: f32, viewport: f32, row: f32) -> (usize, usize) {
    let window = VirtualListConfig::new(total_rows, row, viewport)
        .with_scroll_offset(offset)
        .compute_visible_viewport();
    (window.start, window.end)
}

/// The materialized band must contain every visible row, and must carry the
/// product's own pre-render rows on top of the shared kit's default band.
#[test]
fn the_materialized_band_contains_the_visible_rows_plus_the_pre_render_buffer() {
    let total_rows = 10_000_usize;
    let row_height = 32.0_f32;
    let viewport_height = 480.0_f32;

    for offset in [0.0_f32, 640.0, 12_345.0, 200_000.0] {
        let window = resolve_table_window(total_rows, offset, viewport_height, row_height);
        let visible = window.visible();
        let materialized = window.materialized();

        assert!(
            materialized.start <= visible.start && materialized.end >= visible.end,
            "the materialized band {materialized:?} must contain the visible band {visible:?}"
        );
        // The pre-render buffer exists on both edges whenever the list has
        // rows outside the viewport (the band is clamped by the list bounds).
        // The materialized band is the shared kit's default band PLUS this
        // product's own pre-render rows, so the buffer is wider than the
        // shared band alone.
        let buffered_before = visible.start.saturating_sub(materialized.start);
        let buffered_after = materialized.end.saturating_sub(visible.end);
        if visible.start > 0 {
            assert!(
                buffered_before >= PRODUCT_BAND_ROWS,
                "the band must keep {PRODUCT_BAND_ROWS} materialized rows before the first \
                 visible row (got {buffered_before} at offset {offset})"
            );
        }
        if visible.end < total_rows {
            assert!(
                buffered_after >= PRODUCT_BAND_ROWS,
                "the band must keep {PRODUCT_BAND_ROWS} materialized rows after the last \
                 visible row (got {buffered_after} at offset {offset})"
            );
        }
    }
}

/// The white-block proof: the scroll offset the renderer sees lags the
/// widget's real position by up to one fast flick. Every row the user sees
/// at the LAGGED position must already be materialized, otherwise the body
/// paints a blank block there.
#[test]
fn a_fast_wheel_flick_never_exposes_an_unmaterialized_row() {
    let total_rows = 10_000_usize;
    let row_height = 32.0_f32;
    let viewport_height = 480.0_f32;
    // The worst case the product band must absorb: a flick that advances the
    // scroll by the whole flick tolerance before the report arrives.
    let lag_rows = FLICK_TOLERANCE_ROWS as f32;

    for reported_offset in [0.0_f32, 1_024.0, 4_096.0, 120_000.0] {
        let window = resolve_table_window(total_rows, reported_offset, viewport_height, row_height);
        let lagged_offset = reported_offset + lag_rows * row_height;
        let (actual_start, actual_end) =
            visible_at(total_rows, lagged_offset, viewport_height, row_height);
        let materialized = window.materialized();
        assert!(
            materialized.start <= actual_start && materialized.end >= actual_end,
            "a flick of {lag_rows} rows at offset {reported_offset} exposed the unmaterialized \
             rows {actual_start}..{actual_end} (materialized {materialized:?})"
        );
    }
}

/// The band stays bounded: the pre-render rows never turn the materialized
/// range into an unbounded slice of a long list.
#[test]
fn the_pre_render_band_keeps_the_materialized_range_bounded() {
    let total_rows = 50_000_usize;
    let row_height = 24.0_f32;
    let viewport_height = 800.0_f32;
    let visible_rows = (viewport_height / row_height).ceil() as usize;
    let bound = visible_rows + 2 * MATERIALIZED_BAND_ROWS + 1;

    for offset in [0.0_f32, 9_999.0, 1.9e6, f32::MAX] {
        let window = resolve_table_window(total_rows, offset, viewport_height, row_height);
        assert!(
            window.materialized_range().len() <= bound,
            "offset {offset}: materialized {} exceeds the bound {bound}",
            window.materialized_range().len()
        );
    }
}
