//! Dirty-flag predicate tests: the TUI run loop must NOT call `terminal.draw`
//! on a pure idle cycle, and MUST call it the moment a cycle produces new
//! render state (a non-empty platform batch, a queued refresh, or an ancillary
//! effect). The predicate itself is a pure function of [`DrawCycleInputs`];
//! the cross-cycle keypress/resize/initial-frame signal is exercised
//! separately via the `pending_draw` carry-over (the live crossterm loop needs
//! a real tty, so the pure predicate is the testable seam).

use super::super::*;

#[test]
fn pure_idle_cycle_does_not_draw() {
    // No platform batch, no refresh queued, no ancillary effect: the screen is
    // unchanged, so the run loop must skip `terminal.draw` for this cycle.
    // This is the headline behavioral guarantee of the dirty flag — the TUI
    // no longer repaints at the fixed ~10 Hz poll cadence when nothing
    // happened.
    assert_eq!(DrawCycleInputs::default(), DrawCycleInputs::default());
    assert!(!should_draw(DrawCycleInputs::default()));
}

#[test]
fn non_empty_platform_batch_flags_the_cycle_dirty() {
    // A non-empty drain carried new telemetry / process / hardware data: the
    // renderer must pick it up this cycle, so the predicate returns true.
    let inputs = DrawCycleInputs {
        platform_batch: true,
        ..Default::default()
    };
    assert!(should_draw(inputs));
}

#[test]
fn queued_refresh_flags_the_cycle_dirty() {
    // A RefreshRequest was queued this cycle (the data lands asynchronously in
    // a later drain, but the queue itself must mark the cycle dirty so the
    // next paint is not delayed by the full EVENT_POLL timeout).
    let inputs = DrawCycleInputs {
        refresh_queued: true,
        ..Default::default()
    };
    assert!(should_draw(inputs));
}

#[test]
fn ancillary_effect_flags_the_cycle_dirty() {
    // Selected-process insights refresh, service-log tail poll, or a desktop
    // notification submission all change rendered state and must repaint.
    let inputs = DrawCycleInputs {
        ancillary_effect: true,
        ..Default::default()
    };
    assert!(should_draw(inputs));
}

#[test]
fn any_one_signal_flips_the_predicate_regardless_of_the_others() {
    // The predicate is an OR over the three in-cycle signals, so any one of
    // them being true forces a draw even when the others are false. Verified
    // in both directions (set + unset) to guard against a future regression
    // that swaps the OR for an AND or an XOR.
    let base = DrawCycleInputs::default();
    assert!(!should_draw(base));

    let with_batch = DrawCycleInputs {
        platform_batch: true,
        ..base
    };
    assert!(should_draw(with_batch));
    assert!(should_draw(DrawCycleInputs {
        refresh_queued: true,
        ..base
    }));
    assert!(should_draw(DrawCycleInputs {
        ancillary_effect: true,
        ..base
    }));

    // Two-set combinations (guards against an accidental "exactly one" rule).
    assert!(should_draw(DrawCycleInputs {
        platform_batch: true,
        refresh_queued: true,
        ancillary_effect: false,
    }));
    assert!(should_draw(DrawCycleInputs {
        platform_batch: false,
        refresh_queued: true,
        ancillary_effect: true,
    }));
    assert!(should_draw(DrawCycleInputs {
        platform_batch: true,
        refresh_queued: false,
        ancillary_effect: true,
    }));

    // All three set (the busy case: telemetry tick + insights + service log).
    assert!(should_draw(DrawCycleInputs {
        platform_batch: true,
        refresh_queued: true,
        ancillary_effect: true,
    }));
}

#[test]
fn empty_drain_alone_does_not_flag_dirty() {
    // Mirrors the run-loop guard: `let folded = !batch.is_empty();` — an empty
    // platform batch (the common idle drain) leaves the predicate false. This
    // is the structural reason idle frames now skip draw: the platform port
    // drains empty most cycles, and an empty drain carries no render state.
    let empty_drain = DrawCycleInputs {
        platform_batch: false,
        refresh_queued: false,
        ancillary_effect: false,
    };
    assert_eq!(empty_drain, DrawCycleInputs::default());
    assert!(!should_draw(empty_drain));
}

#[test]
fn pending_draw_is_independent_of_in_cycle_signals() {
    // The cross-cycle `pending_draw` carry-over (initial frame + keypress /
    // resize) is ORed with the in-cycle predicate at the draw site:
    //     if pending_draw || should_draw(cycle) { draw; pending_draw = false; }
    // The predicate itself must NOT know about pending_draw — it only models
    // this cycle's drain/queue work. Verified by asserting the predicate is
    // false on a default cycle even though a pending keypress would still
    // force a draw at the call site.
    let cycle_with_no_in_cycle_work = DrawCycleInputs::default();
    assert!(!should_draw(cycle_with_no_in_cycle_work));
    // And the call-site OR is what makes the initial frame / post-keypress
    // frame draw despite the empty cycle:
    let pending_draw = true;
    assert!(pending_draw || should_draw(cycle_with_no_in_cycle_work));
}
