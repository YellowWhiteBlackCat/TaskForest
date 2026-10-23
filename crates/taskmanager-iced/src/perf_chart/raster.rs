//! Raster-placement primitives for the Iced Performance charts: the
//! device-grid (sub-pixel) alignment of a drawn polyline, the hover pill's
//! edge-collision flip, and the y-axis scale-text tiering.
//!
//! Every primitive here is pure and takes the render scale as a typed
//! input, so the alignment math is proven headlessly at fractional scale
//! factors (the app's own zoom, 125 %, 150 %) instead of being asserted by
//! eyeballing a capture. The device-pixel center offset is the classic
//! crisp-line alignment: a stroke's center line lands on a device-pixel
//! center, so a polyline's antialiasing stays uniform instead of
//! occasionally straddling two device-pixel rows (the "偶发虚化" the
//! Iced canvas showed on HiDPI and fractional-scale windows).
//!
//! The scale factor is the RENDER scale the caller can resolve (the app's
//! render zoom; the compositor's DPI factor multiplies it at the toolkit
//! boundary), and every caller supplies it, so the chart never guesses a
//! device pixel grid.

use iced::widget::canvas;
use iced::{Color, Pixels, Point, Rectangle, Size};

/// The half-device-pixel offset that places a stroke's center line on a
/// device-pixel center: a one-device-pixel-wide stroke then covers exactly
/// one device-pixel row instead of straddling two.
const DEVICE_PIXEL_CENTER_OFFSET: f32 = 0.5;

/// The fraction of the frame width that counts as the right-edge collision
/// zone: inside it the hover pill flips to the cursor's left so a
/// right-most sample can never clip the readout.
pub(crate) const PILL_FLIP_ZONE: f32 = 0.10;

/// The gap between the crosshair and a flipped hover pill.
const PILL_ANCHOR_GAP: f32 = 6.0;

/// The frame height at (and above) which the full y-axis scale ladder
/// (maximum, midpoint, baseline) stays legible: the shared secondary-chart
/// height. A frame shorter than that is a compact gauge — it keeps the MAIN
/// ticks (the maximum and the baseline) and drops the dense midpoint
/// subdivision, so its numbers never stack.
pub(crate) const SCALE_TEXT_FULL_MIN_HEIGHT: f32 = 128.0;

/// The frame height at (and above) which the scale text may be drawn at
/// all; below it the axis keeps the curve and the pointer only.
pub(crate) const SCALE_TEXT_MIN_HEIGHT: f32 = 96.0;

/// Snap one logical coordinate onto the device-pixel grid: the coordinate
/// moves to the nearest device-pixel center, so the stroke that runs
/// through it keeps a constant sub-pixel phase at every scale factor.
///
/// The deviation is at most half a device pixel, and the result is
/// idempotent. A non-finite coordinate is returned untouched (the honest
/// "nothing to align" state), and a non-positive or non-finite scale
/// factor falls back to the 1:1 device grid.
#[must_use]
pub(crate) fn snap_to_device_grid(value: f32, scale_factor: f32) -> f32 {
    if !value.is_finite() {
        return value;
    }
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    let device = value * scale;
    let snapped = (device - DEVICE_PIXEL_CENTER_OFFSET).round() + DEVICE_PIXEL_CENTER_OFFSET;
    snapped / scale
}

/// Which side of the crosshair the hover pill is drawn on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PillSide {
    /// The pill is centered on the crosshair — the default away from the
    /// frame's edges.
    Centered,
    /// The pill is drawn entirely to the LEFT of the crosshair: the
    /// right-edge collision flip.
    Left,
}

/// Resolve the hover pill's side for a crosshair at `anchor_x` inside a
/// frame of `frame_width`. Inside the right-most [`PILL_FLIP_ZONE`] the
/// pill flips to the cursor's left, so the readout is never clipped by the
/// frame edge.
#[must_use]
pub(crate) fn resolve_pill_side(anchor_x: f32, frame_width: f32) -> PillSide {
    if !anchor_x.is_finite() || !frame_width.is_finite() || frame_width <= 0.0 {
        return PillSide::Centered;
    }
    if anchor_x >= frame_width * (1.0 - PILL_FLIP_ZONE) {
        PillSide::Left
    } else {
        PillSide::Centered
    }
}

