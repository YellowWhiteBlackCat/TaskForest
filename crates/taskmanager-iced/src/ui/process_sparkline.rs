//! Per-row CPU-history sparkline for the Applications table, built on the same
//! iced 0.14 `Canvas` + `Program` edge as [`crate::perf_chart`] and
//! [`crate::app_history_chart`].
//!
//! Each canonical process node that owns a real CPU history renders one
//! process's rolling CPU% window as a thin
//! single-series polyline, mirroring the gpui per-row sparkline
//! (`processes_view/rows/cells.rs`). Aggregate rows carry no single history,
//! so they keep
//! a same-width blank cell to preserve column alignment.
//!
//! The point geometry lives in the pure [`process_sparkline_points`] seam
//! (auto-ranged to the row's OWN finite peak, like gpui's `sparkline`), so the
//! geometry math is unit-tested without a renderer. [`ProcessCpuSparkline`]
//! only feeds those points into an iced `Frame`.
//!
//! Cross-frame geometry reuse: iced repaints at vsync (~60 Hz) while the CPU
//! history advances only ~1 Hz, so most frames would recompute identical
//! geometry. The [`canvas::Program::State`] therefore holds a
//! [`canvas::Cache`] plus a [`SparklineFingerprint`]; the cache returns the
//! stored [`Geometry`] unchanged when the canvas bounds are stable AND the
//! cache has not been cleared, and [`ProcessCpuSparkline::draw`] clears it
//! only when the fingerprint changed (new sample / window shift / pid change).
//! The fingerprint carries the pid so a reshuffled row order, an exited
//! process, or a new process can never reuse another row's cached geometry.

use std::cell::RefCell;
use std::rc::Rc;

use iced::mouse;
use iced::widget::canvas::{self, Cache, Geometry};
use iced::{Color, Point, Rectangle, Size};

use crate::app::Message;
use crate::perf_chart::{SeriesGeneration, line_path, sample_x};
use taskmanager_core::core::process::ProcessLiveKey;

/// Fixed sparkline canvas height (matches the gpui per-row sparkline's 16 px
/// band). Width is the column width (`56`); the `Canvas` reports this height to
/// layout so the row height never fluctuates with the sample count.
pub(crate) const PROCESS_SPARK_HEIGHT: f32 = 16.0;
/// The Applications-table Trend column width (matches the gpui sparkline
/// column + its non-sortable Trend header).
pub(crate) const PROCESS_SPARK_WIDTH: f32 = 56.0;
/// Thin polyline stroke (matches the gpui `sparkline` thin stroke).
const STROKE_WIDTH: f32 = 1.0;

/// The per-process CPU-history sparkline program. Built from a shared projected
/// history window and the resolved (token-derived) stroke color; rebuilding the
/// row widget clones only the `Rc`, never the bounded sample buffer.
pub(crate) struct ProcessCpuSparkline {
    samples: Rc<[f32]>,
    color: Color,
    identity: ProcessLiveKey,
}

impl ProcessCpuSparkline {
    /// Build the sparkline program from one process's CPU% history plus the
    /// resolved (token-derived) stroke color and the owning live identity. The
    /// identity seeds the cross-frame fingerprint so a reshuffled row order or
    /// PID reuse can never reuse another process's cached geometry.
    pub(crate) fn new(samples: Rc<[f32]>, color: Color, identity: ProcessLiveKey) -> Self {
        Self {
            samples,
            color,
            identity,
        }
    }

    /// The fingerprint identifying the geometry this program would draw for its
    /// current data. Pure so the cross-frame cache-clear gate is unit-tested
    /// without a renderer (see [`SparklineFingerprint`]).
    #[must_use]
    pub(crate) fn fingerprint(&self) -> SparklineFingerprint {
        SparklineFingerprint::from_samples(self.identity, &self.samples)
    }
}

/// The cached identity of one sparkline's geometry: owning live identity plus immutable
/// snapshot generation. Retaining the `Rc` avoids hashing each frame. Two
/// programs with the same fingerprint would
/// stroke identical geometry, so the cache need not be cleared between them.
#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) struct SparklineFingerprint {
    identity: Option<ProcessLiveKey>,
    samples: SeriesGeneration,
}

