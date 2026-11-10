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
//! The absence of an observation is NOT decided here, and this layer cannot
//! spell it: the shared mark belongs to `taskmanager-shell::presentation`,
//! above `core`, and the one-way flow forbids a core function from reading
//! upward. Every real caller already resolves absence before asking for a
//! spelling (`map_or_else(missing_value, percent_text)`), so these functions
//! are the spelling of a MEASURED value and nothing else.
//!
//! A non-finite argument is therefore a caller fault, and it is deliberately
//! left to the ordinary numeric formatting (`NaN%`) instead of being dressed
//! in a borrowed mark. Silently printing the shared dash would re-create the
//! second authority this module just gave up, and a wrong `NaN%` reaching a
//! frame is louder evidence than an indistinguishable dash.

/// The percentage ceiling a percent-typed series is projected against. Every
/// chart scale and every slider range that measures utilization against a
/// percent reads this fact instead of re-deriving `100.0` per surface.
pub const MAX_PERCENT: f32 = 100.0;

/// The percentage floor that ceiling is paired with. Percent-domain quantities
/// are never negative; a reading below zero is a measurement fault, not data.
pub const MIN_PERCENT: f32 = 0.0;

/// The percent-domain clamp: keep a reading inside [`MIN_PERCENT`]..=
/// [`MAX_PERCENT`].
///
/// Utilization, charge, wear and share are percentages, and the product's
/// invariant is that an unavailable or faulty reading is never painted as
/// progress or printed above a hundred. That is one rule; every surface used
/// to re-spell `clamp(0.0, 100.0)` and each rewrite was a chance to disagree
/// about the bounds.
///
/// Non-finite input keeps the platform `clamp` answer (infinite readings move
/// to the nearer bound, `NaN` returns unchanged): this is the boundary of the
/// percent *domain*, not an absence policy — absence is a presentation
/// decision owned above this crate, exactly as the module comment says. A caller that must not paint a non-finite reading as
/// progress filters for finiteness first, as it already does.
///
/// The bound names are spelled out per impl rather than read back through
/// `Self::MIN`/`Self::MAX`: those paths resolve to the float type's own
/// extreme-value constants, which are a different fact entirely.
pub trait ClampedPercent: Copy {
    /// Keep a percent-domain reading inside `0..=100` without changing its type.
    fn clamped_percent(self) -> Self;
}

impl ClampedPercent for f32 {
    fn clamped_percent(self) -> Self {
        f32::clamp(self, MIN_PERCENT, MAX_PERCENT)
    }
}

impl ClampedPercent for f64 {
    fn clamped_percent(self) -> Self {
        f64::clamp(self, f64::from(MIN_PERCENT), f64::from(MAX_PERCENT))
    }
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_percent_tests.rs"]
mod tests;

/// The level percent spelling: a whole percent and a `%` sign.
///
/// Takes a measured value; absence is the caller's decision, already made by
/// every real call site before reaching here.
#[must_use]
pub fn percent_text(value: impl Into<f64>) -> String {
    format!("{:.0}%", round_half_away_from_zero(value.into()))
}

/// The rate percent spelling: one decimal place and a `%` sign.
///
/// The tenth is the meaningful resolution of a rate, so it is never dropped.
/// Same measured-value-only contract as [`percent_text`].
#[must_use]
pub fn precise_percent_text(value: impl Into<f64>) -> String {
    format!(
        "{:.1}%",
        round_half_away_from_zero(value.into() * 10.0) / 10.0
    )
}

/// Round half away from zero, keeping a negative zero from printing as `-0`.
fn round_half_away_from_zero(value: f64) -> f64 {
    let rounded = value.round();
    if rounded == 0.0 { 0.0 } else { rounded }
}
