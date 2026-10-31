//! Typed availability for independently fallible scalar measurements.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::core::FailureKind;

mod group;
pub use group::{ScalarObservationGroup, ScalarObservationSlot};

/// Whether one scalar is current, partial, retained from a prior success, or
/// unavailable.
///
/// The failure is part of the state rather than a provider-formatted string.
/// `Stale` means the value is last-known data and must not be presented as a
/// current observation. `Partial` means the value was observed in the current
/// refresh but does not cover every native source that contributes to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "status", content = "failure", rename_all = "snake_case")]
pub enum ScalarAvailability {
    /// Compatibility state for snapshots written before typed availability.
    #[default]
    Unknown,
    Available,
    Partial(FailureKind),
    Stale(FailureKind),
    Unavailable(FailureKind),
}

impl ScalarAvailability {
    #[must_use]
    pub const fn is_current(self) -> bool {
        matches!(self, Self::Available | Self::Partial(_))
    }

    #[must_use]
    pub const fn failure(self) -> Option<FailureKind> {
        match self {
            Self::Partial(failure) | Self::Stale(failure) | Self::Unavailable(failure) => {
                Some(failure)
            }
            Self::Unknown | Self::Available => None,
        }
    }
}

/// Stable reasons why a typed observation wire payload is internally
/// contradictory.
///
/// Constructors already create valid states. This error is primarily exposed
/// by deserialization so an untrusted snapshot cannot claim current or stale
/// truth without the value/state and success timestamp that make that claim
/// meaningful.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationWireError {
    CurrentValueMissing,
    CurrentStateUnknown,
    CurrentSuccessTimeMissing,
    AbsentCarriesValue,
    StaleHistoryMissing,
    UnavailableCarriesValue,
    UnavailableCarriesState,
    UnavailableCarriesSuccessTime,
    AvailableGroupContainsNonAvailableItem,
    PartialGroupContainsNonCurrentItem,
    GroupSuccessTimeMismatch,
    StaleGroupContainsNonHistoricalItem,
    UnavailableGroupContainsHistory,
}

impl fmt::Display for ObservationWireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CurrentValueMissing => "current observation is missing its value",
            Self::CurrentStateUnknown => "current observation has an unknown semantic state",
            Self::CurrentSuccessTimeMissing => {
                "current observation is missing its last-success time"
            }
            Self::AbsentCarriesValue => "confirmed-absent observation carries a value",
            Self::StaleHistoryMissing => "stale observation has no trustworthy history",
            Self::UnavailableCarriesValue => "unavailable observation carries a value",
            Self::UnavailableCarriesState => "unavailable observation carries a semantic state",
            Self::UnavailableCarriesSuccessTime => {
                "unavailable observation carries a last-success time"
            }
            Self::AvailableGroupContainsNonAvailableItem => {
                "available observation group contains a non-available item"
            }
            Self::PartialGroupContainsNonCurrentItem => {
                "partial observation group contains an unknown or stale item"
            }
            Self::GroupSuccessTimeMismatch => {
                "observation group and current slot success times differ"
            }
            Self::StaleGroupContainsNonHistoricalItem => {
                "stale observation group contains an item without stale or unavailable truth"
            }
            Self::UnavailableGroupContainsHistory => {
                "unavailable observation group carries trustworthy item history"
            }
        })
    }
}

impl std::error::Error for ObservationWireError {}

/// One provider-neutral scalar value with freshness and last-success truth.
///
/// A retained value may coexist with `Stale`, but [`current_value`](Self::current_value)
/// deliberately hides it. Consumers must opt into
/// [`last_known_value`](Self::last_known_value) when drawing history or
/// diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ScalarObservationWire<T>")]
pub struct ScalarObservation<T> {
    value: Option<T>,
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
struct ScalarObservationWire<T> {
    value: Option<T>,
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
}

impl<T> TryFrom<ScalarObservationWire<T>> for ScalarObservation<T> {
    type Error = ObservationWireError;

    fn try_from(wire: ScalarObservationWire<T>) -> Result<Self, Self::Error> {
        validate_scalar_wire(wire.value.as_ref(), wire.availability, wire.last_success_ms)?;
        Ok(Self {
            value: wire.value,
            availability: wire.availability,
            last_success_ms: wire.last_success_ms,
        })
    }
}

/// Semantic state of an optional provider field, independent of freshness.
///
/// This is deliberately not represented as `ScalarObservation<Option<T>>`.
/// Nested `Option` values collapse during Serde decoding and cannot preserve
/// the difference between an unknown field, a confirmed absence, and a field
/// that does not apply to the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "state", content = "value", rename_all = "snake_case")]
pub enum OptionalObservationState<T> {
    /// Compatibility state for snapshots written before typed optional
    /// observations.
    #[default]
    Unknown,
    /// The provider observed a value.
    Present(T),
    /// The provider confirmed that the optional value is currently absent.
    Absent,
    /// The field has no meaning for this device class.
    NotApplicable,
}

/// One optional value with orthogonal semantic state and freshness.
///
/// `state` answers whether a value is present, absent, or not applicable.
/// `availability` answers whether that state is current, partial, stale, or
/// unavailable. Keeping these axes separate means a failed refresh can retain
/// a previously confirmed `Absent` or `NotApplicable` state without inventing
/// a nested optional value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "OptionalObservationWire<T>")]
pub struct OptionalObservation<T> {
    state: OptionalObservationState<T>,
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct OptionalObservationWire<T> {
    state: OptionalObservationState<T>,
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
}

impl<T> TryFrom<OptionalObservationWire<T>> for OptionalObservation<T> {
    type Error = ObservationWireError;

