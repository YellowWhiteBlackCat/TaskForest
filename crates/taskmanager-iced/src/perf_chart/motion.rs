//! Chart-motion primitives for the Iced Performance charts: the
//! sample-window slide transition (the smoother for the ~1 Hz sampling
//! cadence) and the damped dynamic value-range transition (the axis zoom).
//!
//! Both transitions are time-based and typed: the state carries the instant
//! a transition began and the phase it reached, so the headless tests
//! assert the resolved phase (or the resolved axis value) instead of
//! sleeping for a wall-clock interval.
//!
//! The slide follows the GPUI reference: the outgoing frame keeps its
//! positions while the incoming sample enters from the right, and the whole
//! curve eases into the incoming frame over [`SLIDE_DURATION`]. The
//! outgoing frame is reproduced EXACTLY at progress 0 and the incoming
//! frame EXACTLY at progress 1, so the transition never introduces a jump
//! of its own.

use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

use iced::{Point, Size};

use super::series_point_runs;

/// The slide transition duration (the GPUI graph-slide parity).
pub(crate) const SLIDE_DURATION: Duration = Duration::from_millis(180);

/// The dynamic value-range transition duration: the Y ceiling eases over
/// 120 ms so a rate spike never snaps the graph to a new scale.
pub(crate) const RANGE_DAMP_DURATION: Duration = Duration::from_millis(120);

/// The symmetric quadratic ease-in-out ramp (the GPUI `ease_in_out`
/// parity): a gentle start and stop with no overshoot.
#[must_use]
pub(crate) fn ease_in_out(raw: f32) -> f32 {
    let t = if raw.is_finite() {
        raw.clamp(0.0, 1.0)
    } else {
        1.0
    };
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

/// The eased slide progress for an elapsed time: 0 at the start of the
/// transition, 1 once it has run its course.
#[must_use]
pub(crate) fn slide_progress(elapsed: Duration) -> f32 {
    let raw = (elapsed.as_secs_f32() / SLIDE_DURATION.as_secs_f32()).clamp(0.0, 1.0);
    ease_in_out(raw)
}

/// The phase of one series' slide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SlidePhase {
    /// The curve is drawn at its settled positions.
    Settled,
    /// A slide is in flight: the curve eases from the outgoing frame into
    /// the incoming one.
    Sliding { progress: f32 },
}

impl SlidePhase {
    /// The eased progress of the phase (1.0 when settled).
    #[must_use]
    pub(crate) fn progress(self) -> f32 {
        match self {
            SlidePhase::Settled => 1.0,
            SlidePhase::Sliding { progress } => progress,
        }
    }
}

/// One series' slide transition: the window the chart last drew (the
/// slide's base) and the instant the current slide began.
#[derive(Clone, Debug, Default)]
pub(crate) struct SeriesSlide {
    /// The window the chart last drew.
    previous: Option<Rc<[f32]>>,
    /// The window the chart draws now.
    current: Option<Rc<[f32]>>,
    /// The instant the current slide began.
    started_at: Option<Instant>,
}

impl SeriesSlide {
    /// Advance the transition to `now` for the window the chart is about to
    /// draw. A new window generation restarts the slide; the same
    /// generation keeps the running slide advancing. The very first window
    /// has no outgoing frame, so it settles immediately.
    pub(crate) fn advance(&mut self, current: &Rc<[f32]>, now: Instant) -> SlidePhase {
        let changed = !matches!(&self.current, Some(known) if Rc::ptr_eq(known, current));
        if changed {
            self.previous = self.current.take();
            self.current = Some(Rc::clone(current));
            self.started_at = Some(now);
        }
        let (Some(previous), Some(started_at)) = (&self.previous, self.started_at) else {
            return SlidePhase::Settled;
        };
        if previous.len() < 2 {
            return SlidePhase::Settled;
        }
        let progress = slide_progress(now.saturating_duration_since(started_at));
        if progress >= 1.0 {
            SlidePhase::Settled
        } else {
            SlidePhase::Sliding { progress }
        }
    }

