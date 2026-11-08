//! Neutral percent formatting — the single source for how the product spells
//! a percentage.
//!
//! Pure functions over plain data: no I/O, no toolkit types, no absence
//! convention. The output spec is the ladder every surface already renders:
//!
//! * a **level** — a measured magnitude on a fixed 0..=100 scale (a device's
//!   utilization, a battery's charge, a disk's spare capacity) — reads as a
//!   whole percent (`42%`). It is the industry spelling and the one the
//!   release surface already paints.
//! * a **rate** — a value whose tenth of a point carries the fact (a process's
//!   single-core-equivalent CPU rate, a pressure average over a window) —
//!   reads with one decimal (`13.0%`, `0.5%`), so `13.4%` and `13.6%` stay
//!   apart.
//!
//! The absence of an observation is NOT decided here: a surface that renders
//! an unobserved reading uses its own missing-value convention (the shared
//! dash in the presentation layer, a localized word in a labelled row).
//! `taskmanager-shell::presentation` owns the absence-aware readouts that
//! combine these spellings with that convention.

/// The shared absence mark for a reading that has no measurement.
pub const MISSING_PERCENT: &str = "—";

/// The percentage ceiling a percent-typed series is projected against. Every
/// chart scale and every slider range that measures utilization against a
/// percent reads this fact instead of re-deriving `100.0` per surface.
pub const MAX_PERCENT: f32 = 100.0;

#[cfg(test)]
#[path = "../../tests/headless/core_core_percent_tests.rs"]
mod tests;

/// The level percent spelling: a whole percent and a `%` sign.
///
/// A non-finite value reads as the shared absence mark, because a percent
/// with no measurement has no honest spelling.
#[must_use]
pub fn percent_text(value: impl Into<f64>) -> String {
    let value = value.into();
    if value.is_finite() {
        format!("{:.0}%", round_half_away_from_zero(value))
    } else {
        MISSING_PERCENT.to_owned()
    }
}

/// The rate percent spelling: one decimal place and a `%` sign.
///
/// The tenth is the meaningful resolution of a rate, so it is never dropped.
#[must_use]
pub fn precise_percent_text(value: impl Into<f64>) -> String {
    let value = value.into();
    if value.is_finite() {
        format!("{:.1}%", round_half_away_from_zero(value * 10.0) / 10.0)
    } else {
        MISSING_PERCENT.to_owned()
    }
}

/// Round half away from zero, keeping a negative zero from printing as `-0`.
fn round_half_away_from_zero(value: f64) -> f64 {
    let rounded = value.round();
    if rounded == 0.0 { 0.0 } else { rounded }
}
