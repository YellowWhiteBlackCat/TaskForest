//! Bounded, gap-aware chart projection for the Bevy performance surface.
//!
//! Bevy UI 0.19 lays out the surface; this module keeps the measurement math
//! toolkit-neutral and makes the render seam a small `bsn!` scene. Non-finite
//! observations create gaps rather than joining across missing data.

#![allow(dead_code)]

use bevy::ecs::component::Component;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{BackgroundColor, Node, percent, px};

use crate::palette::UiPalette;

/// Hard upper bound used by the performance chart surface.
pub(crate) const MAX_CHART_POINTS: usize = 600;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartVertex {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartSegment {
    pub(crate) start: ChartVertex,
    pub(crate) end: ChartVertex,
}

/// Project the newest bounded sample window into line segments.
///
/// A segment is emitted only between adjacent finite observations. A gap,
/// including a leading or trailing unavailable sample, is therefore visible
/// to the eventual renderer and is never converted into a zero.
#[must_use]
pub(crate) fn line_segments(
    samples: &[f32],
    width: f32,
    height: f32,
    max_points: usize,
) -> Vec<ChartSegment> {
    if samples.is_empty() || max_points == 0 {
        return Vec::new();
    }
    let window = &samples[samples.len().saturating_sub(max_points)..];
    let (min, max) = window
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(None, |range: Option<(f32, f32)>, value| {
            Some(match range {
                Some((min, max)) => (min.min(value), max.max(value)),
                None => (value, value),
            })
        })
        .unwrap_or((0.0, 0.0));
    let value_range = max - min;
    let position = |index: usize, value: f32| -> ChartVertex {
        let x = if window.len() <= 1 {
            width
        } else {
            width * index as f32 / (window.len() - 1) as f32
        };
        let y = if !value.is_finite() || value_range <= 0.0 {
            height / 2.0
        } else {
            height * (1.0 - ((value - min) / value_range).clamp(0.0, 1.0))
        };
        ChartVertex { x, y }
    };

    let mut segments = Vec::new();
    let mut previous = None;
    for (index, value) in window.iter().copied().enumerate() {
        if !value.is_finite() {
            previous = None;
            continue;
        }
        let current = position(index, value);
        if let Some(start) = previous {
            segments.push(ChartSegment {
                start,
                end: current,
            });
        }
        previous = Some(current);
    }
    segments
}

/// The layout/render anchor for a chart surface. `segment_count` is metadata
/// only until the vector renderer is integrated; retaining it makes the scene
/// truthful and headlessly inspectable.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChartSurface(pub(crate) usize);

/// Minimal themed chart surface scene. The Performance page uses the same
/// projected segment count as a live `ChartSurface` metadata anchor while its
/// current Bevy UI draw adapter renders bounded bars.
pub(crate) fn chart_scene(
    samples: &[f32],
    width_px: f32,
    height_px: f32,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let segment_count = line_segments(samples, width_px, height_px, MAX_CHART_POINTS).len();
    bsn! {
        Node { width: percent(100), height: px(height_px) }
        BackgroundColor({ palette.panel_fill })
        ChartSurface({ segment_count })
    }
}

#[cfg(test)]
#[path = "../../tests/headless/chart.rs"]
mod tests;
