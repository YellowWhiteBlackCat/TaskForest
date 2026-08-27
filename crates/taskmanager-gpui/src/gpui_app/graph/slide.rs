//! Window-owned timing for Mission Center graph slides.
//!
//! gpui element state is dropped when a page unmounts, so the animation clock
//! must outlive one element tree. The clock still belongs to one window: a
//! stable `ElementId` in another window cannot inherit it. Both the window
//! registry and each per-window graph ledger are bounded.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{ElementId, Window, WindowId, ease_in_out};

/// The freshly shifted series enters from the right by one sample slot.
pub(super) const GRAPH_SLIDE_DURATION: Duration = Duration::from_millis(180);

const MAX_SLIDE_LEDGER_ENTRIES: usize = 1024;
const MAX_SLIDE_LEDGER_WINDOWS: usize = 64;

#[derive(Clone)]
pub(super) struct SlideTiming {
    /// Bit-exact window content is the generation. An unrelated telemetry
    /// revision must never restart this graph's slide.
    samples: Rc<[f32]>,
    started_at: Instant,
}

pub(super) type SlideLedger = HashMap<ElementId, SlideTiming>;

struct WindowSlideLedger {
    timings: SlideLedger,
    last_painted_at: Instant,
}

thread_local! {
    static SLIDE_LEDGERS: RefCell<HashMap<WindowId, WindowSlideLedger>> =
        RefCell::new(HashMap::new());
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

pub(super) fn slide_timing_for_window(
    window: &Window,
    id: &ElementId,
    samples: &Rc<[f32]>,
) -> Instant {
    let window_id = window.window_handle().window_id();
    let now = Instant::now();
    SLIDE_LEDGERS.with(|ledgers| {
        let mut ledgers = ledgers.borrow_mut();
        if !ledgers.contains_key(&window_id) && ledgers.len() >= MAX_SLIDE_LEDGER_WINDOWS {
            let least_recent = ledgers
                .iter()
                .min_by_key(|(_, ledger)| ledger.last_painted_at)
                .map(|(id, _)| *id);
            if let Some(least_recent) = least_recent {
                ledgers.remove(&least_recent);
            }
        }
        let window_ledger = ledgers
            .entry(window_id)
            .or_insert_with(|| WindowSlideLedger {
                timings: HashMap::new(),
                last_painted_at: now,
            });
        window_ledger.last_painted_at = now;
        slide_timing_for(&mut window_ledger.timings, id, samples, now)
    })
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
