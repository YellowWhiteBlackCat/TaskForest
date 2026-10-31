//! Typed truth for independently fallible groups of scalar observations.

use serde::{Deserialize, Serialize};

use super::{ObservationWireError, ScalarAvailability, ScalarObservation};
use crate::core::FailureKind;

/// One current-refresh slot accepted by a partial observation group.
///
/// Callers provide semantic facts, not preassembled observations. The group
/// applies one timestamp to every current slot and cannot admit Unknown or
/// Stale children.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarObservationSlot<T> {
    Current(T),
    Partial(T, FailureKind),
    Unavailable(FailureKind),
}

/// One provider-neutral observation group with freshness separate from its
/// item count.
///
/// An empty `observations` vector is meaningful only when `availability` is
/// current: it then means the provider successfully observed an empty group.
/// `Unknown` is reserved for snapshots written before group truth existed,
/// while `Unavailable` and `Stale` retain their typed failure reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ScalarObservationGroupWire<T>")]
pub struct ScalarObservationGroup<T> {
    observations: Vec<ScalarObservation<T>>,
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct ScalarObservationGroupWire<T> {
    observations: Vec<ScalarObservation<T>>,
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
}

impl<T> TryFrom<ScalarObservationGroupWire<T>> for ScalarObservationGroup<T> {
    type Error = ObservationWireError;

    fn try_from(wire: ScalarObservationGroupWire<T>) -> Result<Self, Self::Error> {
        validate_group_wire(&wire.observations, wire.availability, wire.last_success_ms)?;
        Ok(Self {
            observations: wire.observations,
            availability: wire.availability,
            last_success_ms: wire.last_success_ms,
        })
    }
}

impl<T> Default for ScalarObservationGroup<T> {
    fn default() -> Self {
        Self {
            observations: Vec::new(),
            availability: ScalarAvailability::Unknown,
            last_success_ms: None,
        }
    }
}

impl<T> ScalarObservationGroup<T> {
    #[must_use]
    pub const fn availability(&self) -> ScalarAvailability {
        self.availability
    }

    #[must_use]
    pub const fn last_success_ms(&self) -> Option<u64> {
        self.last_success_ms
    }

