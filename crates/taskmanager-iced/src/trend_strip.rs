//! System-wide device trend strip for the Performance page.
//!
//! A compact "sidebar sparklines" surface for the Iced frontend: one mini
//! polyline per system-wide series (CPU / Memory / Disk / Network / GPU),
//! projected from the SHARED `LiveGraphHistory` (single source — the same windows
//! the TUI strip and the gpui sidebar histories read). Each series wears its
//! OWN semantic color (CPU / memory / disk / network / gpu theme tokens), so a
//! green CPU line reads apart from a blue memory line at a glance — matching
//! the gpui sidebar, which tints each device's sparkline with its category
//! color. A series with fewer than two finite samples draws no polyline at all
//! (its caption stays) — never a fabricated flat line. Non-percentage series
//! (disk / network bytes/sec) auto-scale to their own finite peak so traffic
//! actually moves the line instead of clamping flat against the 100% ceiling.
//!
//! Cross-frame geometry reuse: iced repaints at vsync (~60 Hz) while the shared
//! series advance only ~1 Hz, so most frames would recompute all five polylines
//! and captions identically. The state therefore holds a `canvas::Cache` plus a
//! `TrendStripFingerprint`, clearing the cache only when the fingerprint changes
//! (any entry's immutable snapshot generation or auto-scale max) — the
//! multi-entry generalization of the `process_sparkline` pattern.

use std::cell::RefCell;
use std::rc::Rc;

use iced::mouse;
use iced::widget::canvas::{self, Cache, Geometry};
use iced::{Color, Point, Rectangle, Size};

use crate::app::Message;
use crate::perf_chart::{SeriesGeneration, line_path, series_point_runs_for};

/// The fixed canvas height for the strip. Width is `Fill` so the strip tracks
/// the panel width.
pub(crate) const STRIP_HEIGHT: f32 = 44.0;

/// Stroke width for each mini polyline.
const STROKE_WIDTH: f32 = 1.4;

/// One strip entry: the caption, the series samples, the stroke color, and the
/// value that maps to the top of the cell (`100.0` for percentages; the finite
/// peak for auto-scaled bytes/sec series).
pub(crate) struct TrendEntry {
    pub(crate) caption: &'static str,
    pub(crate) samples: Rc<[f32]>,
    pub(crate) color: Color,
    pub(crate) max: f32,
}

/// The system-wide trend strip: five labeled mini polylines side by side.
/// Built fresh each render from the shared `LiveGraphHistory` series and the
/// per-series theme color; produces no messages (read-only).
pub(crate) struct TrendStrip {
    pub(crate) entries: Vec<TrendEntry>,
    pub(crate) caption_color: Color,
}

impl TrendStrip {
    /// Build the strip from one snapshot of the five shared series plus the
    /// token-derived caption color (each entry carries its own stroke color).
    pub(crate) fn new(entries: Vec<TrendEntry>, caption_color: Color) -> Self {
        Self {
            entries,
            caption_color,
        }
    }

    /// The fingerprint identifying the geometry this program would draw for its
    /// current data. Pure so the cross-frame cache-clear gate is unit-tested
    /// without a renderer (see [`TrendStripFingerprint`]).
    #[must_use]
    pub(crate) fn fingerprint(&self) -> TrendStripFingerprint {
        TrendStripFingerprint::from_entries(&self.entries)
    }
}

/// The cached identity of the strip's geometry: one entry-fingerprint per
/// series, capturing immutable snapshot generation and auto-scale max. The
/// retained `Rc` detects every new history revision without hashing each frame.
/// Colors/captions are
/// NOT part of the fingerprint (a theme switch is rare and one stale-color
/// frame is acceptable — matches round-1 `process_sparkline`).
#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) struct TrendStripFingerprint {
    entries: Vec<TrendEntryFingerprint>,
}

/// One entry's contribution to the strip fingerprint.
#[derive(Clone, Default, PartialEq, Debug)]
struct TrendEntryFingerprint {
    samples: SeriesGeneration,
    max_bits: u32,
}

