//! Pure chart geometry for the Bevy performance surface.
//!
//! The projection is toolkit-neutral on purpose: a bounded sample window
//! becomes continuous polyline *runs*, so the headless tests can pin the
//! curve's shape without a renderer. A missing observation breaks the run —
//! the curve never joins across a gap, and a single observation draws
//! nothing.

/// One projected point of the curve, in chart-local pixels (y grows down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartVertex {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

/// One segment of a polyline: the pair of vertices the stroke covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartSegment {
    pub(crate) start: ChartVertex,
    pub(crate) end: ChartVertex,
}

/// One contiguous piece of a curve: the samples between two gaps.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ChartRun {
    pub(crate) vertices: Vec<ChartVertex>,
}

/// Hard upper bound used by the performance chart surface.
pub(crate) const MAX_CHART_POINTS: usize = 600;

/// Visual thickness of the chart stroke, in px. A chart geometry contract,
/// not a layout length.
pub(crate) const CHART_LINE_THICKNESS_PX: f32 = 2.0;

/// The vertical scale a curve projects against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ChartScale {
    /// Normalize across the window's own extrema.
    Window,
    /// Project against a fixed `0..ceiling` scale.
    Ceiling(f32),
}

/// The slot grid one curve's samples occupy inside the measured frame.
///
/// The product contract is a **right-anchored capacity window**: the newest
/// sample pins to the right edge and every sample owns one fixed slot of the
/// window's width. A window that holds fewer samples than the capacity
/// therefore grows a short, evenly-paced curve from the right edge leftward
/// instead of stretching a handful of samples across the whole frame — the
/// curve never claims a history the product does not have. GPUI's
/// `sample_x` and Iced's `WindowSlots` own the same rule; this is the Bevy
/// reading of it.
///
/// The drawing x and the hover resolution derive from the same denominator,
/// so a hover readout can never disagree with a drawn sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartSlots {
    sample_count: usize,
    capacity: usize,
    width: f32,
}

impl ChartSlots {
    /// Build the slot grid for a window of `sample_count` samples inside a
    /// `capacity`-wide frame. A capacity below the sample count is safe: the
    /// denominator is their maximum, so an over-full window degrades to the
    /// full-span spread rather than dividing by zero.
    #[must_use]
    pub(crate) fn new(sample_count: usize, capacity: usize, width: f32) -> Self {
        Self {
            sample_count,
            capacity,
            width,
        }
    }

    /// The shared denominator: the sample span, or the capacity span, or one.
    fn denominator(&self) -> f32 {
        self.sample_count
            .saturating_sub(1)
            .max(self.capacity.saturating_sub(1))
            .max(1) as f32
    }

    /// The x-coordinate of the sample at `index` (oldest→newest, the newest
    /// pinned at the right edge). A window with fewer than two samples has no
    /// span, so it carries no x at all.
    ///
    /// The slot is measured from the right and subtracted from the denominator
    /// BEFORE the divide, so a full window (capacity == sample count) resolves
    /// to `width * index / (count - 1)` bit-for-bit: the settled curve is the
    /// same numbers the full-span projection always painted, never a value
    /// that merely rounds to it.
    #[must_use]
    pub(crate) fn x(&self, index: usize) -> f32 {
        if self.sample_count < 2 {
            return 0.0;
        }
        let width = if self.width.is_finite() {
            self.width.max(0.0)
        } else {
            0.0
        };
        let denominator = self.denominator();
        let from_right = (self.sample_count - 1 - index.min(self.sample_count - 1)) as f32;
        width * (denominator - from_right) / denominator
    }

    /// The inverse of [`ChartSlots::x`]: map a frame-relative cursor x onto
    /// the nearest sample slot. `None` before the frame, for a non-positive or
    /// non-finite width, or when the window holds fewer than two samples.
    ///
    /// The newest sample owns the right edge, so a cursor AT the edge is the
    /// newest slot rather than nothing: the relative position clamps into the
    /// frame exactly like the reference surface's resolution.
    #[must_use]
    pub(crate) fn index_at(&self, cursor_x: f32) -> Option<usize> {
        if self.sample_count < 2
            || !cursor_x.is_finite()
            || !self.width.is_finite()
            || self.width <= 0.0
            || cursor_x < 0.0
        {
            return None;
        }
        let relative = (cursor_x / self.width).clamp(0.0, 1.0);
        let index = (self.sample_count as f32 - 1.0 - (1.0 - relative) * self.denominator())
            .round()
            .clamp(0.0, (self.sample_count - 1) as f32);
        Some(index as usize)
    }
}

/// The finite extent of a sample window, or `None` when nothing was measured.
fn finite_extrema(window: &[f32]) -> Option<(f32, f32)> {
    window
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(None, |range: Option<(f32, f32)>, value| {
            Some(match range {
                Some((min, max)) => (min.min(value), max.max(value)),
                None => (value, value),
            })
        })
}

