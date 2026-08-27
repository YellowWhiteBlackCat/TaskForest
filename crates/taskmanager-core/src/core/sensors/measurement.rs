//! Validated, platform-neutral sensor quantity and unit contracts.

use std::fmt;

mod descriptor;
mod observation;
#[cfg(test)]
#[path = "../../../tests/headless/sensors/measurement.rs"]
mod tests;

pub use descriptor::{SensorDescriptor, SensorQuantity, SensorScale, SensorUnit};
pub use observation::{SensorMagnitude, SensorMeasurementObservation};

/// Why a quantity/unit/value combination was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorModelError {
    EmptyOpaqueToken,
    InvalidScale,
    InvalidDescriptor,
    InvalidMagnitude,
    InvalidObservation,
}

impl fmt::Display for SensorModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyOpaqueToken => "opaque sensor tokens must not be empty",
            Self::InvalidScale => "sensor scale must be a positive finite ratio",
            Self::InvalidDescriptor => "sensor quantity, unit, and scale are incompatible",
            Self::InvalidMagnitude => "sensor magnitude is incompatible or out of range",
            Self::InvalidObservation => "sensor value, availability, and success time disagree",
        })
    }
}

impl std::error::Error for SensorModelError {}