/// The pill's left edge for the resolved side, clamped so the pill always
/// stays inside the frame.
#[must_use]
pub(crate) fn pill_x_for_side(
    side: PillSide,
    anchor_x: f32,
    pill_width: f32,
    frame_width: f32,
) -> f32 {
    let max_x = (frame_width - pill_width).max(0.0);
    let x = match side {
        PillSide::Centered => anchor_x - pill_width / 2.0,
        PillSide::Left => anchor_x - pill_width - PILL_ANCHOR_GAP,
    };
    x.clamp(0.0, max_x)
}

/// Place a measured value pill beside a data point. It prefers the upper
/// side of the point, flips below near the top edge, and clamps both axes
/// so long labels stay inside short and narrow canvases. Horizontally the
/// pill follows the crosshair's side: inside the right-edge collision zone
/// it flips to the cursor's left instead of hanging off the frame.
#[must_use]
pub(crate) fn data_point_badge_bounds(size: Size, anchor: Point, text_width: f32) -> Rectangle {
    let width = (text_width.max(0.0) + 12.0).min(size.width.max(0.0));
    let height = 18.0_f32.min(size.height.max(0.0));
    let max_y = (size.height - height).max(0.0);
    let x = pill_x_for_side(
        resolve_pill_side(anchor.x, size.width),
        anchor.x,
        width,
        size.width,
    );
    let above = anchor.y - height - 6.0;
    let below = anchor.y + 6.0;
    let y = if above >= 0.0 {
        above.min(max_y)
    } else if below <= max_y {
        below
    } else {
        (anchor.y - height / 2.0).clamp(0.0, max_y)
    };
    Rectangle {
        x,
        y,
        width,
        height,
    }
}

/// The y-axis tick ladder: `[max, max / 2, 0]` — the three labels that make a
/// magnitude graph readable without crowding a mini-chart. Empty for a
/// non-positive or non-finite `max` (the honest idle window carries no scale
/// to read). Pure so the ladder is unit-tested headlessly.
#[must_use]
pub(crate) fn y_axis_tick_values(max: f32) -> Vec<f32> {
    if !max.is_finite() || max <= 0.0 {
        return Vec::new();
    }
    vec![max, max / 2.0, 0.0]
}

/// The scale ladder a frame of `height` may carry: the full ladder on a
/// tall frame, the MAIN ticks only (the maximum and the baseline) on a
/// compact one — the dense midpoint subdivision is dropped so the numbers
/// never stack — and nothing at all below the readability floor.
///
/// Pure so the tiering is unit-tested headlessly.
#[must_use]
pub(crate) fn y_axis_scale_ticks(ticks: &[f32], height: f32) -> Vec<f32> {
    if !height.is_finite() || height < SCALE_TEXT_MIN_HEIGHT {
        return Vec::new();
    }
    if height >= SCALE_TEXT_FULL_MIN_HEIGHT {
        return ticks.to_vec();
    }
    match (ticks.first(), ticks.last()) {
        (Some(first), Some(last)) if first == last => vec![*first],
        (Some(first), Some(last)) => vec![*first, *last],
        _ => Vec::new(),
    }
}

/// Draw the y-axis tick labels for one ladder (see [`y_axis_tick_values`]):
/// each value's text (formatted by the caller's injected unit formatter — the
/// MB/s spelling belongs to the call site, never the chart) sits at the
/// height its value maps to under [`super::scaled_y`], in a token-derived
/// muted color. Labels are clamped inside the frame so the 0 and max ticks
/// never half-clip past the edges, and the ladder is tiered by the frame
/// height ([`y_axis_scale_ticks`]): a compact gauge keeps only its main
/// ticks.
pub(crate) fn draw_y_axis_ticks(
    frame: &mut canvas::Frame<iced::Renderer>,
    size: Size,
    ticks: &[f32],
    max: f32,
    format_value: impl Fn(f32) -> String,
    color: Color,
) {
    if !max.is_finite() || max <= 0.0 {
        return;
    }
    for &value in &y_axis_scale_ticks(ticks, size.height) {
        let fraction = (value / max).clamp(0.0, 1.0);
        let y = (size.height * (1.0 - fraction)).clamp(5.0, (size.height - 5.0).max(5.0));
        frame.fill_text(canvas::Text {
            content: format_value(value),
            position: Point::new(3.0, y),
            color,
            size: Pixels(9.0),
            align_y: iced::alignment::Vertical::Center,
            ..canvas::Text::default()
        });
    }
}