impl TrendStripFingerprint {
    /// Build the fingerprint from each entry's generation and scale.
    #[must_use]
    fn from_entries(entries: &[TrendEntry]) -> Self {
        Self {
            entries: entries
                .iter()
                .map(|entry| TrendEntryFingerprint {
                    samples: SeriesGeneration::new(&entry.samples),
                    max_bits: entry.max.to_bits(),
                })
                .collect(),
        }
    }
}

/// The persistent per-canvas state: the geometry [`Cache`] plus the last
/// fingerprint drawn into it. The fingerprint holds a `Vec` (one entry per
/// series) so it lives in a `RefCell` rather than a `Cell`; `Default`-derivable
/// so iced can seed one when the canvas node first appears. Both the `Cache`
/// and the `RefCell` reuse interior mutability so [`canvas::Program::draw`]
/// (which takes `&State`) can clear and redraw through a shared reference.
#[derive(Default)]
pub(crate) struct TrendStripState {
    cache: Cache,
    fingerprint: RefCell<TrendStripFingerprint>,
}

/// The x-origin of one entry's segment inside a frame of `size` — the pure
/// geometry seam the headless tests assert on (mirrors `series_point_runs`).
#[must_use]
pub(crate) fn segment_origin(index: usize, count: usize, size: Size) -> Point {
    Point::new(size.width * index as f32 / count as f32, 0.0)
}

/// The finite positive peak across `samples` — the auto-scale `max` for a
/// non-percentage (bytes/sec) trend series so its line rises with traffic
/// instead of clamping flat. `0.0` when every sample is empty/zero/non-finite
/// (idle), which [`series_point_runs_for`] renders as a flat baseline.
#[must_use]
pub(crate) fn finite_peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value > 0.0)
        .fold(0.0_f32, f32::max)
}

impl canvas::Program<Message> for TrendStrip {
    type State = TrendStripState;

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
        // recomputes the five polylines only on the ~1 Hz tick that moved any
        // series (or changed a bytes/sec auto-scale max).
        let current = self.fingerprint();
        let needs_clear = *state.fingerprint.borrow() != current;
        if needs_clear {
            *state.fingerprint.borrow_mut() = current;
            state.cache.clear();
        }
        // `Cache::draw` invokes the closure synchronously. Borrow the five
        // entries directly so a cached repaint does not clone every caption
        // and history window.
        let entries = &self.entries;
        let caption_color = self.caption_color;
        let geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            // The frame owns its own coordinate space (origin at the canvas
            // origin); all drawing is frame-relative, so no translation is
            // needed when the canvas sits at a nonzero position in the app.
            let count = entries.len().max(1);
            let frame_size = frame.size();
            let width = frame_size.width / count as f32;
            for (index, entry) in entries.iter().enumerate() {
                let origin = segment_origin(index, count, frame_size);
                // Caption pinned to the segment's top-left.
                frame.fill_text(iced::widget::canvas::Text {
                    content: entry.caption.to_string(),
                    position: origin,
                    color: caption_color,
                    size: iced::Pixels(11.0),
                    ..iced::widget::canvas::Text::default()
                });
                // The polyline occupies the caption's baseline band; a series
                // with fewer than two finite samples draws nothing — honest
                // collecting.
                let band_size = Size::new(width, frame_size.height - 14.0);
                for points in series_point_runs_for(&entry.samples, band_size, entry.max) {
                    if points.len() < 2 {
                        continue;
                    }
                    let shifted: Vec<Point> = points
                        .iter()
                        .map(|point| Point::new(point.x + origin.x, point.y + origin.y + 14.0))
                        .collect();
                    if let Some(path) = line_path(&shifted) {
                        frame.stroke(
                            &path,
                            iced::widget::canvas::Stroke::default()
                                .with_width(STROKE_WIDTH)
                                .with_color(entry.color),
                        );
                    }
                }
            }
        });
        vec![geometry]
    }
}

#[cfg(test)]
#[path = "../tests/gui/trend_strip_tests.rs"]
mod tests;
