//! Pure chart path construction and sample-to-point projections.

use iced::widget::canvas::Path;
use iced::{Point, Size};
use taskmanager_application::history_decimation::lttb_gap_preserving_runs;

use super::{WindowSlots, sample_x};

/// The Catmull-Rom tension for [`smooth_line_path`] (matches GPUI's
/// `SMOOTH_TENSION` — the curve passes through the sample points).
const SMOOTH_TENSION: f32 = 1.0 / 6.0;

/// Build the smooth open path through `points`: each segment becomes a cubic
/// Bezier whose control points come from the neighbouring samples
/// (Catmull-Rom → Bezier conversion). Returns `None` for fewer than three
/// vertices (the spline needs a neighbour on both sides of at least one
/// segment). Pure function so the spline geometry is unit-tested headlessly.
#[must_use]
pub(crate) fn smooth_line_path(points: &[Point]) -> Option<Path> {
    if points.len() < 3 || !points_are_finite(points) {
        return None;
    }
    Some(Path::new(|builder| {
        builder.move_to(points[0]);
        for index in 0..points.len() - 1 {
            let previous = points[index.saturating_sub(1)];
            let start = points[index];
            let end = points[index + 1];
            let next = points.get(index + 2).copied().unwrap_or(end);
            let ctrl_a = Point::new(
                start.x + (end.x - previous.x) * SMOOTH_TENSION,
                start.y + (end.y - previous.y) * SMOOTH_TENSION,
            );
            let ctrl_b = Point::new(
                end.x - (next.x - start.x) * SMOOTH_TENSION,
                end.y - (next.y - start.y) * SMOOTH_TENSION,
            );
            builder.bezier_curve_to(ctrl_a, ctrl_b, end);
        }
    }))
}

/// Build the closed smooth area polygon: the spline down the top edge,
/// closing square to this run's baseline corners. Returns `None` for fewer
/// than three vertices.
#[must_use]
pub(crate) fn smooth_area_path(points: &[Point], baseline_y: f32) -> Option<Path> {
    if points.len() < 3 || !baseline_y.is_finite() || !points_are_finite(points) {
        return None;
    }
    let first = points[0];
    let last = points[points.len() - 1];
    Some(Path::new(|builder| {
        builder.move_to(first);
        for index in 0..points.len() - 1 {
            let previous = points[index.saturating_sub(1)];
            let start = points[index];
            let end = points[index + 1];
            let next = points.get(index + 2).copied().unwrap_or(end);
            let ctrl_a = Point::new(
                start.x + (end.x - previous.x) * SMOOTH_TENSION,
                start.y + (end.y - previous.y) * SMOOTH_TENSION,
            );
            let ctrl_b = Point::new(
                end.x - (next.x - start.x) * SMOOTH_TENSION,
                end.y - (next.y - start.y) * SMOOTH_TENSION,
            );
            builder.bezier_curve_to(ctrl_a, ctrl_b, end);
        }
        builder.line_to(Point::new(last.x, baseline_y));
        builder.line_to(Point::new(first.x, baseline_y));
        builder.close();
    }))
}

/// Build the open polyline path through `points`. Returns `None` for fewer
/// than two vertices (no strokeable segment). Shared with the trend strip so
/// the two widgets cannot drift apart on polyline construction.
pub(crate) fn line_path(points: &[Point]) -> Option<Path> {
    if points.len() < 2 || !points_are_finite(points) {
        return None;
    }
    let first = points[0];
    Some(Path::new(|builder| {
        builder.move_to(first);
        for point in &points[1..] {
            builder.line_to(*point);
        }
    }))
}

/// Build the closed area polygon: the polyline followed back along the
/// baseline (`y = baseline_y`) so the fill reads as "area under the curve".
/// Returns `None` for fewer than two vertices. Shared with the device headline
/// graph and the per-core cell sparkline so every area fill closes back along
/// the same baseline.
pub(crate) fn area_path(points: &[Point], baseline_y: f32) -> Option<Path> {
    if points.len() < 2 || !baseline_y.is_finite() || !points_are_finite(points) {
        return None;
    }
    let first = points[0];
    let last = points[points.len() - 1];
    Some(Path::new(|builder| {
        builder.move_to(first);
        for point in &points[1..] {
            builder.line_to(*point);
        }
        builder.line_to(Point::new(last.x, baseline_y));
        builder.line_to(Point::new(first.x, baseline_y));
        builder.close();
    }))
}

/// Whether every point is safe to pass to iced/lyon's path builder.
fn points_are_finite(points: &[Point]) -> bool {
    points
        .iter()
        .all(|point| point.x.is_finite() && point.y.is_finite())
}

