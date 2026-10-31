//! Validated sensor magnitudes, availability, and retention policy.

use serde::{Deserialize, Serialize};

use super::{SensorDescriptor, SensorModelError, SensorQuantity, SensorScale, SensorUnit};
use crate::core::FailureKind;
use crate::core::metrics::ScalarAvailability;

const ABSOLUTE_ZERO_C: f64 = -273.15;

/// Raw value shape. Fixed-point numeric values retain their integer magnitude
/// and exact scale; boolean and PWM channels do not masquerade as SI floats.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SensorMagnitude {
    Signed(i64),
    Unsigned(u64),
    Decimal(f64),
    Boolean(bool),
    DutyCycle { value: u32, maximum: u32 },
}

/// One validated sensor channel observation.
///
/// Descriptor metadata survives `Unavailable`, while a retained value is only
/// exposed through `last_known_value` after it becomes `Stale`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SensorMeasurementObservationWire")]
pub struct SensorMeasurementObservation {
    descriptor: SensorDescriptor,
    value: Option<SensorMagnitude>,
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct SensorMeasurementObservationWire {
    descriptor: SensorDescriptor,
    value: Option<SensorMagnitude>,
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
}

impl TryFrom<SensorMeasurementObservationWire> for SensorMeasurementObservation {
    type Error = SensorModelError;

    fn try_from(wire: SensorMeasurementObservationWire) -> Result<Self, Self::Error> {
        Self::try_from_parts(
            wire.descriptor,
            wire.value,
            wire.availability,
            wire.last_success_ms,
        )
    }
}

impl Default for SensorMeasurementObservation {
    fn default() -> Self {
        Self {
            descriptor: SensorDescriptor::default(),
            value: None,
            availability: ScalarAvailability::Unknown,
            last_success_ms: None,
        }
    }
}

impl SensorMeasurementObservation {
    pub fn available(
        descriptor: SensorDescriptor,
        value: SensorMagnitude,
        observed_at_ms: u64,
    ) -> Result<Self, SensorModelError> {
        Self::try_from_parts(
            descriptor,
            Some(value),
            ScalarAvailability::Available,
            Some(observed_at_ms),
        )
    }

    pub fn partial(
        descriptor: SensorDescriptor,
        value: SensorMagnitude,
        observed_at_ms: u64,
        failure: FailureKind,
    ) -> Result<Self, SensorModelError> {
        Self::try_from_parts(
            descriptor,
            Some(value),
            ScalarAvailability::Partial(failure),
            Some(observed_at_ms),
        )
    }

    #[must_use]
    pub fn unavailable(descriptor: SensorDescriptor, failure: FailureKind) -> Self {
        Self {
            descriptor,
            value: None,
            availability: ScalarAvailability::Unavailable(failure),
            last_success_ms: None,
        }
    }

    pub fn try_from_parts(
        descriptor: SensorDescriptor,
        value: Option<SensorMagnitude>,
        availability: ScalarAvailability,
        last_success_ms: Option<u64>,
    ) -> Result<Self, SensorModelError> {
        validate_observation(&descriptor, value.as_ref(), availability, last_success_ms)?;
        Ok(Self {
            descriptor,
            value,
            availability,
            last_success_ms,
        })
    }

    #[must_use]
    pub const fn descriptor(&self) -> &SensorDescriptor {
        &self.descriptor
    }

    #[must_use]
    pub const fn quantity(&self) -> &SensorQuantity {
        self.descriptor.quantity()
    }

    #[must_use]
    pub const fn unit(&self) -> &SensorUnit {
        self.descriptor.unit()
    }

    #[must_use]
    pub const fn source_scale(&self) -> Option<SensorScale> {
        self.descriptor.source_scale()
    }

    #[must_use]
    pub const fn availability(&self) -> ScalarAvailability {
        self.availability
    }

    #[must_use]
    pub const fn failure(&self) -> Option<FailureKind> {
        self.availability.failure()
    }

    #[must_use]
    pub const fn last_success_ms(&self) -> Option<u64> {
        self.last_success_ms
    }

    #[must_use]
    pub const fn current_value(&self) -> Option<&SensorMagnitude> {
        if self.availability.is_current() {
            self.value.as_ref()
        } else {
            None
        }
    }

    #[must_use]
    pub const fn last_known_value(&self) -> Option<&SensorMagnitude> {
        self.value.as_ref()
    }

    #[must_use]
    pub fn current_number(&self) -> Option<f64> {
        self.current_value()
            .and_then(|value| scaled_number(value, self.source_scale()?))
    }

