//! The frontend's one interaction easing curve (BEVY-182).
//!
//! Every interaction-state transition in this frontend — the switch thumb's
//! slide, the scrollbar thumb's fade, the focus ring's halo — rides the same
//! typed curve instead of each widget inventing its own timing. The curve is
//! the standard cubic-bezier **ease-out** `(0.16, 1, 0.3, 1)`: a fast start
//! that settles gently, which is what a state change on a desktop surface
//! reads as "immediate, but not abrupt".
//!
//! The module is a **pure core** (no bevy deps): the curve is data, so the
//! headless test surface can pin its shape without a world, and the render
//! adapters only ever ask it for a progress value.

/// The standard cubic-bezier ease-out control points `(x1, y1, x2, y2)`.
///
/// The x coordinates are the input progress anchors, the y coordinates the
/// output anchors. `(0.16, 1, 0.3, 1)` means: 16% of the input time covers
/// the first anchor, the curve then races to its peak and eases into the
/// final value.
pub(crate) const EASE_OUT_POINTS: [f32; 4] = [0.16, 1.0, 0.3, 1.0];

/// The typed easing curve an interaction transition rides.
///
/// The enum is the vocabulary: a widget asks for a curve by name, never by
/// hand-rolled timing, so "every interaction uses the standard ease-out" is a
/// property of the type rather than a convention.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Easing {
    /// The standard cubic-bezier ease-out `(0.16, 1, 0.3, 1)`.
    #[default]
    EaseOut,
}

impl Easing {
    /// The curve's cubic-bezier control points `(x1, y1, x2, y2)`.
    #[must_use]
    pub(crate) const fn control_points(self) -> [f32; 4] {
        match self {
            Self::EaseOut => EASE_OUT_POINTS,
        }
    }

    /// The curve's output for the input progress `t` in `0.0..=1.0`.
    ///
    /// `t = 0.0` answers `0.0` and `t = 1.0` answers `1.0` exactly: the
    /// curve starts and ends on the value it transitions between, so an
    /// animation never overshoots its own endpoints. Progress outside the
    /// range is clamped — an over-long frame lands on the final value
    /// instead of extrapolating past it.
    #[must_use]
    pub(crate) fn evaluate(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        let [x1, y1, x2, y2] = self.control_points();
        // Solve the curve's x axis for the parameter, then read the y axis.
        // Newton-Raphson converges in a handful of steps; the bisection
        // fallback keeps the answer monotone even where the derivative is
        // flat (near the anchors).
        let mut parameter = t;
        for _ in 0..8 {
            let error = axis(parameter, x1, x2) - t;
            if error.abs() < 1e-6 {
                break;
            }
            let slope = axis_derivative(parameter, x1, x2);
            if slope.abs() < 1e-6 {
                break;
            }
            parameter = (parameter - error / slope).clamp(0.0, 1.0);
        }
        if (axis(parameter, x1, x2) - t).abs() > 1e-4 {
            parameter = bisect(t, x1, x2);
        }
        axis(parameter, y1, y2)
    }
}

/// The cubic bezier's value on one axis at the parameter `u`.
fn axis(u: f32, first: f32, second: f32) -> f32 {
    let inverse = 1.0 - u;
    3.0 * inverse * inverse * u * first + 3.0 * inverse * u * u * second + u * u * u
}

/// The cubic bezier's slope on one axis at the parameter `u`.
fn axis_derivative(u: f32, first: f32, second: f32) -> f32 {
    let inverse = 1.0 - u;
    3.0 * inverse * inverse * first
        + 6.0 * inverse * u * (second - first)
        + 3.0 * u * u * (1.0 - second)
}

/// The bisection fallback: find the parameter whose x axis equals `target`.
fn bisect(target: f32, x1: f32, x2: f32) -> f32 {
    let mut low = 0.0;
    let mut high = 1.0;
    let mut parameter = target;
    for _ in 0..24 {
        parameter = (low + high) * 0.5;
        let value = axis(parameter, x1, x2);
        if (value - target).abs() < 1e-6 {
            break;
        }
        if value < target {
            low = parameter;
        } else {
            high = parameter;
        }
    }
    parameter
}

/// The eased progress of a transition that started at `elapsed` and lasts
/// `duration`.
///
/// A zero (or absent) duration answers the final value immediately: a
/// transition with no time budget is not an animation, it is a state change.
#[must_use]
pub(crate) fn eased_progress(
    curve: Easing,
    elapsed: std::time::Duration,
    duration: std::time::Duration,
) -> f32 {
    if duration.is_zero() {
        return 1.0;
    }
    let progress = elapsed.as_secs_f32() / duration.as_secs_f32();
    curve.evaluate(progress.clamp(0.0, 1.0))
}

#[cfg(test)]
#[path = "../../tests/headless/widgets/easing.rs"]
mod tests;
