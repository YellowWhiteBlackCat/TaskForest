//! One accepted metric sample together with the instant it was measured at.
//!
//! The shared trend vocabulary needs the sampling instant, not a sample
//! offset: a hover pill must read the real moment a value was observed.
//! This module owns that pairing so the telemetry store, the shell and every
//! frontend agree on one shape and one meaning.

/// One accepted live-graph sample: the measured value and the instant it was
/// measured at.
///
/// The instants are the accepted observation's own timestamps — the
/// correlated history carries `measured_at_ms` (the sampling time) and
/// `completed_at_ms` (the wall-clock completion of the accepted event). The
/// read layer never reads a clock of its own, and a sample whose observation
/// carried no measurement time carries no value either: the gap stays
/// explicit instead of becoming a zero or a synthetic instant.
///
/// `completed_at_ms` orders samples whose measurement instants are equal or
/// absent. It is the acceptance time, not the sampling instant, and a
/// consumer that shows a sampling time must read
/// [`measured_at_ms`](Self::measured_at_ms).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricSample {
    value: Option<f32>,
    measured_at_ms: Option<u64>,
    completed_at_ms: u64,
}

impl MetricSample {
    /// One measured sample: the value and the instant it was measured at.
    #[must_use]
    pub const fn measured(value: f32, measured_at_ms: u64, completed_at_ms: u64) -> Self {
        Self {
            value: Some(value),
            measured_at_ms: Some(measured_at_ms),
            completed_at_ms,
        }
    }

    /// One gap: the accepted event carried no measurement to show.
    #[must_use]
    pub const fn gap(completed_at_ms: u64) -> Self {
        Self {
            value: None,
            measured_at_ms: None,
            completed_at_ms,
        }
    }

    /// The measured value, or `None` when the observation carried no
    /// measurement.
    #[must_use]
    pub const fn value(&self) -> Option<f32> {
        self.value
    }

    /// The instant the value was measured at, in milliseconds since the Unix
    /// epoch. `None` for a gap.
    #[must_use]
    pub const fn measured_at_ms(&self) -> Option<u64> {
        self.measured_at_ms
    }

    /// The wall-clock completion time of the accepted correlated event, in
    /// milliseconds since the Unix epoch.
    #[must_use]
    pub const fn completed_at_ms(&self) -> u64 {
        self.completed_at_ms
    }

    /// Whether this sample records the absence of a measurement.
    #[must_use]
    pub const fn is_gap(&self) -> bool {
        self.value.is_none()
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_sample_tests.rs"]
mod tests;
