//! Typed availability for independently fallible process metadata.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::{FailureKind, ObservationWireError};

/// A platform-neutral process-owner identity token.
///
/// Unix adapters can publish a numeric UID without teaching consumers about
/// passwd files. Platforms with opaque identities (for example, account SIDs)
/// retain their native stable token as a string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ProcessOwnerIdentity {
    Numeric(u64),
    Opaque(String),
}

impl ProcessOwnerIdentity {
    #[must_use]
    pub fn display_value(&self) -> String {
        match self {
            Self::Numeric(value) => value.to_string(),
            Self::Opaque(value) => value.clone(),
        }
    }
}

/// Current owner identity and its independently optional friendly label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessOwner {
    pub identity: ProcessOwnerIdentity,
    pub label: Option<String>,
}

impl ProcessOwner {
    #[must_use]
    pub fn opaque(value: impl Into<String>) -> Self {
        Self {
            identity: ProcessOwnerIdentity::Opaque(value.into()),
            label: None,
        }
    }

    #[must_use]
    pub fn display_value(&self) -> String {
        self.label
            .clone()
            .unwrap_or_else(|| self.identity.display_value())
    }
}

/// Stable reasons why one process metadata field is not fully current.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessMetadataFailure {
    Unsupported,
    PermissionDenied,
    NotFound,
    PidRace,
    ProviderFault,
}

impl ProcessMetadataFailure {
    #[must_use]
    pub const fn from_inventory_failure(failure: FailureKind) -> Self {
        match failure {
            FailureKind::Unsupported => Self::Unsupported,
            // RequiresEscalation is an escalatable denial; the metadata-level
            // vocabulary has no escalation token, so fold it into PermissionDenied.
            FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
                Self::PermissionDenied
            }
            FailureKind::MissingDependency => Self::NotFound,
            FailureKind::IdentityChanged | FailureKind::TemporarilyUnavailable => Self::PidRace,
            FailureKind::TimedOut | FailureKind::Rejected | FailureKind::ProviderFault => {
                Self::ProviderFault
            }
        }
    }
}

/// Freshness and failure truth for one process metadata value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "status", content = "failure", rename_all = "snake_case")]
pub enum ProcessMetadataAvailability {
    /// Compatibility state for snapshots written before typed metadata.
    #[default]
    Unknown,
    Available,
    Partial(ProcessMetadataFailure),
    /// A successful current observation that proves the value does not exist.
    Absent,
    Stale(ProcessMetadataFailure),
    Unavailable(ProcessMetadataFailure),
}

impl ProcessMetadataAvailability {
    #[must_use]
    pub const fn is_current(self) -> bool {
        matches!(self, Self::Available | Self::Partial(_) | Self::Absent)
    }

    #[must_use]
    pub const fn has_current_value(self) -> bool {
        matches!(self, Self::Available | Self::Partial(_))
    }
}

/// One metadata value with explicit absence, freshness, and last-success time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ProcessMetadataObservationWire<T>")]
pub struct ProcessMetadataObservation<T> {
    value: Option<T>,
    availability: ProcessMetadataAvailability,
    last_success_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ProcessMetadataObservationWire<T> {
    value: Option<T>,
    availability: ProcessMetadataAvailability,
    last_success_ms: Option<u64>,
}

impl<T> TryFrom<ProcessMetadataObservationWire<T>> for ProcessMetadataObservation<T> {
    type Error = ObservationWireError;

