//! Availability-preserving aggregation for process groups.
//!
//! The legacy [`super::AppGroup`] shape stores bare totals and remains
//! available for compatibility. New projections should use the helpers here:
//! an aggregate must preserve the difference between a measured zero, a
//! partial sum, stale history, and no observation at all.

use std::borrow::Borrow;

use super::super::{FailureKind, ScalarAvailability, ScalarObservation};

/// One aggregate metric plus the coverage needed to explain its state.
#[derive(Clone, Debug, PartialEq)]
pub struct AggregateMetric<T> {
    observation: ScalarObservation<T>,
    member_count: usize,
    current_member_count: usize,
    known_member_count: usize,
}

impl<T> AggregateMetric<T> {
    /// The canonical availability-bearing observation.
    #[must_use]
    pub const fn observation(&self) -> &ScalarObservation<T> {
        &self.observation
    }

    /// The aggregate's current availability.
    #[must_use]
    pub const fn availability(&self) -> ScalarAvailability {
        self.observation.availability()
    }

    /// The aggregate value only when the aggregate is current.
    #[must_use]
    pub const fn current_value(&self) -> Option<&T> {
        self.observation.current_value()
    }

    /// The aggregate's last-known value, including stale observations.
    #[must_use]
    pub const fn last_known_value(&self) -> Option<&T> {
        self.observation.last_known_value()
    }

    /// Number of members included in the fold.
    #[must_use]
    pub const fn member_count(&self) -> usize {
        self.member_count
    }

    /// Number of members contributing a current value.
    #[must_use]
    pub const fn current_member_count(&self) -> usize {
        self.current_member_count
    }

    /// Number of members with either a current or stale value.
    #[must_use]
    pub const fn known_member_count(&self) -> usize {
        self.known_member_count
    }

    /// Consume the wrapper and return its canonical observation.
    #[must_use]
    pub fn into_observation(self) -> ScalarObservation<T> {
        self.observation
    }
}

/// Fold current/stale/unavailable CPU observations in input order.
///
/// `observed_at_ms` is supplied by the owning snapshot rather than inferred
/// from member fields. An empty iterator returns `None`, so an empty category
/// cannot become a fabricated zero-valued aggregate.
#[must_use]
pub fn aggregate_f32<'a>(
    observations: impl IntoIterator<Item = &'a ScalarObservation<f32>>,
    observed_at_ms: u64,
) -> Option<AggregateMetric<f32>> {
    aggregate(observations, observed_at_ms, |left, right| left + right)
}

/// Fold current/stale/unavailable byte observations in input order with
/// saturating addition.
#[must_use]
pub fn aggregate_u64<'a>(
    observations: impl IntoIterator<Item = &'a ScalarObservation<u64>>,
    observed_at_ms: u64,
) -> Option<AggregateMetric<u64>> {
    aggregate(observations, observed_at_ms, u64::saturating_add)
}

fn aggregate<T, I, F>(
    observations: I,
    observed_at_ms: u64,
    mut add: F,
) -> Option<AggregateMetric<T>>
where
    T: Clone,
    I: IntoIterator,
    I::Item: Borrow<ScalarObservation<T>>,
    F: FnMut(T, T) -> T,
{
    let mut member_count = 0;
    let mut current_member_count = 0;
    let mut known_member_count = 0;
    let mut current_total = None;
    let mut known_total = None;
    let mut first_failure = None;
    let mut oldest_success_ms: Option<u64> = None;
    let mut all_current_available = true;
    let mut any_current = false;

    for item in observations {
        let observation = item.borrow();
        member_count += 1;

        if let Some(value) = observation.current_value() {
            any_current = true;
            current_member_count += 1;
            current_total = Some(match current_total {
                Some(total) => add(total, value.clone()),
                None => value.clone(),
            });
        }
        if let Some(value) = observation.last_known_value() {
            known_member_count += 1;
            known_total = Some(match known_total {
                Some(total) => add(total, value.clone()),
                None => value.clone(),
            });
        }

        let availability = observation.availability();
        if availability != ScalarAvailability::Available {
            all_current_available = false;
        }
        if let Some(failure) = availability.failure() {
            first_failure.get_or_insert(failure);
        }
        if let Some(success_ms) = observation.last_success_ms() {
            oldest_success_ms =
                Some(oldest_success_ms.map_or(success_ms, |oldest| oldest.min(success_ms)));
        }
    }

    if member_count == 0 {
        return None;
    }

    let observation = if all_current_available {
        // Every Available scalar carries a value by construction, so this is
        // a real zero when the sum is zero, never a missing-value fallback.
        match current_total {
            Some(total) => ScalarObservation::available(total, observed_at_ms),
            // Keep the production path fail-closed if a future observation
            // implementation violates the current-value invariant.
            None => ScalarObservation::default(),
        }
    } else if any_current {
        // Unknown coverage has no provider-specific failure. The generic
        // temporary-unavailability reason keeps the partial value honest
        // without inventing a more specific source diagnosis.
        match current_total {
            Some(total) => ScalarObservation::partial(
                total,
                observed_at_ms,
                first_failure.unwrap_or(FailureKind::TemporarilyUnavailable),
            ),
            None => ScalarObservation::default(),
        }
    } else if let Some(total) = known_total {
        ScalarObservation::stale(
            total,
            oldest_success_ms.unwrap_or(observed_at_ms),
            first_failure.unwrap_or(FailureKind::TemporarilyUnavailable),
        )
    } else if let Some(failure) = first_failure {
        ScalarObservation::unavailable(failure)
    } else {
        ScalarObservation::default()
    };

    Some(AggregateMetric {
        observation,
        member_count,
        current_member_count,
        known_member_count,
    })
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_aggregate_tests.rs"]
mod tests;