    #[must_use]
    pub fn last_known_number(&self) -> Option<f64> {
        self.last_known_value()
            .and_then(|value| scaled_number(value, self.source_scale()?))
    }

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

    #[must_use]
    pub fn retain_previous(self, previous: &Self) -> Self {
        match self.availability {
            ScalarAvailability::Unavailable(failure) if self.descriptor == previous.descriptor => {
                previous.clone().transition_failure(failure)
            }
            _ => self,
        }
    }
}

fn validate_observation(
    descriptor: &SensorDescriptor,
    value: Option<&SensorMagnitude>,
    availability: ScalarAvailability,
    last_success_ms: Option<u64>,
) -> Result<(), SensorModelError> {
    let shape_valid = match availability {
        ScalarAvailability::Unknown => {
            descriptor.quantity() == &SensorQuantity::Unknown
                && value.is_none()
                && last_success_ms.is_none()
        }
        ScalarAvailability::Available
        | ScalarAvailability::Partial(_)
        | ScalarAvailability::Stale(_) => value.is_some() && last_success_ms.is_some(),
        ScalarAvailability::Unavailable(_) => value.is_none() && last_success_ms.is_none(),
    };
    if !shape_valid {
        return Err(SensorModelError::InvalidObservation);
    }
    if let Some(value) = value {
        validate_magnitude(descriptor, value)?;
    }
    Ok(())
}

fn validate_magnitude(
    descriptor: &SensorDescriptor,
    magnitude: &SensorMagnitude,
) -> Result<(), SensorModelError> {
    let quantity = descriptor.quantity();
    if matches!(quantity, SensorQuantity::Opaque(_)) {
        return validate_opaque_magnitude(magnitude);
    }
    let scaled = descriptor
        .source_scale()
        .and_then(|scale| scaled_number(magnitude, scale));
    let valid = match quantity {
        SensorQuantity::Temperature => scaled.is_some_and(|value| value >= ABSOLUTE_ZERO_C),
        SensorQuantity::FanSpeed => {
            matches!(magnitude, SensorMagnitude::Unsigned(_))
                && scaled.is_some_and(|value| value >= 0.0)
        }
        SensorQuantity::Power | SensorQuantity::Energy => {
            numeric_is_nonnegative(magnitude) && scaled.is_some_and(|value| value >= 0.0)
        }
        SensorQuantity::Voltage | SensorQuantity::Current => scaled.is_some(),
        SensorQuantity::RelativeHumidity => {
            numeric_is_nonnegative(magnitude)
                && scaled.is_some_and(|value| (0.0..=100.0).contains(&value))
        }
        SensorQuantity::PwmDutyCycle => matches!(
            magnitude,
            SensorMagnitude::DutyCycle { value, maximum }
                if *maximum > 0 && *value <= *maximum
        ),
        SensorQuantity::Intrusion => matches!(magnitude, SensorMagnitude::Boolean(_)),
        SensorQuantity::Unknown | SensorQuantity::Opaque(_) => false,
    };
    valid
        .then_some(())
        .ok_or(SensorModelError::InvalidMagnitude)
}

fn validate_opaque_magnitude(magnitude: &SensorMagnitude) -> Result<(), SensorModelError> {
    let valid = match magnitude {
        SensorMagnitude::Decimal(value) => value.is_finite(),
        SensorMagnitude::DutyCycle { value, maximum } => *maximum > 0 && *value <= *maximum,
        SensorMagnitude::Signed(_) | SensorMagnitude::Unsigned(_) | SensorMagnitude::Boolean(_) => {
            true
        }
    };
    valid
        .then_some(())
        .ok_or(SensorModelError::InvalidMagnitude)
}

fn numeric_is_nonnegative(magnitude: &SensorMagnitude) -> bool {
    match magnitude {
        SensorMagnitude::Signed(value) => *value >= 0,
        SensorMagnitude::Unsigned(_) => true,
        SensorMagnitude::Decimal(value) => value.is_finite() && *value >= 0.0,
        SensorMagnitude::Boolean(_) | SensorMagnitude::DutyCycle { .. } => false,
    }
}

fn scaled_number(magnitude: &SensorMagnitude, scale: SensorScale) -> Option<f64> {
    let raw = match magnitude {
        SensorMagnitude::Signed(value) => value.to_string().parse::<f64>().ok()?,
        SensorMagnitude::Unsigned(value) => value.to_string().parse::<f64>().ok()?,
        SensorMagnitude::Decimal(value) if value.is_finite() => *value,
        SensorMagnitude::Decimal(_)
        | SensorMagnitude::Boolean(_)
        | SensorMagnitude::DutyCycle { .. } => return None,
    };
    scale.apply(raw)
}
