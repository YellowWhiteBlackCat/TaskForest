//! Bounded, gap-aware chart projection for the Bevy performance surface.
//!
//! Bevy UI 0.19 lays out the surface; this module keeps the measurement math
//! toolkit-neutral and makes the render seam a small `bsn!` scene. Non-finite
//! observations create gaps rather than joining across missing data.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::math::Rot2;
use bevy::scene::{Scene, bsn, template_value};
use bevy::ui::prelude::{BackgroundColor, Node, PositionType, UiTransform, percent, px};

/// Hard upper bound used by the performance chart surface.
pub(crate) const MAX_CHART_POINTS: usize = 600;

/// Visual thickness of one polyline segment in px.
pub(crate) const CHART_LINE_THICKNESS_PX: f32 = 2.0;

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

/// Layout of one segment as an absolutely-positioned rotated rectangle: the
/// untransformed top-left, the rectangle length along x, and the clockwise
/// rotation that aims it from `start` to `end`. A zero-length segment (two
/// coincident points) degrades to 1px instead of disappearing — a real
/// observation is never rendered as nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SegmentLayout {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) length: f32,
    pub(crate) rotation: f32,
}

#[must_use]
pub(crate) fn segment_layout(segment: ChartSegment) -> SegmentLayout {
    let dx = segment.end.x - segment.start.x;
    let dy = segment.end.y - segment.start.y;
    let length = dx.hypot(dy).max(1.0);
    let center_x = (segment.start.x + segment.end.x) / 2.0;
    let center_y = (segment.start.y + segment.end.y) / 2.0;
    SegmentLayout {
        left: center_x - length / 2.0,
        top: center_y - CHART_LINE_THICKNESS_PX / 2.0,
        length,
        rotation: dy.atan2(dx),
    }
}

/// One polyline segment as a rotated 2px rectangle. The transform rotates
/// around the node center, so the untransformed top-left is placed such that
/// the CENTER lands on the segment midpoint — the pure [`segment_layout`]
/// math is the authority and headless tests pin it.
pub(crate) fn segment_scene(
    segment: ChartSegment,
    color: bevy::color::Color,
) -> impl Scene + use<> {
    let layout = segment_layout(segment);
    let transform = UiTransform::from_rotation(Rot2::radians(layout.rotation));
    bsn! {
        Node {
            width: px(layout.length),
            height: px(CHART_LINE_THICKNESS_PX),
            position_type: PositionType::Absolute,
            left: px(layout.left),
            top: px(layout.top),
        }
        BackgroundColor(color)
        template_value(transform)
    }
}

/// The polyline render adapter: one clipped absolute layer of segment
/// rectangles inside the chart strip. Bounded by [`MAX_CHART_POINTS`] at the
/// projection layer; gaps are simply absent segments (never zero-joined).
pub(crate) fn polyline_scene(
    segments: &[ChartSegment],
    color: bevy::color::Color,
) -> impl Scene + use<> {
    let parts: Vec<Box<dyn Scene>> = segments
        .iter()
        .copied()
        .map(|segment| Box::new(segment_scene(segment, color)) as Box<dyn Scene>)
        .collect();
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            overflow: bevy::ui::Overflow::clip(),
        }
        Children [
            { parts },
        ]
    }
}

#[cfg(test)]
#[path = "../../tests/headless/chart.rs"]
mod tests;