    /// The slide base: the outgoing window followed by the incoming one, so
    /// the transition can show the outgoing frame at its old positions
    /// while the incoming sample enters from the right.
    pub(crate) fn base(&self) -> Option<Vec<f32>> {
        let previous = self.previous.as_ref()?;
        let current = self.current.as_ref()?;
        let mut base = Vec::with_capacity(previous.len() + 1);
        base.extend_from_slice(previous);
        if let Some(incoming) = current.last() {
            base.push(*incoming);
        }
        Some(base)
    }

    /// The length of the outgoing window: the slide base's sample count
    /// minus the incoming sample.
    pub(crate) fn previous_len(&self) -> usize {
        self.previous.as_ref().map_or(0, |previous| previous.len())
    }

    /// The drawn position of the current window's sample `index` while a
    /// slide is in flight: the same interpolated position the curve uses,
    /// so the crosshair never points beside the sample it reports.
    pub(crate) fn sample_position(
        &self,
        current_len: usize,
        width: f32,
        progress: f32,
        index: usize,
    ) -> Option<f32> {
        let previous_len = self.previous_len();
        let positions = slide_positions(previous_len, current_len, width, progress);
        let offset = previous_len + 1 - current_len;
        positions.get(index + offset).copied()
    }

    /// The slide's projected points: the base window's runs at the
    /// interpolated positions between the outgoing and the incoming frame.
    pub(crate) fn points(
        &self,
        current_len: usize,
        size: Size,
        progress: f32,
    ) -> Option<Vec<Vec<Point>>> {
        let base = self.base()?;
        Some(slide_points(
            &base,
            self.previous_len(),
            current_len,
            size,
            progress,
        ))
    }
}

/// The points one series draws: the slide's interpolated runs while a slide
/// is in flight, the settled projection otherwise. The shared gap rule stays
/// with [`series_point_runs`], so a non-finite sample never becomes a drawn
/// line in either state.
#[must_use]
pub(crate) fn series_runs(
    slide: &SeriesSlide,
    phase: SlidePhase,
    samples: &[f32],
    size: Size,
) -> Vec<Vec<Point>> {
    if let SlidePhase::Sliding { progress } = phase
        && let Some(points) = slide.points(samples.len(), size, progress)
    {
        return points;
    }
    series_point_runs(samples, size)
}

/// The horizontal positions the slide base's samples travel through: the
/// outgoing frame's positions at progress 0 and the incoming frame's
/// positions at progress 1.
///
/// The base carries the outgoing window plus the incoming sample, so the
/// incoming sample starts one slot off the right edge and the outgoing
/// window's oldest sample leaves past the left edge — exactly the GPUI
/// "the freshly shifted series enters from the right by one sample slot"
/// motion. Pure so the transition geometry is unit-tested headlessly.
#[must_use]
pub(crate) fn slide_positions(
    previous_len: usize,
    current_len: usize,
    width: f32,
    progress: f32,
) -> Vec<f32> {
    let base_len = previous_len + 1;
    if base_len == 0 || current_len < 2 || previous_len < 2 || !width.is_finite() {
        return Vec::new();
    }
    let progress = progress.clamp(0.0, 1.0);
    let current_slot = width / (current_len - 1) as f32;
    let previous_slot = width / (previous_len - 1) as f32;
    // How many samples the incoming window dropped: the base's sample `i`
    // lands on the incoming window's sample `i - offset`.
    let offset = previous_len as isize + 1 - current_len as isize;
    (0..base_len)
        .map(|index| {
            let from = if index < previous_len {
                index as f32 * previous_slot
            } else {
                // The incoming sample waits one slot off the right edge.
                width + current_slot
            };
            let to = (index as isize - offset) as f32 * current_slot;
            from + (to - from) * progress
        })
        .collect()
}

