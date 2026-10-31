//! Pure cumulative-counter transition and rate rules.

use super::ScalarObservation;
use crate::core::FailureKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CounterPoint {
    value: u64,
    observed_at_ms: u64,
}

/// Delta produced by two cumulative samples, or the typed reason for a gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterDelta {
    Available { value: u64, elapsed_ms: u64 },
    Unavailable(FailureKind),
}

impl CounterDelta {
    /// Convert the delta into an integer per-second rate without overflow.
    #[must_use]
    pub fn per_second(self, published_at_ms: u64) -> ScalarObservation<u64> {
        let Self::Available { value, elapsed_ms } = self else {
            return ScalarObservation::unavailable(self.failure());
        };
        u128::from(value)
            .checked_mul(1_000)
            .and_then(|value| value.checked_div(u128::from(elapsed_ms)))
            .and_then(|value| u64::try_from(value).ok())
            .map_or_else(
                || ScalarObservation::unavailable(FailureKind::ProviderFault),
                |value| ScalarObservation::available(value, published_at_ms),
            )
    }

    #[must_use]
    pub const fn failure(self) -> FailureKind {
        match self {
            Self::Unavailable(failure) => failure,
            Self::Available { .. } => FailureKind::ProviderFault,
        }
    }
}

/// Baseline for one cumulative counter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CumulativeCounter {
    previous: Option<CounterPoint>,
}

impl CumulativeCounter {
    /// Observe one counter value and replace the baseline with this sample.
    ///
    /// The first sample returns `initial_gap`; an upstream failure clears the
    /// baseline; a zero-duration window is a temporary gap; clock or counter
    /// rollback is an identity change. Equal counters yield a measured zero.
    pub fn observe(
        &mut self,
        current: Result<u64, FailureKind>,
        observed_at_ms: u64,
        initial_gap: FailureKind,
    ) -> CounterDelta {
        let current = match current {
            Ok(value) => value,
            Err(failure) => {
                self.previous = None;
                return CounterDelta::Unavailable(failure);
            }
        };
        let current_point = CounterPoint {
            value: current,
            observed_at_ms,
        };
        let Some(previous) = self.previous.replace(current_point) else {
            return CounterDelta::Unavailable(initial_gap);
        };
        let elapsed_ms = match observed_at_ms.checked_sub(previous.observed_at_ms) {
            Some(0) => return CounterDelta::Unavailable(FailureKind::TemporarilyUnavailable),
            Some(elapsed_ms) => elapsed_ms,
            None => return CounterDelta::Unavailable(FailureKind::IdentityChanged),
        };
        current.checked_sub(previous.value).map_or(
            CounterDelta::Unavailable(FailureKind::IdentityChanged),
            |value| CounterDelta::Available { value, elapsed_ms },
        )
    }

    /// Forget the baseline after an adapter-specific identity boundary changes.
    pub const fn reset(&mut self) {
        self.previous = None;
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_counter_tests.rs"]
mod tests;
