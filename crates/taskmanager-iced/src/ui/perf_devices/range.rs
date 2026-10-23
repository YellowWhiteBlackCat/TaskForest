//! The damped dynamic Y range for the device throughput graphs.
//!
//! A throughput graph's Y ceiling tracks the finite peak of its read/write
//! windows, so a traffic spike changes the scale. Switching the ceiling in
//! one step makes the whole curve jump, so the ceiling eases over the shared
//! 120 ms damped transition instead: the axis and the curve move together,
//! and the spike never snaps the graph to a new scale.
//!
//! One authority for every device page: the disk throughput graph and the
//! network rx/tx graph read the same damped ceiling, so two graphs of the
//! same product never disagree about how a spike should look.
//!
//! The ledger is renderer-local and bounded (one entry per rendered device
//! identity): a device page is a pure view function, so the transition state
//! cannot live in the application state machine.

use std::cell::RefCell;
use std::time::Instant;

use crate::perf_chart::DampedRangeLedger;

thread_local! {
    /// The renderer-local ledger of per-device damped Y ranges.
    static DAMPED_RANGES: RefCell<DampedRangeLedger> =
        RefCell::new(DampedRangeLedger::default());
}

/// The damped Y ceiling for one device's throughput graph: the resolved range
/// at `now`, easing toward `target` (the traffic peak) over the shared
/// damped transition.
///
/// A non-finite target keeps the ceiling it has (an unreadable window never
/// fabricates a scale); the key identifies the rendered device so two devices
/// never share one ceiling.
pub(crate) fn damped_ceiling(key: &str, target: f32, now: Instant) -> f32 {
    DAMPED_RANGES.with(|ranges| ranges.borrow_mut().resolve(key, target, now))
}
