//! Pure placement geometry for anchored floating panels.
//!
//! Everything here is a pure rectangle-in/point-out function so placements
//! are provable headlessly — no renderer, no widget tree, no Iced runtime.
//! The policy mirrors the GPUI popup seam: prefer below the anchor, flip
//! above when the panel does not fit, and never let the panel leave the
//! window.

use iced::{Point, Rectangle, Size};

/// The interior margin kept between the panel and the window edges. A panel
/// that cannot fit inside the viewport pins to this margin instead of
/// over- or under-flowing.
const MARGIN: f32 = 4.0;

/// Place `panel` below `anchor` (gap-separated). When the panel does not fit
/// below and above the anchor is free, flip above; when neither side fits
/// (a panel taller than the window), pin to the top margin. The horizontal
/// position follows the anchor, clamped inside `viewport`. A panel wider
/// than the window pins to the left margin.
#[must_use]
pub(crate) fn below(anchor: Rectangle, panel: Size, viewport: Rectangle, gap: f32) -> Point {
    let below_y = anchor.y + anchor.height + gap;
    let above_y = anchor.y - gap - panel.height;
    let fits_below = below_y + panel.height <= viewport.y + viewport.height;
    let fits_above = above_y >= viewport.y;
    let unclamped_y = if fits_below || !fits_above {
        below_y
    } else {
        above_y
    };

    let min_x = viewport.x + MARGIN;
    let min_y = viewport.y + MARGIN;
    let x = if panel.width + MARGIN >= viewport.width {
        min_x
    } else {
        anchor
            .x
            .max(min_x)
            .min(viewport.x + viewport.width - MARGIN - panel.width)
    };
    let max_y = (viewport.y + viewport.height - MARGIN - panel.height).max(min_y);
    Point::new(x, unclamped_y.clamp(min_y, max_y))
}

#[cfg(test)]
#[path = "../../../../tests/gui/popover_anchor_tests.rs"]
mod tests;
