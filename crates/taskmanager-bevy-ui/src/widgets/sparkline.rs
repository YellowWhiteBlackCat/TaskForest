#![allow(dead_code)]
// ^ The pure core + render adapters are consumed by the M1 page bodies and
// the headless tests; in-product call sites land with M1 (process table)
// and M2 (curves). Tracked, not accidental: docs/BEVY_UI_FRONTEND.md ladder.

//! Sparkline: pure sample→polyline projection + minimal bsn! bar render.
//!
//! **Pure core**: [`polyline`] maps a bounded sample slice onto polyline
//! vertices inside a `width × height` box with the TUI sparkline's Tufte
//! semantics (per-series min/max normalization shows recent SHAPE; the
//! absolute value lives in the adjacent text). The M2 chart work consumes
//! the same projection for real line rendering — this module is the shared
//! math, not a final look.
//!
//! **Render adapter**: a minimal bottom-aligned bar strip. Bars are the one
//! shape `bevy_ui` alone paints today; honest for placeholders and cheap
//! enough that M1 pages can use it for in-table trends immediately.

use bevy::color::Color;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{AlignItems, BackgroundColor, FlexDirection, Node, Val, percent, px};

use crate::palette::space_2;

/// One polyline vertex in UI pixel space: `x` from the box left, `y` from
/// the box TOP (bevy UI coordinates grow downward), so `y == 0` is the
/// maximum sample's line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SparkVertex {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

/// Project `samples` (oldest→newest) onto a polyline in a `width × height`
/// box.
///
/// Contract (headless-tested):
/// - empty input → empty output — the caller renders an honest empty state,
///   never a fabricated flat line;
/// - a single sample → a single vertex at the newest edge (`x == width`),
///   mid-height: one point has no range, and mid is the neutral projection;
/// - a constant series, or a non-finite min/max window → a flat mid-height
///   line: a flat trend still reads as a flat trend (TUI parity);
/// - a non-finite sample → that vertex clamps to the mid line, never to a
///   fabricated zero/maximum;
/// - finite input maps min→top, max→bottom-clamped-to-top, evenly spaced in
///   `x`, clamped to the box on both axes.
pub(crate) fn polyline(samples: &[f32], width: f32, height: f32) -> Vec<SparkVertex> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mid_y = height / 2.0;
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for &sample in samples {
        if sample.is_finite() {
            min = min.min(sample);
            max = max.max(sample);
        }
    }
    if !min.is_finite() || !max.is_finite() {
        // No finite observation at all: flat mid line, one vertex per sample.
        return spaced(samples.len(), width)
            .into_iter()
            .map(|x| SparkVertex { x, y: mid_y })
            .collect();
    }
    let range = max - min;
    let project = |sample: f32| -> f32 {
        if !sample.is_finite() || range <= 0.0 {
            return mid_y;
        }
        let normalized = ((sample - min) / range).clamp(0.0, 1.0);
        // y down: normalized 1 (max) → 0 (top), normalized 0 (min) → height.
        height * (1.0 - normalized)
    };
    spaced(samples.len(), width)
        .into_iter()
        .zip(samples.iter().copied().map(project))
        .map(|(x, y)| SparkVertex { x, y })
        .collect()
}

/// Even `x` positions across `width` for `count` samples: endpoints at 0 and
/// `width` (the newest sample sits on the right edge), one position per
/// sample. `count == 1` pins the lone sample to the newest edge.
fn spaced(count: usize, width: f32) -> Vec<f32> {
    match count {
        0 => Vec::new(),
        1 => vec![width],
        _ => (0..count)
            .map(|index| {
                let step = index as f32 / (count - 1) as f32;
                width * step
            })
            .collect(),
    }
}

/// Bar heights for the minimal render: the same normalization as
/// [`polyline`], expressed as a fraction of `height` (0..1, mid for the
/// degenerate cases). Bottom-aligned bars share the polyline's semantics, so
/// the two renderings cannot disagree about a series' shape.
pub(crate) fn bar_fractions(samples: &[f32]) -> Vec<f32> {
    let height = 1.0_f32;
    polyline(samples, 0.0, height)
        .into_iter()
        .map(|vertex| 1.0 - vertex.y.clamp(0.0, 1.0))
        .collect()
}

/// Render adapter: a bottom-aligned bar strip for `samples` in a
/// `width × height` box. `bar_width_px` comes from the page (density token);
/// `color` from the palette.
pub(crate) fn bars_scene(
    samples: &[f32],
    height_px: f32,
    bar_width_px: f32,
    color: Color,
) -> impl Scene + use<> {
    let bars: Vec<f32> = bar_fractions(samples)
        .into_iter()
        .map(|fraction| (fraction * height_px).max(1.0))
        .collect();
    bsn! {
        Node {
            width: percent(100),
            height: px(height_px),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexEnd,
            column_gap: Val::Px(space_2()),
        }
        Children [
            { bar_nodes(&bars, bar_width_px, color) },
        ]
    }
}

fn bar_nodes(fractions: &[f32], bar_width_px: f32, color: Color) -> Vec<impl Scene + use<>> {
    fractions
        .iter()
        .copied()
        .map(|height| {
            bsn! {
                Node { width: px(bar_width_px), height: px(height) }
                BackgroundColor(color)
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/headless/sparkline.rs"]
mod tests;