    fn try_from(wire: ProcessMetadataObservationWire<T>) -> Result<Self, Self::Error> {
        validate_metadata_wire(wire.value.as_ref(), wire.availability, wire.last_success_ms)?;
        Ok(Self {
            value: wire.value,
            availability: wire.availability,
            last_success_ms: wire.last_success_ms,
        })
    }
}

impl<T> Default for ProcessMetadataObservation<T> {
    fn default() -> Self {
        Self {
            value: None,
            availability: ProcessMetadataAvailability::Unknown,
            last_success_ms: None,
        }
    }
}

impl<T> ProcessMetadataObservation<T> {
    #[must_use]
    pub const fn availability(&self) -> ProcessMetadataAvailability {
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
            availability: ProcessMetadataAvailability::Available,
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn partial(value: T, observed_at_ms: u64, failure: ProcessMetadataFailure) -> Self {
        Self {
            value: Some(value),
            availability: ProcessMetadataAvailability::Partial(failure),
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn absent(observed_at_ms: u64) -> Self {
        Self {
            value: None,
            availability: ProcessMetadataAvailability::Absent,
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn unavailable(failure: ProcessMetadataFailure) -> Self {
        Self {
            value: None,
            availability: ProcessMetadataAvailability::Unavailable(failure),
            last_success_ms: None,
        }
    }

    #[must_use]
    pub fn transition_failure(mut self, failure: ProcessMetadataFailure) -> Self {
        self.availability = if self.last_success_ms.is_some()
            && self.availability != ProcessMetadataAvailability::Unknown
        {
            ProcessMetadataAvailability::Stale(failure)
        } else {
            self.value = None;
            ProcessMetadataAvailability::Unavailable(failure)
        };
        self
    }

    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        match self.availability {
            ProcessMetadataAvailability::Unavailable(failure) => {
                previous.transition_failure(failure)
            }
            _ => self,
        }
    }

    #[must_use]
    pub const fn current_value(&self) -> Option<&T> {
        if self.availability.has_current_value() {
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

fn validate_metadata_wire<T>(
    value: Option<&T>,
    availability: ProcessMetadataAvailability,
    last_success_ms: Option<u64>,
) -> Result<(), ObservationWireError> {
    match availability {
        ProcessMetadataAvailability::Unknown => Ok(()),
        ProcessMetadataAvailability::Available | ProcessMetadataAvailability::Partial(_) => {
            if value.is_none() {
                return Err(ObservationWireError::CurrentValueMissing);
            }
            if last_success_ms.is_none() {
                return Err(ObservationWireError::CurrentSuccessTimeMissing);
            }
            Ok(())
        }
        ProcessMetadataAvailability::Absent => {
            if value.is_some() {
                return Err(ObservationWireError::AbsentCarriesValue);
            }
            if last_success_ms.is_none() {
                return Err(ObservationWireError::CurrentSuccessTimeMissing);
            }
            Ok(())
        }
        // Stale metadata may retain either a prior value or a prior confirmed
        // absence. This older contract has no separate semantic-state axis, so
        // the success timestamp is the authority for both histories.
        ProcessMetadataAvailability::Stale(_) => {
            if last_success_ms.is_none() {
                Err(ObservationWireError::StaleHistoryMissing)
            } else {
                Ok(())
            }
        }
        ProcessMetadataAvailability::Unavailable(_) => {
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

/// Typed observations behind the legacy process owner and executable fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProcessMetadataObservations {
    pub owner: ProcessMetadataObservation<ProcessOwner>,
    pub executable_path: ProcessMetadataObservation<PathBuf>,
}

impl ProcessMetadataObservations {
    #[must_use]
    pub fn current(
        owner: ProcessOwner,
        executable_path: Option<PathBuf>,
        observed_at_ms: u64,
    ) -> Self {
        Self {
            owner: ProcessMetadataObservation::available(owner, observed_at_ms),
            executable_path: executable_path.map_or_else(
                || ProcessMetadataObservation::absent(observed_at_ms),
                |path| ProcessMetadataObservation::available(path, observed_at_ms),
            ),
        }
    }

    #[must_use]
    pub fn transition_failure(self, failure: ProcessMetadataFailure) -> Self {
        Self {
            owner: self.owner.transition_failure(failure),
            executable_path: self.executable_path.transition_failure(failure),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_metadata_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_metadata_metadata_predicate_tests.rs"]
mod metadata_predicate_tests;