/// Project the newest bounded sample window into continuous curve runs.
///
/// Every run is one unbroken polyline: the renderer strokes it as a single
/// path, so a direction change never opens a seam. A non-finite observation
/// ends the current run — the next finite sample starts a new one.
#[must_use]
pub(crate) fn chart_runs(
    samples: &[f32],
    width: f32,
    height: f32,
    max_points: usize,
    scale: ChartScale,
) -> Vec<ChartRun> {
    chart_runs_in(samples, width, height, max_points, scale, samples.len())
}

/// Project the newest bounded sample window into continuous curve runs over
/// a `capacity`-wide frame.
///
/// This is [`chart_runs`] with the product's right-anchored capacity window:
/// the curve occupies the newest slots of the frame and grows leftward as
/// samples arrive, instead of stretching the whole window across the frame.
#[must_use]
pub(crate) fn chart_runs_in(
    samples: &[f32],
    width: f32,
    height: f32,
    max_points: usize,
    scale: ChartScale,
    capacity: usize,
) -> Vec<ChartRun> {
    if samples.is_empty() || max_points == 0 || !width.is_finite() || !height.is_finite() {
        return Vec::new();
    }
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    let window = &samples[samples.len().saturating_sub(max_points)..];
    // The drawing budget can be smaller than the history the product keeps.
    // Once the window is at that budget the slots it draws ARE the slots it
    // owns, so a truncated window keeps spanning the frame instead of being
    // anchored against slots that were never going to be drawn.
    let capacity = capacity.min(max_points);
    let (floor, ceiling) = match scale {
        ChartScale::Window => match finite_extrema(window) {
            Some((min, max)) => (min, max),
            None => return Vec::new(),
        },
        ChartScale::Ceiling(ceiling) => {
            let ceiling = if ceiling.is_finite() && ceiling > 0.0 {
                ceiling
            } else {
                100.0
            };
            (0.0, ceiling)
        }
    };
    let span = ceiling - floor;
    let count = window.len();
    let slots = ChartSlots::new(count, capacity, width);

    let mut runs: Vec<ChartRun> = Vec::new();
    let mut current: Vec<ChartVertex> = Vec::new();
    for (index, value) in window.iter().copied().enumerate() {
        if !value.is_finite() {
            if current.len() >= 2 {
                runs.push(ChartRun {
                    vertices: std::mem::take(&mut current),
                });
            } else {
                current.clear();
            }
            continue;
        }
        let x = if count <= 1 { width } else { slots.x(index) };
        let y = if span <= 0.0 {
            height / 2.0
        } else {
            height * (1.0 - ((value - floor) / span).clamp(0.0, 1.0))
        };
        current.push(ChartVertex { x, y });
    }
    if current.len() >= 2 {
        runs.push(ChartRun { vertices: current });
    }
    runs
}

/// Flatten the curve runs into the segment list the legacy adapters read.
///
/// The segments stay a *derived* view: the renderer strokes the runs, so the
/// segment list is never the drawing truth.
#[must_use]
pub(crate) fn runs_to_segments(runs: &[ChartRun]) -> Vec<ChartSegment> {
    let mut segments = Vec::new();
    for run in runs {
        for pair in run.vertices.windows(2) {
            segments.push(ChartSegment {
                start: pair[0],
                end: pair[1],
            });
        }
    }
    segments
}

/// Project sample window into line segments anchored to the window's own
/// extrema (the legacy segment view of [`chart_runs`]).
#[must_use]
pub(crate) fn line_segments(
    samples: &[f32],
    width: f32,
    height: f32,
    max_points: usize,
) -> Vec<ChartSegment> {
    runs_to_segments(&chart_runs(
        samples,
        width,
        height,
        max_points,
        ChartScale::Window,
    ))
}

/// Project a sample window into line segments inside a `capacity`-wide frame.
///
/// This is [`line_segments`] with the product's right-anchored capacity
/// window: the curve occupies the newest slots of the frame and grows
/// leftward as samples arrive.
#[must_use]
pub(crate) fn line_segments_in(
    samples: &[f32],
    width: f32,
    height: f32,
    max_points: usize,
    capacity: usize,
) -> Vec<ChartSegment> {
    runs_to_segments(&chart_runs_in(
        samples,
        width,
        height,
        max_points,
        ChartScale::Window,
        capacity,
    ))
}

/// The measured extent of a set of projected points: the box the points were
/// projected into. A scene built from pre-projected points carries this so
/// the painter can re-project onto the slot it actually measured.
#[must_use]
pub(crate) fn points_extent(vertices: &[ChartVertex]) -> (f32, f32) {
    vertices
        .iter()
        .fold((0.0_f32, 0.0_f32), |(max_x, max_y), vertex| {
            (max_x.max(vertex.x), max_y.max(vertex.y))
        })
}

/// The vertices of a segment list, in order, without the duplicated joints.
#[must_use]
pub(crate) fn segment_vertices(segments: &[ChartSegment]) -> Vec<ChartVertex> {
    let mut vertices: Vec<ChartVertex> = Vec::new();
    for segment in segments {
        if vertices.last() != Some(&segment.start) {
            vertices.push(segment.start);
        }
        vertices.push(segment.end);
    }
    vertices
}
