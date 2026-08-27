//! Mini line sparkline for the App-history page rows, built on the same iced
//! 0.14 `Canvas` + `Program` edge as [`crate::perf_chart`].
//!
//! Each App-history row renders one application's rolling CPU% window as a
//! thin single-series polyline. The point geometry reuses the shared
//! [`crate::perf_chart::series_point_runs`] projection (already unit-tested) so the
//! two pages never disagree on how a `[0,100]` sample maps onto a frame; this
//! module only owns the one-series stroke. Read-only — the sparkline produces
//! no messages.
//!
//! Honesty matches the rest of the typed surface: a window with fewer than two
//! samples strokes nothing (the caller substitutes an explicit "collecting"
//! cell), and an empty window never fabricates a flat baseline.
//!
//! Cross-frame geometry reuse: iced repaints at vsync (~60 Hz) while the CPU%
//! history advances only ~1 Hz, so most frames would recompute identical
//! geometry. The [`canvas::Program::State`] therefore holds a [`canvas::Cache`]
//! plus an [`AppSparkFingerprint`]; the cache returns the stored [`Geometry`]
//! unchanged when the canvas bounds are stable AND the cache has not been
//! cleared, and [`Sparkline::draw`] clears it only when the fingerprint
//! changed (new sample / window shift) — the same pattern
//! `process_sparkline` verified.

use std::cell::RefCell;
use std::rc::Rc;

use iced::mouse;
use iced::widget::canvas::{self, Cache, Geometry};
use iced::{Color, Rectangle};

use crate::app::Message;
use crate::perf_chart::{SeriesGeneration, line_path, series_point_runs};

/// Fixed sparkline canvas height. Width is `Fill` against the row cell so the
/// line tracks the column; the `Canvas` reports this height to layout.
pub(crate) const SPARK_HEIGHT: f32 = 22.0;
/// Stroke width for the sparkline polyline (slightly thinner than the
/// Performance chart's two-series stroke — one series needs less weight).
const SPARK_STROKE_WIDTH: f32 = 1.4;

/// A single-series mini polyline for one application's CPU% history. Built
/// fresh each render from that app's rolling samples and a token-derived color.
pub(crate) struct Sparkline {
    samples: Rc<[f32]>,
    color: Color,
}

impl Sparkline {
    /// Build the sparkline program from one app's CPU% series plus the resolved
    /// (token-derived) stroke color.
    pub(crate) fn new<S>(samples: S, color: Color) -> Self
    where
        S: Into<Rc<[f32]>>,
    {
        Self {
            samples: samples.into(),
            color,
        }
    }

    /// The fingerprint identifying the geometry this program would draw for its
    /// current data. Pure so the cross-frame cache-clear gate is unit-tested
    /// without a renderer (see [`AppSparkFingerprint`]).
    #[must_use]
    pub(crate) fn fingerprint(&self) -> AppSparkFingerprint {
        AppSparkFingerprint::from_samples(&self.samples)
    }
}

/// The cached identity of one immutable snapshot generation. Retaining the
/// `Rc` detects shifted windows without hashing each frame. Color is NOT part
/// of the fingerprint (a theme switch is
/// rare and a stale-color frame on theme change is acceptable — matches the
/// round-1 `process_sparkline` decision).
#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) struct AppSparkFingerprint {
    samples: SeriesGeneration,
}

impl AppSparkFingerprint {
    /// Build the fingerprint for one immutable sample-window generation.
    #[must_use]
    fn from_samples(samples: &Rc<[f32]>) -> Self {
        Self {
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
    fingerprint: RefCell<AppSparkFingerprint>,
}

impl canvas::Program<Message> for Sparkline {
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
        // `Cache::draw` consumes the closure immediately; borrow the history
        // instead of allocating a copy on every cached repaint.
        let samples = &self.samples;
        let color = self.color;
        let geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            let size = frame.size();
            // Reuse the Performance chart's `[0,100]`→frame projection so a
            // sparkline and the full chart read the same sample identically.
            for points in series_point_runs(samples, size) {
                if let Some(line) = line_path(&points) {
                    frame.stroke(
                        &line,
                        canvas::Stroke::default()
                            .with_width(SPARK_STROKE_WIDTH)
                            .with_color(color),
                    );
                }
            }
        });
        vec![geometry]
    }
}

#[cfg(test)]
#[path = "../tests/gui/app_history_chart_tests.rs"]
mod tests;
