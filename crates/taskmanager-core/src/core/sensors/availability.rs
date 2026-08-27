//! Sensor availability: `SensorReading` constructors and current/last-known
//! accessors that reconcile legacy value fields with authoritative typed
//! measurement observations, and bridge the old `SensorKind`/`SensorValue`
//! vocabulary to descriptors and magnitudes.

use super::{
    SensorDescriptor, SensorKind, SensorMagnitude, SensorMeasurementObservation, SensorQuantity,
    SensorReading, SensorScale, SensorValue,
};
use crate::core::metrics::{ScalarAvailability, ScalarObservation};
use crate::core::{DeviceGeneration, DeviceId, DeviceState, DeviceStatus, FailureKind};

impl SensorReading {
    /// Construct a current or explicitly unavailable reading from authoritative
    /// quantity/unit/value truth.
    ///
    /// Native providers should use this constructor. Legacy wire fields are
    /// projected only when the reading is serialized.
    #[must_use]
    pub fn from_measurement_observation(
        device_id: DeviceId,
        id: String,
        label: String,
        observation: SensorMeasurementObservation,
    ) -> Self {
        let mut reading = Self {
            device_id,
            device_generation: DeviceGeneration::default(),
            id,
            label,
            measurement_observation: SensorMeasurementObservation::default(),
        };
        reading.replace_measurement_observation(observation);
        reading
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn device_generation(&self) -> DeviceGeneration {
        self.device_generation
    }

    #[must_use]
    pub const fn id(&self) -> &str {
        self.id.as_str()
    }

    #[must_use]
    pub const fn label(&self) -> &str {
        self.label.as_str()
    }

    #[must_use]
    pub const fn measurement_observation(&self) -> &SensorMeasurementObservation {
        &self.measurement_observation
    }

    #[must_use]
    pub const fn quantity(&self) -> &SensorQuantity {
        self.measurement_observation.quantity()
    }

    #[must_use]
    pub fn current_number(&self) -> Option<f64> {
        self.measurement_observation.current_number()
    }

    #[must_use]
    pub fn last_known_number(&self) -> Option<f64> {
        self.measurement_observation.last_known_number()
    }

    #[must_use]
    pub fn state(&self) -> DeviceState {
        observation_state(&self.measurement_observation)
    }

    #[must_use]
    pub const fn with_device_generation(mut self, generation: DeviceGeneration) -> Self {
        self.device_generation = generation;
        self
    }

    /// Current platform-neutral magnitude. Legacy fields are consulted only
    /// while the authoritative observation is `Unknown`.
    #[must_use]
    pub fn current_measurement(&self) -> Option<SensorMagnitude> {
        self.measurement_observation.current_value().copied()
    }

    #[must_use]
    pub fn last_known_measurement(&self) -> Option<SensorMagnitude> {
        self.measurement_observation.last_known_value().copied()
    }

    pub(super) fn replace_measurement_observation(
        &mut self,
        observation: SensorMeasurementObservation,
    ) {
        self.measurement_observation = observation;
    }

    pub(super) fn set_device_id(&mut self, device_id: DeviceId) {
        self.device_id = device_id;
    }

    pub(super) fn set_device_generation(&mut self, generation: DeviceGeneration) {
        self.device_generation = generation;
    }
}

pub(super) fn compatibility_observation(
    legacy: Option<SensorValue>,
    state: DeviceState,
) -> ScalarObservation<SensorValue> {
    match (state.status, legacy, state.last_success_ms) {
        (DeviceStatus::Healthy, Some(value), Some(observed_at_ms)) => {
            ScalarObservation::available(value, observed_at_ms)
        }
        (DeviceStatus::Healthy, _, _) => ScalarObservation::default(),
        (status, value, last_success_ms) => {
            let failure = status.failure().unwrap_or(FailureKind::ProviderFault);
            match (value, last_success_ms) {
                (Some(value), Some(last_success_ms)) => {
                    ScalarObservation::stale(value, last_success_ms, failure)
                }
                _ => ScalarObservation::unavailable(failure),
            }
        }
    }
}

pub(super) fn observation_state(observation: &SensorMeasurementObservation) -> DeviceState {
    let status = match observation.availability() {
        ScalarAvailability::Available | ScalarAvailability::Partial(_) => DeviceStatus::Healthy,
        ScalarAvailability::Stale(failure) | ScalarAvailability::Unavailable(failure) => {
            DeviceStatus::from_failure(failure)
        }
        ScalarAvailability::Unknown => DeviceStatus::Unsupported,
    };
    DeviceState {
        status,
        last_success_ms: observation.last_success_ms(),
    }
}

fn legacy_descriptor(kind: SensorKind) -> Option<SensorDescriptor> {
    match kind {
        SensorKind::Temperature => Some(SensorDescriptor::temperature(SensorScale::IDENTITY)),
        SensorKind::Fan => Some(SensorDescriptor::fan_speed(SensorScale::IDENTITY)),
        SensorKind::Power => Some(SensorDescriptor::power(SensorScale::IDENTITY)),
        SensorKind::Unknown => None,
    }
}

pub(super) fn legacy_kind(descriptor: &SensorDescriptor) -> SensorKind {
    match descriptor.quantity() {
        SensorQuantity::Temperature => SensorKind::Temperature,
        SensorQuantity::FanSpeed => SensorKind::Fan,
        SensorQuantity::Power => SensorKind::Power,
        SensorQuantity::Unknown
        | SensorQuantity::Voltage
        | SensorQuantity::Current
        | SensorQuantity::Energy
        | SensorQuantity::RelativeHumidity
        | SensorQuantity::PwmDutyCycle
        | SensorQuantity::Intrusion
        | SensorQuantity::Opaque(_) => SensorKind::Unknown,
    }
}

pub(super) fn measurement_from_legacy(
    kind: SensorKind,
    observation: ScalarObservation<SensorValue>,
) -> SensorMeasurementObservation {
    if observation.availability() == ScalarAvailability::Unknown {
        return SensorMeasurementObservation::default();
    }
    let Some(descriptor) = legacy_descriptor(kind) else {
        return SensorMeasurementObservation::default();
    };
    let magnitude = observation
        .last_known_value()
        .and_then(|value| legacy_magnitude(kind, *value));
    SensorMeasurementObservation::try_from_parts(
        descriptor,
        magnitude,
        observation.availability(),
        observation.last_success_ms(),
    )
    .unwrap_or_default()
}

fn legacy_magnitude(kind: SensorKind, value: SensorValue) -> Option<SensorMagnitude> {
    match (kind, value) {
        (SensorKind::Temperature, SensorValue::TemperatureC(value))
        | (SensorKind::Power, SensorValue::PowerW(value)) => {
            Some(SensorMagnitude::Decimal(f64::from(value)))
        }
        (SensorKind::Fan, SensorValue::FanRpm(value)) => {
            Some(SensorMagnitude::Unsigned(u64::from(value)))
        }
        _ => None,
    }
}

pub(super) fn project_legacy_observation(
    observation: &SensorMeasurementObservation,
) -> ScalarObservation<SensorValue> {
    if legacy_kind(observation.descriptor()) == SensorKind::Unknown {
        return ScalarObservation::default();
    }
    let value = observation
        .last_known_value()
        .and_then(|magnitude| project_legacy_value(observation.descriptor(), magnitude));
    if observation.last_known_value().is_some() && value.is_none() {
        return ScalarObservation::default();
    }
    let Some(value) = value else {
        return ScalarObservation::default();
    };
    match observation.availability() {
        ScalarAvailability::Unknown => ScalarObservation::default(),
        ScalarAvailability::Available => observation
            .last_success_ms()
            .map_or_else(ScalarObservation::default, |observed_at_ms| {
                ScalarObservation::available(value, observed_at_ms)
            }),
        ScalarAvailability::Partial(failure) => observation
            .last_success_ms()
            .map_or_else(ScalarObservation::default, |observed_at_ms| {
                ScalarObservation::partial(value, observed_at_ms, failure)
            }),
        ScalarAvailability::Stale(failure) => observation
            .last_success_ms()
            .map_or_else(ScalarObservation::default, |observed_at_ms| {
                ScalarObservation::stale(value, observed_at_ms, failure)
            }),
        ScalarAvailability::Unavailable(failure) => ScalarObservation::unavailable(failure),
    }
}

fn project_legacy_value(
    descriptor: &SensorDescriptor,
    magnitude: &SensorMagnitude,
) -> Option<SensorValue> {
    let number = SensorMeasurementObservation::try_from_parts(
        descriptor.clone(),
        Some(*magnitude),
        ScalarAvailability::Available,
        Some(0),
    )
    .ok()?
    .current_number()?;
    match descriptor.quantity() {
        SensorQuantity::Temperature => number
            .to_string()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(SensorValue::TemperatureC),
        SensorQuantity::FanSpeed => number
            .to_string()
            .parse::<u32>()
            .ok()
            .map(SensorValue::FanRpm),
        SensorQuantity::Power => number
            .to_string()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(SensorValue::PowerW),
        SensorQuantity::Unknown
        | SensorQuantity::Voltage
        | SensorQuantity::Current
        | SensorQuantity::Energy
        | SensorQuantity::RelativeHumidity
        | SensorQuantity::PwmDutyCycle
        | SensorQuantity::Intrusion
        | SensorQuantity::Opaque(_) => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_sensors_availability_tests.rs"]
mod tests;