    fn try_from(wire: OptionalObservationWire<T>) -> Result<Self, Self::Error> {
        validate_optional_wire(&wire.state, wire.availability, wire.last_success_ms)?;
        Ok(Self {
            state: wire.state,
            availability: wire.availability,
            last_success_ms: wire.last_success_ms,
        })
    }
}

impl<T> Default for OptionalObservation<T> {
    fn default() -> Self {
        Self {
            state: OptionalObservationState::Unknown,
            availability: ScalarAvailability::Unknown,
            last_success_ms: None,
        }
    }
}

impl<T> OptionalObservation<T> {
    #[must_use]
    pub const fn availability(&self) -> ScalarAvailability {
        self.availability
    }

    #[must_use]
    pub const fn last_success_ms(&self) -> Option<u64> {
        self.last_success_ms
    }

    #[must_use]
    pub fn into_last_known_state(self) -> OptionalObservationState<T> {
        self.state
    }

    #[must_use]
    pub const fn present(value: T, observed_at_ms: u64) -> Self {
        Self {
            state: OptionalObservationState::Present(value),
            availability: ScalarAvailability::Available,
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn absent(observed_at_ms: u64) -> Self {
        Self {
            state: OptionalObservationState::Absent,
            availability: ScalarAvailability::Available,
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn not_applicable(observed_at_ms: u64) -> Self {
        Self {
            state: OptionalObservationState::NotApplicable,
            availability: ScalarAvailability::Available,
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn partial_present(value: T, observed_at_ms: u64, failure: FailureKind) -> Self {
        Self {
            state: OptionalObservationState::Present(value),
            availability: ScalarAvailability::Partial(failure),
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn partial_absent(observed_at_ms: u64, failure: FailureKind) -> Self {
        Self {
            state: OptionalObservationState::Absent,
            availability: ScalarAvailability::Partial(failure),
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn partial_not_applicable(observed_at_ms: u64, failure: FailureKind) -> Self {
        Self {
            state: OptionalObservationState::NotApplicable,
            availability: ScalarAvailability::Partial(failure),
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn unavailable(failure: FailureKind) -> Self {
        Self {
            state: OptionalObservationState::Unknown,
            availability: ScalarAvailability::Unavailable(failure),
            last_success_ms: None,
        }
    }

    /// Retain the last trustworthy semantic state while recording why it is
    /// no longer current.
    #[must_use]
    pub fn transition_failure(mut self, failure: FailureKind) -> Self {
        self.availability = if !matches!(self.state, OptionalObservationState::Unknown)
            && self.last_success_ms.is_some()
        {
            ScalarAvailability::Stale(failure)
        } else {
            self.state = OptionalObservationState::Unknown;
            ScalarAvailability::Unavailable(failure)
        };
        self
    }

    /// Merge an unavailable refresh with the prior value/state. Current,
    /// partial, confirmed-absent, and not-applicable observations always win.
    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        match self.availability {
            ScalarAvailability::Unavailable(failure) => previous.transition_failure(failure),
            _ => self,
        }
    }

    #[must_use]
    pub const fn current_value(&self) -> Option<&T> {
        if !self.availability.is_current() {
            return None;
        }
        match &self.state {
            OptionalObservationState::Present(value) => Some(value),
            OptionalObservationState::Unknown
            | OptionalObservationState::Absent
            | OptionalObservationState::NotApplicable => None,
        }
    }

    #[must_use]
    pub const fn is_current_absent(&self) -> bool {
        self.availability.is_current() && matches!(self.state, OptionalObservationState::Absent)
    }

    #[must_use]
    pub const fn is_current_not_applicable(&self) -> bool {
        self.availability.is_current()
            && matches!(self.state, OptionalObservationState::NotApplicable)
    }

    #[must_use]
    pub const fn last_known_state(&self) -> &OptionalObservationState<T> {
        &self.state
    }
}

impl<T> Default for ScalarObservation<T> {
    fn default() -> Self {
        Self {
            value: None,
            availability: ScalarAvailability::Unknown,
            last_success_ms: None,
        }
    }
}

impl<T> ScalarObservation<T> {
    #[must_use]
    pub const fn availability(&self) -> ScalarAvailability {
        self.availability
    }

    #[must_use]
    pub const fn last_success_ms(&self) -> Option<u64> {
        self.last_success_ms
    }

    #[must_use]
    pub fn into_last_known_value(self) -> Option<T> {
        self.value
    }

    #[must_use]
    pub const fn available(value: T, observed_at_ms: u64) -> Self {
        Self {
            value: Some(value),
            availability: ScalarAvailability::Available,
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn partial(value: T, observed_at_ms: u64, failure: FailureKind) -> Self {
        Self {
            value: Some(value),
            availability: ScalarAvailability::Partial(failure),
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn stale(value: T, last_success_ms: u64, failure: FailureKind) -> Self {
        Self {
            value: Some(value),
            availability: ScalarAvailability::Stale(failure),
            last_success_ms: Some(last_success_ms),
        }
    }

    #[must_use]
    pub const fn unavailable(failure: FailureKind) -> Self {
        Self {
            value: None,
            availability: ScalarAvailability::Unavailable(failure),
            last_success_ms: None,
        }
    }

    /// Retain the last trustworthy value while recording why it is no longer
    /// current. An observation without prior success remains unavailable.
    #[must_use]
    pub fn transition_failure(mut self, failure: FailureKind) -> Self {
        self.availability = if self.value.is_some() && self.last_success_ms.is_some() {
            ScalarAvailability::Stale(failure)
        } else {
            self.value = None;
            ScalarAvailability::Unavailable(failure)
        };
        self
    }

    /// Merge a new unavailable observation with prior trustworthy data. A
    /// current or partial observation always wins and advances its own success
    /// time.
    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        match self.availability {
            ScalarAvailability::Unavailable(failure) => previous.transition_failure(failure),
            _ => self,
        }
    }

    #[must_use]
    pub const fn current_value(&self) -> Option<&T> {
        if self.availability.is_current() {
            self.value.as_ref()
        } else {
            None
        }
    }

    #[must_use]
    pub const fn last_known_value(&self) -> Option<&T> {
        self.value.as_ref()
    }
}

pub(super) fn hydrate_legacy_group<T>(
    group: ScalarObservationGroup<T>,
    legacy_items: Vec<ScalarObservation<T>>,
) -> ScalarObservationGroup<T> {
    group.hydrate_legacy_items(legacy_items)
}

fn validate_scalar_wire<T>(
    value: Option<&T>,
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
) -> Result<(), ObservationWireError> {
    match availability {
        // Unknown is the explicit schema-v1 compatibility state. It never
        // becomes current through the typed accessor, so legacy payload
        // details may survive until their owning DTO applies its compatibility
        // policy.
        ScalarAvailability::Unknown => Ok(()),
        ScalarAvailability::Available | ScalarAvailability::Partial(_) => {
            if value.is_none() {
                return Err(ObservationWireError::CurrentValueMissing);
            }
            if last_success_ms.is_none() {
                return Err(ObservationWireError::CurrentSuccessTimeMissing);
            }
            Ok(())
        }
        ScalarAvailability::Stale(_) => {
            if value.is_none() || last_success_ms.is_none() {
                Err(ObservationWireError::StaleHistoryMissing)
            } else {
                Ok(())
            }
        }
        ScalarAvailability::Unavailable(_) => {
            if value.is_some() {
                return Err(ObservationWireError::UnavailableCarriesValue);
            }
            if last_success_ms.is_some() {
                return Err(ObservationWireError::UnavailableCarriesSuccessTime);
            }
            Ok(())
        }
    }
}

fn validate_optional_wire<T>(
    state: &OptionalObservationState<T>,
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
) -> Result<(), ObservationWireError> {
    match availability {
        ScalarAvailability::Unknown => Ok(()),
        ScalarAvailability::Available | ScalarAvailability::Partial(_) => {
            if matches!(state, OptionalObservationState::Unknown) {
                return Err(ObservationWireError::CurrentStateUnknown);
            }
            if last_success_ms.is_none() {
                return Err(ObservationWireError::CurrentSuccessTimeMissing);
            }
            Ok(())
        }
        ScalarAvailability::Stale(_) => {
            if matches!(state, OptionalObservationState::Unknown) || last_success_ms.is_none() {
                Err(ObservationWireError::StaleHistoryMissing)
            } else {
                Ok(())
            }
        }
        ScalarAvailability::Unavailable(_) => {
            if !matches!(state, OptionalObservationState::Unknown) {
                return Err(ObservationWireError::UnavailableCarriesState);
            }
            if last_success_ms.is_some() {
                return Err(ObservationWireError::UnavailableCarriesSuccessTime);
            }
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_availability_tests.rs"]
mod tests;