/// Project `samples` scaled to `max` into finite contiguous runs inside `size`.
///
/// Same geometry as the 0–100 percentage chart, but for series whose unit is
/// not a percentage (e.g. disk/network bytes/sec): each value maps to
/// `value / max` of the frame height. `max <= 0` (or all-zero samples) anchors
/// every point at the baseline — the honest idle state, never a fabricated
/// mid-line.
///
/// The frame origin is the top-left and y grows downward, so `0` maps to the
/// bottom edge (`y = height`) and `max` to the top edge (`y = 0`). Samples
/// spread evenly across the width left→right (oldest→newest); values are
/// clamped to `[0, max]` so an out-of-range reading cannot escape the frame.
///
/// A non-finite sample is an authoritative history gap. It ends the current
/// run while retaining its chronological slot in the x projection; the next
/// finite sample starts a new run at its original x. This prevents both an
/// invalid lyon point and the more subtle lie of filtering gaps and drawing a
/// line or area across missing evidence.
///
/// Empty/all-gap input yields no runs. Edge gaps likewise create no fabricated
/// vertices.
#[must_use]
pub(crate) fn series_point_runs_for(samples: &[f32], size: Size, max: f32) -> Vec<Vec<Point>> {
    series_point_runs_with(samples, size, 0.0, max, &|index| {
        sample_x(index, samples.len(), size.width)
    })
}

/// The windowed projection: the same value normalization and NaN-gap
/// splitting as [`series_point_runs_for`], but the x coordinates come from the
/// caller's [`WindowSlots`] grid — the identical mapping the chart's hover
/// resolves through, so a capacity-windowed graph and its readout cannot
/// disagree about where a sample sits.
#[must_use]
pub(crate) fn series_point_runs_windowed(
    samples: &[f32],
    size: Size,
    max: f32,
    slots: &WindowSlots,
) -> Vec<Vec<Point>> {
    series_point_runs_with(samples, size, 0.0, max, &|index| slots.x(index))
}

/// The shared run splitter behind both projections: finite contiguous runs of
/// points, y from [`scaled_y`], x from the injected slot mapping.
/// Complete recorded windows retain signed magnitude coordinates and gaps.
#[must_use]
pub(crate) fn series_point_runs_in_range(
    samples: &[f32],
    size: Size,
    min: f32,
    max: f32,
) -> Vec<Vec<Point>> {
    series_point_runs_with(samples, size, min, max, &|index| {
        sample_x(index, samples.len(), size.width)
    })
}

fn series_point_runs_with(
    samples: &[f32],
    size: Size,
    min: f32,
    max: f32,
    x_of: &dyn Fn(usize) -> f32,
) -> Vec<Vec<Point>> {
    let height = finite_nonnegative(size.height);
    let width = if size.width.is_finite() {
        size.width.clamp(0.0, 16_384.0)
    } else {
        0.0
    };
    let budget = (width.ceil() as usize).saturating_mul(2).max(2);
    lttb_gap_preserving_runs(samples, budget)
        .into_iter()
        .map(|run| {
            run.into_iter()
                .map(|(index, raw)| {
                    Point::new(x_of(index), scaled_y_in_range(raw, min, max, height))
                })
                .collect()
        })
        .collect()
}

/// The y-coordinate for one value under the exact normalization
/// [`series_point_runs_for`] applies: `0` at the baseline, `max` at the top
/// edge, out-of-range values clamped inside the frame, and a non-positive or
/// non-finite ceiling (or value) anchoring at the baseline — the honest idle
/// state, never a fabricated mid-line. Pure so a hover dot provably lands on
/// the drawn sample.
#[must_use]
pub(crate) fn scaled_y(value: f32, max: f32, height: f32) -> f32 {
    scaled_y_in_range(value, 0.0, max, height)
}

#[must_use]
pub(crate) fn scaled_y_in_range(value: f32, min: f32, max: f32, height: f32) -> f32 {
    let height = finite_nonnegative(height);
    let floor = if min.is_finite() { min.min(0.0) } else { 0.0 };
    let ceiling = if max.is_finite() { max.max(floor) } else { 0.0 };
    let span = f64::from(ceiling) - f64::from(floor);
    if !value.is_finite() || span <= 0.0 {
        return height;
    }
    height * (1.0 - ((f64::from(value) - f64::from(floor)) / span).clamp(0.0, 1.0) as f32)
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

/// Project normalized `[0,100]` percentage samples into finite contiguous
/// runs — the percentage-specialization of [`series_point_runs_for`].
#[must_use]
pub(crate) fn series_point_runs(samples: &[f32], size: Size) -> Vec<Vec<Point>> {
    series_point_runs_for(samples, size, 100.0)
}
