//! The right-anchored capacity window: one sample window → one pixel-slot grid.
//!
//! Every chart in the product maps a **window of samples** onto **pixels**, and
//! every chart must draw its samples and resolve its hover through the *same*
//! mapping — otherwise the crosshair can point beside the value it reports.
//! GPUI (`sample_x` / `sample_index_at_cursor_x`), Iced (`WindowSlots`) and
//! Bevy (`ChartSlots`) each carried their own copy of that mapping; this
//! module is the single owner (ADR-020: a tool with more than one caller
//! lives on the shared floor).
//!
//! The contract:
//!
//! * The newest sample pins to the **right edge**; every sample owns one fixed
//!   slot of the window's width. A window that holds fewer samples than the
//!   capacity therefore grows a short, evenly-paced curve from the right edge
//!   leftward instead of stretching a handful of samples across the whole
//!   frame — the curve never claims a history the product does not have.
//! * The drawing x and the hover resolution derive from the **same
//!   denominator**, so a hover readout can never disagree with a drawn sample.
//! * Inside the frame the cursor clamps into the grid: the empty leading area
//!   of a growing window resolves to the oldest occupied slot, and a cursor
//!   at or past the right edge is the newest slot.
//! * Outside the frame there is **no slot**: a cursor before the plot (or a
//!   frame with no width) resolves to nothing rather than to a fabricated
//!   sample.
//! * A window with fewer than two samples has no line to draw, but its single
//!   sample still owns the right edge and stays hoverable.

/// The slot grid one curve's samples occupy inside the measured frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartSlots {
    sample_count: usize,
    capacity: usize,
    width: f32,
}

impl ChartSlots {
    /// Build the slot grid for a window of `sample_count` samples inside a
    /// `capacity`-wide frame (`width` pixels). A capacity below the sample
    /// count is safe: the denominator is their maximum, so an over-full window
    /// degrades to the full-span spread rather than dividing by zero.
    #[must_use]
    pub fn new(sample_count: usize, capacity: usize, width: f32) -> Self {
        Self {
            sample_count,
            capacity,
            width,
        }
    }

    /// The full-window form: every sample spreads evenly across the whole
    /// width (the legacy spread — no right-anchored growth).
    #[must_use]
    pub fn spread(sample_count: usize, width: f32) -> Self {
        Self::new(sample_count, sample_count, width)
    }

    /// The shared denominator: the sample span, or the capacity span, or one.
    ///
    /// Both the drawing x and the hover resolution divide by it, so the two
    /// directions can never disagree about where a sample sits.
    #[must_use]
    pub fn denominator(&self) -> f32 {
        self.sample_count
            .saturating_sub(1)
            .max(self.capacity.saturating_sub(1))
            .max(1) as f32
    }

    /// The x-coordinate of the sample at `index` (oldest→newest slots, newest
    /// pinned at `width`). Indices beyond the window clamp to the newest slot;
    /// an empty window has no x at all.
    ///
    /// The evaluation order is the one the **release surface** has always
    /// painted (`width * (1.0 - from_right / denominator)`): the newest
    /// sample's slot is measured from the right and the ratio is taken from
    /// the denominator. The mathematically equivalent form
    /// (`width * (denominator - from_right) / denominator`) rounds once
    /// instead of twice and lands within **one f32 ULP** of it (≤ 0.000122 px
    /// at 1920 px wide), but the release surface's numbers are the ones the
    /// product ships and its capture receipts pin, so the owner keeps them
    /// instead of moving a shipped pixel for a rounding nicety.
    #[must_use]
    pub fn x(&self, index: usize) -> f32 {
        if self.sample_count == 0 {
            return 0.0;
        }
        let width = if self.width.is_finite() {
            self.width.max(0.0)
        } else {
            0.0
        };
        let from_right = (self.sample_count - 1 - index.min(self.sample_count - 1)) as f32;
        width * (1.0 - from_right / self.denominator())
    }

    /// The inverse of [`ChartSlots::x`]: map a frame-relative cursor x onto
    /// the nearest sample slot.
    ///
    /// `None` before the frame, for a non-positive or non-finite width, for a
    /// non-finite cursor, or when the window holds no sample at all. Inside the
    /// frame the cursor clamps into the grid, so the empty leading area of a
    /// growing window resolves to the oldest occupied slot and a cursor at or
    /// past the right edge is the newest slot.
    #[must_use]
    pub fn index_at(&self, cursor_x: f32) -> Option<usize> {
        if self.sample_count == 0
            || !cursor_x.is_finite()
            || !self.width.is_finite()
            || self.width <= 0.0
            || cursor_x < 0.0
        {
            return None;
        }
        if self.sample_count == 1 {
            return Some(0);
        }
        let relative = (cursor_x / self.width).clamp(0.0, 1.0);
        let index = ((self.sample_count - 1) as f32 - (1.0 - relative) * self.denominator())
            .round()
            .clamp(0.0, (self.sample_count - 1) as f32);
        Some(index as usize)
    }
}

#[cfg(test)]
#[path = "../../tests/headless/presentation/chart_slots_tests.rs"]
mod tests;