impl SparklineFingerprint {
    /// Build the fingerprint for one live identity + immutable snapshot generation.
    #[must_use]
    fn from_samples(identity: ProcessLiveKey, samples: &Rc<[f32]>) -> Self {
        Self {
            identity: Some(identity),
            samples: SeriesGeneration::new(samples),
        }
    }
}

/// The persistent per-canvas state: the geometry [`Cache`] plus the last
/// fingerprint drawn into it. `Default`-derivable so iced can seed one when the
/// canvas node first appears; the `Cache` and the `Cell` both reuse interior
/// mutability so [`canvas::Program::draw`] (which takes `&State`) can clear and
/// redraw through a shared reference.
#[derive(Default)]
pub(crate) struct SparklineState {
    cache: Cache,
    fingerprint: RefCell<SparklineFingerprint>,
}

impl canvas::Program<Message> for ProcessCpuSparkline {
    type State = SparklineState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        // Clear the cache ONLY when the data fingerprint changed. iced's
        // `Cache::draw` then reuses last frame's Geometry whenever the bounds
        // are stable and the cache was not cleared — so the ~60 Hz repaint loop
        // recomputes the polyline only on the ~1 Hz tick that actually moved
        // the history.
        let current = self.fingerprint();
        if *state.fingerprint.borrow() != current {
            *state.fingerprint.borrow_mut() = current;
            state.cache.clear();
        }
        // The cache closure is synchronous. Avoid copying each process's
        // history window on every frame when its geometry is already cached.
        let samples = &self.samples;
        let color = self.color;
        let geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            let points = process_sparkline_points(samples, frame.size());
            // `process_sparkline_points` always yields ≥2 points (the
            // midpoint baseline for <2 samples, the polyline otherwise), so
            // `line_path` always strokes here.
            if let Some(path) = line_path(&points) {
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_width(STROKE_WIDTH)
                        .with_color(color),
                );
            }
        });
        vec![geometry]
    }
}

/// Project `samples` onto a sparkline polyline inside `size`, auto-ranged to
/// the sample set's OWN finite peak (≥ a tiny floor so an all-zero history
/// still draws a valid baseline instead of dividing by zero) — standard
/// sparkline behavior for a tiny, axis-less mini-chart where two rows aren't
/// meant to be compared in amplitude. Mirrors the gpui per-row `sparkline`
/// geometry (`gpui_app/elements.rs`).
///
/// Fewer than two samples return the midpoint horizontal baseline as two points
/// spanning the width, so the row height stays stable and nothing panics — the
/// honest cold-start state (the cell keeps its column, no fabricated trend).
///
/// The frame origin is the top-left and y grows downward: the finite peak maps
/// to the top edge (`y` = half a stroke under the midpoint), zero maps to the
/// midpoint. Samples spread evenly across the width oldest → newest. Pure so
/// the geometry is unit-tested without a renderer.
#[must_use]
pub(crate) fn process_sparkline_points(samples: &[f32], size: Size) -> Vec<Point> {
    let width = size.width.max(0.0);
    let height = size.height.max(0.0);
    let mid = height * 0.5;
    if samples.len() < 2 {
        // Midpoint horizontal baseline (two points spanning the width).
        return vec![Point::new(0.0, mid), Point::new(width, mid)];
    }
    // Half-band minus half-stroke so the polyline never clips at the edges.
    let amp = (height * 0.5 - STROKE_WIDTH * 0.5).max(0.0);
    // Auto-range to the finite peak (floor 1e-6 so an all-zero history still
    // draws a valid baseline instead of dividing by zero).
    let max = samples
        .iter()
        .copied()
        .filter(|sample| sample.is_finite())
        .fold(0.0_f32, f32::max)
        .max(1e-6);
    samples
        .iter()
        .enumerate()
        .map(|(index, &sample)| {
            let x = sample_x(index, samples.len(), width);
            let y = mid - (sample / max) * amp;
            Point::new(x, y)
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/process_sparkline_tests.rs"]
mod tests;
