//! The product-owned virtual-table window: the rows that intersect the
//! viewport, the rows the body actually materializes for them, and the
//! geometry both bands are derived from.
//!
//! Iced reports a table's scroll offset through the scroll callback, so the
//! offset the renderer sees always lags the widget's real position by up to
//! one frame. A fast wheel flick therefore reaches rows the last materialized
//! band does not contain, and the body paints a blank block where the new
//! rows should be. The product band materializes [`OVERSCAN_ROWS`] extra rows
//! on each side of the shared kit's default band, so the lagged report always
//! lands inside rows that are already built.

use zhuge_ui_core::virtual_list::{DEFAULT_OVERSCAN, VirtualListConfig, VirtualWindow};

/// The product's own pre-render band: extra rows materialized on each side of
/// the visible window, on top of the shared geometry kit's default band.
pub(crate) const OVERSCAN_ROWS: usize = 2;

/// The total band materialized on each side of the visible window: the shared
/// kit's default band plus this product's pre-render rows.
pub(crate) const MATERIALIZED_BAND_ROWS: usize = DEFAULT_OVERSCAN + OVERSCAN_ROWS;

/// The scroll advance, in rows, a fast wheel flick may produce before the
/// scroll report reaches the renderer. The materialized band must absorb it
/// completely: a flick that outruns the band exposes an unbuilt row, which is
/// the blank block the pre-render band exists to remove.
pub(crate) const FLICK_TOLERANCE_ROWS: usize = 6;

/// One resolved table window: the rows that intersect the viewport and the
/// rows the body materializes for them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TableWindow {
    visible: VirtualWindow,
    materialized: VirtualWindow,
}

impl TableWindow {
    /// The rows that intersect the viewport at the reported scroll offset.
    ///
    /// Test seam for the pre-render proof: the render path materializes
    /// [`Self::materialized`], and the headless scroll-lag check resolves the
    /// visible band through this accessor.
    #[cfg_attr(not(test), allow(dead_code))]
    #[must_use]
    pub(crate) const fn visible(self) -> VirtualWindow {
        self.visible
    }

    /// The rows the body materializes: the visible rows plus the
    /// pre-render band on each side.
    #[must_use]
    pub(crate) const fn materialized(self) -> VirtualWindow {
        self.materialized
    }

    /// The materialized row range as a half-open interval.
    #[cfg_attr(not(test), allow(dead_code))]
    #[must_use]
    pub(crate) const fn materialized_range(self) -> std::ops::Range<usize> {
        self.materialized.start..self.materialized.end
    }
}

/// Resolve the visible and materialized windows for one fixed-extent list.
///
/// The visible band is the rows that intersect the viewport; the materialized
/// band is that band widened by [`MATERIALIZED_BAND_ROWS`] rows on each side
/// (clamped by the list bounds). Both bands come from the shared geometry kit
/// so the row extents, the prefix handling and the end clamping stay owned by
/// one implementation.
#[must_use]
pub(crate) fn resolve_table_window(
    total_rows: usize,
    scroll_offset: f32,
    viewport_height: f32,
    row_height: f32,
) -> TableWindow {
    let config = VirtualListConfig::new(total_rows, row_height, viewport_height)
        .with_scroll_offset(scroll_offset);
    let visible = config.compute_visible_viewport();
    // The band must absorb a fast wheel flick completely; the release build
    // keeps the same band, the debug build states the contract.
    let band = MATERIALIZED_BAND_ROWS;
    debug_assert!(
        band >= FLICK_TOLERANCE_ROWS,
        "the pre-render band must cover a fast wheel flick"
    );
    let materialized = config.with_overscan(band).compute_viewport();
    TableWindow {
        visible,
        materialized,
    }
}
