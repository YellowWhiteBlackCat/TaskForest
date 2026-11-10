//! Memory readout rules shared by every frontend.
//!
//! The used-rate row deliberately hides changes the host cannot distinguish
//! from zero. All four shapes ask the same question before painting
//! `mem.usage_rate`, so the floor and its predicate live here rather than in
//! a renderer.

/// Absolute used-rate floor, in MiB/s, below which the row reports absence.
pub const MEMORY_USAGE_RATE_NOISE_FLOOR_MIB_PER_SEC: f32 = 0.05;

/// Whether a measured memory used-rate is large enough to paint as a rate.
/// Non-finite observations are absent, never fabricated as zero.
#[must_use]
pub fn memory_usage_rate_is_visible(rate: f32) -> bool {
    rate.is_finite() && rate.abs() >= MEMORY_USAGE_RATE_NOISE_FLOOR_MIB_PER_SEC
}

#[cfg(test)]
#[path = "../../tests/headless/presentation/memory_tests.rs"]
mod tests;