    #[must_use]
    pub fn available(values: Vec<T>, observed_at_ms: u64) -> Self {
        Self {
            observations: values
                .into_iter()
                .map(|value| ScalarObservation::available(value, observed_at_ms))
                .collect(),
            availability: ScalarAvailability::Available,
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub fn partial(
        slots: Vec<ScalarObservationSlot<T>>,
        observed_at_ms: u64,
        failure: FailureKind,
    ) -> Self {
        Self {
            observations: slots
                .into_iter()
                .map(|slot| match slot {
                    ScalarObservationSlot::Current(value) => {
                        ScalarObservation::available(value, observed_at_ms)
                    }
                    ScalarObservationSlot::Partial(value, failure) => {
                        ScalarObservation::partial(value, observed_at_ms, failure)
                    }
                    ScalarObservationSlot::Unavailable(failure) => {
                        ScalarObservation::unavailable(failure)
                    }
                })
                .collect(),
            availability: ScalarAvailability::Partial(failure),
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn unavailable(failure: FailureKind) -> Self {
        Self {
            observations: Vec::new(),
            availability: ScalarAvailability::Unavailable(failure),
            last_success_ms: None,
        }
    }

    /// Record a failed group refresh while preserving each slot's failure
    /// reason without accepting caller-assembled observation states.
    #[must_use]
    pub fn unavailable_slots(slot_failures: Vec<FailureKind>, failure: FailureKind) -> Self {
        Self {
            observations: slot_failures
                .into_iter()
                .map(ScalarObservation::unavailable)
                .collect(),
            availability: ScalarAvailability::Unavailable(failure),
            last_success_ms: None,
        }
    }

    /// Retain prior observations as last-known data while recording why the
    /// group is no longer current.
    #[must_use]
    pub fn transition_failure(mut self, failure: FailureKind) -> Self {
        self.availability = if self.last_success_ms.is_some() {
            self.observations = self
                .observations
                .into_iter()
                .map(|observation| observation.transition_failure(failure))
                .collect();
            ScalarAvailability::Stale(failure)
        } else {
            self.observations.clear();
            ScalarAvailability::Unavailable(failure)
        };
        self
    }

    /// Merge an unavailable refresh with prior group truth. Current, partial,
    /// and confirmed-empty groups always replace the previous state.
    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        match self.availability {
            ScalarAvailability::Unavailable(failure) => previous.transition_failure(failure),
            _ => self,
        }
    }

    /// Return the current group, including `Some(&[])` for a confirmed empty
    /// observation. Unknown, stale, and unavailable groups return `None`.
    #[must_use]
    pub fn current_observations(&self) -> Option<&[ScalarObservation<T>]> {
        self.availability
            .is_current()
            .then_some(self.observations.as_slice())
    }

    #[must_use]
    pub fn last_known_observations(&self) -> &[ScalarObservation<T>] {
        &self.observations
    }

    pub(super) fn hydrate_legacy_items(self, legacy_items: Vec<ScalarObservation<T>>) -> Self {
        if self.availability != ScalarAvailability::Unknown || legacy_items.is_empty() {
            return self;
        }
        let last_success_ms = legacy_items
            .iter()
            .filter_map(ScalarObservation::last_success_ms)
            .max();
        let failure = legacy_items
            .iter()
            .find_map(|observation| observation.availability().failure());
        let any_current = legacy_items
            .iter()
            .any(|observation| observation.availability().is_current());
        let all_available = legacy_items
            .iter()
            .all(|observation| observation.availability() == ScalarAvailability::Available);
        match (all_available, any_current, last_success_ms, failure) {
            (true, _, Some(observed_at_ms), _) => Self::available(
                legacy_items
                    .into_iter()
                    .filter_map(ScalarObservation::into_last_known_value)
                    .collect(),
                observed_at_ms,
            ),
            (_, true, Some(observed_at_ms), failure) => {
                let slots = legacy_items
                    .into_iter()
                    .map(|observation| {
                        let availability = observation.availability();
                        match (availability, observation.into_last_known_value()) {
                            (ScalarAvailability::Available, Some(value)) => {
                                ScalarObservationSlot::Current(value)
                            }
                            (ScalarAvailability::Partial(failure), Some(value)) => {
                                ScalarObservationSlot::Partial(value, failure)
                            }
                            (availability, _) => ScalarObservationSlot::Unavailable(
                                availability.failure().unwrap_or(FailureKind::ProviderFault),
                            ),
                        }
                    })
                    .collect();
                Self::partial(
                    slots,
                    observed_at_ms,
                    failure.unwrap_or(FailureKind::ProviderFault),
                )
            }
            (_, false, Some(observed_at_ms), failure) => {
                let failure = failure.unwrap_or(FailureKind::ProviderFault);
                let observations = legacy_items
                    .into_iter()
                    .map(|observation| {
                        let slot_failure = observation.availability().failure().unwrap_or(failure);
                        observation.into_last_known_value().map_or_else(
                            || ScalarObservation::unavailable(slot_failure),
                            |value| ScalarObservation::stale(value, observed_at_ms, slot_failure),
                        )
                    })
                    .collect();
                Self {
                    observations,
                    availability: ScalarAvailability::Stale(failure),
                    last_success_ms: Some(observed_at_ms),
                }
            }
            (_, false, None, Some(failure)) => Self::unavailable_slots(
                legacy_items
                    .iter()
                    .map(|observation| observation.availability().failure().unwrap_or(failure))
                    .collect(),
                failure,
            ),
            _ => Self::default(),
        }
    }
}

fn validate_group_wire<T>(
    observations: &[ScalarObservation<T>],
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
) -> Result<(), ObservationWireError> {
    match availability {
        ScalarAvailability::Unknown => Ok(()),
        ScalarAvailability::Available => {
            if last_success_ms.is_none() {
                return Err(ObservationWireError::CurrentSuccessTimeMissing);
            }
            if observations
                .iter()
                .any(|observation| observation.availability() != ScalarAvailability::Available)
            {
                return Err(ObservationWireError::AvailableGroupContainsNonAvailableItem);
            }
            if observations
                .iter()
                .any(|observation| observation.last_success_ms() != last_success_ms)
            {
                return Err(ObservationWireError::GroupSuccessTimeMismatch);
            }
            Ok(())
        }
        ScalarAvailability::Partial(_) => {
            if last_success_ms.is_none() {
                return Err(ObservationWireError::CurrentSuccessTimeMissing);
            }
            if observations.iter().any(|observation| {
                matches!(
                    observation.availability(),
                    ScalarAvailability::Unknown | ScalarAvailability::Stale(_)
                )
            }) {
                return Err(ObservationWireError::PartialGroupContainsNonCurrentItem);
            }
            if observations.iter().any(|observation| {
                observation.availability().is_current()
                    && observation.last_success_ms() != last_success_ms
            }) {
                return Err(ObservationWireError::GroupSuccessTimeMismatch);
            }
            Ok(())
        }
        ScalarAvailability::Stale(_) => {
            if last_success_ms.is_none() {
                return Err(ObservationWireError::StaleHistoryMissing);
            }
            if observations.iter().any(|observation| {
                !matches!(
                    observation.availability(),
                    ScalarAvailability::Stale(_) | ScalarAvailability::Unavailable(_)
                )
            }) {
                return Err(ObservationWireError::StaleGroupContainsNonHistoricalItem);
            }
            Ok(())
        }
        ScalarAvailability::Unavailable(_) => {
            if last_success_ms.is_some() {
                return Err(ObservationWireError::UnavailableCarriesSuccessTime);
            }
            if observations.iter().any(|observation| {
                observation.last_known_value().is_some()
                    || observation.last_success_ms().is_some()
                    || !matches!(
                        observation.availability(),
                        ScalarAvailability::Unknown | ScalarAvailability::Unavailable(_)
                    )
            }) {
                return Err(ObservationWireError::UnavailableGroupContainsHistory);
            }
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/core_core_metrics_availability_group_tests.rs"]
mod tests;