/// The slide's projected points: the base window's finite runs at the
/// interpolated positions between the outgoing and the incoming frame.
///
/// The gap rule stays with the shared projection ([`series_point_runs`]):
/// this only re-attaches each projected point to its own base index, so a
/// non-finite sample still ends a run instead of being drawn across.
#[must_use]
pub(crate) fn slide_points(
    base: &[f32],
    previous_len: usize,
    current_len: usize,
    size: Size,
    progress: f32,
) -> Vec<Vec<Point>> {
    let positions = slide_positions(previous_len, current_len, size.width, progress);
    if positions.is_empty() {
        return Vec::new();
    }
    let mut runs = series_point_runs(base, size);
    let mut finite = base
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_finite())
        .map(|(index, _)| index);
    for run in &mut runs {
        for point in run.iter_mut() {
            let Some(index) = finite.next() else {
                break;
            };
            if let Some(x) = positions.get(index) {
                point.x = *x;
            }
        }
    }
    runs
}

/// The eased interpolation of a value range from `from` to `to` after
/// `elapsed`: the axis zoom that keeps a rate spike from snapping the
/// graph to a new ceiling. Pure so the ramp is unit-tested headlessly.
#[must_use]
pub(crate) fn damped_range_value(from: f32, to: f32, elapsed: Duration) -> f32 {
    if !from.is_finite() || !to.is_finite() {
        return to;
    }
    let raw = (elapsed.as_secs_f32() / RANGE_DAMP_DURATION.as_secs_f32()).clamp(0.0, 1.0);
    let eased = 1.0 - (1.0 - raw) * (1.0 - raw);
    from + (to - from) * eased
}

/// The damped value-range state: the range the chart shows now and the
/// target it eases toward.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DampedRange {
    /// The range the current transition started from.
    from: f32,
    /// The range the current transition targets.
    to: f32,
    /// The instant the current transition began.
    started_at: Instant,
}

impl DampedRange {
    /// Start a settled transition at `target`.
    #[must_use]
    pub(crate) fn new(target: f32, now: Instant) -> Self {
        Self {
            from: target,
            to: target,
            started_at: now,
        }
    }

    /// Advance the transition toward `target` at `now` and return the
    /// resolved range. A changed target restarts the transition from the
    /// range that is on screen, so the ceiling never jumps.
    pub(crate) fn advance(&mut self, target: f32, now: Instant) -> f32 {
        if !target.is_finite() {
            return self.value(now);
        }
        if self.to != target {
            self.from = self.value(now);
            self.to = target;
            self.started_at = now;
        }
        self.value(now)
    }

    /// The resolved range at `now`.
    #[must_use]
    pub(crate) fn value(&self, now: Instant) -> f32 {
        damped_range_value(
            self.from,
            self.to,
            now.saturating_duration_since(self.started_at),
        )
    }
}

/// The maximum number of damped ranges one ledger keeps: a device list
/// longer than this evicts the least recently touched entry.
const DAMPED_RANGE_LEDGER_LIMIT: usize = 32;

/// A bounded ledger of damped value ranges, one entry per rendered device
/// identity: the renderer memo that lets a device page damp its dynamic Y
/// ceiling without owning a second projection cache.
#[derive(Default)]
pub(crate) struct DampedRangeLedger {
    entries: HashMap<String, DampedRange>,
    order: VecDeque<String>,
}

impl DampedRangeLedger {
    /// The damped range for `key`, easing toward `target` at `now`.
    pub(crate) fn resolve(&mut self, key: &str, target: f32, now: Instant) -> f32 {
        if !target.is_finite() {
            return target;
        }
        if !self.entries.contains_key(key) {
            self.evict();
            self.entries
                .insert(key.to_owned(), DampedRange::new(target, now));
            self.order.push_back(key.to_owned());
            return target;
        }
        if let Some(entry) = self.entries.get_mut(key) {
            let resolved = entry.advance(target, now);
            self.touch(key);
            resolved
        } else {
            target
        }
    }

    /// Move `key` to the ledger's fresh end.
    fn touch(&mut self, key: &str) {
        if let Some(position) = self.order.iter().position(|entry| entry == key) {
            self.order.remove(position);
        }
        self.order.push_back(key.to_owned());
    }

    /// Drop the least recently touched entry when the ledger is full.
    fn evict(&mut self) {
        while self.entries.len() >= DAMPED_RANGE_LEDGER_LIMIT {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }
}
