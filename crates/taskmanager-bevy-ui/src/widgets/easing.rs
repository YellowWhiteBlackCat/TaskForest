//! The frontend's one interaction easing curve (BEVY-182).
//!
//! Every interaction-state transition in this frontend — the switch thumb's
//! slide, the scrollbar thumb's fade, the focus ring's halo — rides the same
//! typed curve instead of each widget inventing its own timing. The curve is
//! the standard cubic-bezier **ease-out** `(0.16, 1, 0.3, 1)`: a fast start
//! that settles gently, which is what a state change on a desktop surface
//! reads as "immediate, but not abrupt". Naming one constant is this module's
//! whole job: the solve, the sampling, and the elapsed-ratio rule are owned by
//! `zhuge-ui-core::easing`, so the frontend never carries a second solver.

use zhuge_ui_core::easing::{CubicBezier, transition_progress};

/// The curve every interaction-state transition rides: cubic-bezier ease-out
/// `(0.16, 1, 0.3, 1)`.
///
/// The x coordinates are the input progress anchors, the y coordinates the
/// output anchors: 16% of the input time covers the first anchor, the curve
/// then races to its peak and eases into the final value. A widget asks for
/// this constant by name, never for hand-rolled timing, so "every interaction
/// uses the standard ease-out" is a property of the type rather than a
/// convention.
pub(crate) const INTERACTION_EASE_OUT: CubicBezier = CubicBezier::new(0.16, 1.0, 0.3, 1.0);

/// The eased progress of a transition that started at `elapsed` and lasts
/// `duration`, sampled on the frontend's one interaction curve.
#[must_use]
pub(crate) fn eased_progress(elapsed: std::time::Duration, duration: std::time::Duration) -> f32 {
    INTERACTION_EASE_OUT.sample(transition_progress(elapsed, duration))
}

#[cfg(test)]
#[path = "../../tests/headless/widgets/easing.rs"]
mod tests;
