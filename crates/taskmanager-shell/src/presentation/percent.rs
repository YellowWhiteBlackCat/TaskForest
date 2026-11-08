//! Percent and scalar readouts shared by every frontend.
//!
//! A percentage is the same fact everywhere: the TUI Performance page, the
//! Iced device cards, the Bevy Performance page, and the GPUI dashboards must
//! not disagree about how `63.25%` reads, and a missing observation must read
//! the same way in all of them.
//!
//! The SPELLINGS live in the shared floor (`taskforest_core::core::percent`):
//! a level reads as a whole percent, a rate keeps one decimal. This module
//! owns the ABSENCE-aware readouts — the ones that turn an unobserved or
//! non-finite measurement into the shared dash instead of a fabricated zero.

use crate::presentation::missing_value;
use taskforest_core::core::percent::{percent_text, precise_percent_text};

/// Level percent readout: a whole percent, or the shared dash.
///
/// This is the SINGLE absence-aware level-percent spelling (ADR-020). A
/// missing observation and a non-finite observation both render the shared
/// dash — a fabricated `0%` would claim the host measured an idle device it
/// never saw.
#[must_use]
pub fn observed_percentage<T: Into<f64>>(value: Option<T>) -> String {
    value
        .map(Into::into)
        .filter(|value| value.is_finite())
        .map_or_else(missing_value, percent_text)
}

/// Precise percent readout: one decimal place, or the shared dash.
///
/// The same absence rules as [`observed_percentage`]: an unavailable rate
/// reads the shared dash, never a fabricated `0.0%`.
#[must_use]
pub fn observed_precise_percentage<T: Into<f64>>(value: Option<T>) -> String {
    value
        .map(Into::into)
        .filter(|value| value.is_finite())
        .map_or_else(missing_value, precise_percent_text)
}

/// Scalar readout with an explicit suffix: one decimal place plus `suffix`.
///
/// This is the SINGLE suffixed-scalar spelling (ADR-020) for the scalars that
/// are not percentages (watts, hertz, a byte rate). A missing or non-finite
/// observation renders the shared dash, never a fabricated `0.0`.
#[must_use]
pub fn scalar_text(value: Option<f64>, suffix: &str) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(missing_value, |value| format!("{value:.1}{suffix}"))
}

#[cfg(test)]
#[path = "../../tests/headless/presentation/percent_tests.rs"]
mod tests;
