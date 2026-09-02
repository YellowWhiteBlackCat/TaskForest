//! Window-owned timing for Mission Center graph slides.
//!
//! gpui element state is dropped when a page unmounts, so the animation clock
//! must outlive one element tree. The clock still belongs to one window: a
//! stable `ElementId` in another window cannot inherit it. Both the window
//! registry and each per-window graph ledger are bounded.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{ElementId, Window, ease_in_out};

/// The freshly shifted series enters from the right by one sample slot.
pub(super) const GRAPH_SLIDE_DURATION: Duration = Duration::from_millis(180);

const MAX_SLIDE_LEDGER_ENTRIES: usize = 1024;

#[derive(Clone)]
pub(super) struct SlideTiming {
    /// Bit-exact window content is the generation. An unrelated telemetry
    /// revision must never restart this graph's slide.
    samples: Rc<[f32]>,
    started_at: Instant,
}

pub(super) type SlideLedger = HashMap<ElementId, SlideTiming>;

#[derive(Default)]
pub(super) struct SlideCache {
    ledger: SlideLedger,
}

impl SlideCache {
    pub(super) fn timing_for(
        &mut self,
        id: &ElementId,
        samples: &Rc<[f32]>,
        now: Instant,
    ) -> Instant {
        slide_timing_for(&mut self.ledger, id, samples, now)
    }
}

/// Return one graph generation's stable start time inside a supplied ledger.
///
/// Content equality, not `Rc` identity, is authoritative: a full history ring
/// may allocate an equivalent tail slice on a later frame.
pub(super) fn slide_timing_for(
    ledger: &mut SlideLedger,
    id: &ElementId,
    samples: &Rc<[f32]>,
    now: Instant,
) -> Instant {
    if let Some(timing) = ledger.get_mut(id) {
        if samples_bit_eq(&timing.samples, samples) {
            return timing.started_at;
        }
        timing.samples = Rc::clone(samples);
        timing.started_at = now;
        return timing.started_at;
    }
    if ledger.len() >= MAX_SLIDE_LEDGER_ENTRIES {
        ledger.retain(|_, timing| Rc::strong_count(&timing.samples) > 1);
        if ledger.len() >= MAX_SLIDE_LEDGER_ENTRIES {
            ledger.clear();
        }
    }
    ledger.insert(
        id.clone(),
        SlideTiming {
            samples: Rc::clone(samples),
            started_at: now,
        },
    );
    now
}

fn samples_bit_eq(left: &[f32], right: &[f32]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.to_bits() == right.to_bits())
}

pub(super) fn slide_progress(started_at: Instant, window: &mut Window) -> f32 {
    let now = Instant::now();
    if now.duration_since(started_at) < GRAPH_SLIDE_DURATION {
        window.request_animation_frame();
    }
    slide_progress_value(started_at, now)
}

pub(super) fn slide_progress_value(started_at: Instant, now: Instant) -> f32 {
    let raw = now.duration_since(started_at).as_secs_f32() / GRAPH_SLIDE_DURATION.as_secs_f32();
    ease_in_out(raw.clamp(0.0, 1.0))
}
