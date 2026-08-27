//! Provider-neutral thermal control and mitigation observations.
//!
//! These facts are deliberately separate from [`super::SensorReading`].
//! A trip threshold or cooling state is control-plane telemetry, not a sensor
//! measurement and not evidence of physical-device presence by itself.

use serde::{Deserialize, Serialize};

use crate::core::{DeviceGeneration, DeviceId, FailureKind, ScalarAvailability, ScalarObservation};

use super::ThermalThrottleSnapshot;

/// Whether a thermal zone currently participates in kernel/OS thermal policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ThermalZoneMode {
    Enabled,
    Disabled,
    Other(String),
}

/// Provider-neutral thermal policy vocabulary with a future-safe opaque case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ThermalPolicy {
    PowerAllocator,
    UserSpace,
    StepWise,
    BangBang,
    FairShare,
    Other(String),
}

/// Semantic role of a temperature trip point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ThermalTripKind {
    Active,
    Passive,
    Hot,
    Critical,
    Other(String),
}

/// Broad cooling mechanism without exposing an operating-system type token as
/// product logic. Unknown future mechanisms retain their exact opaque label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ThermalCoolingKind {
    Fan,
    Processor,
    Charger,
    Radio,
    PowerClamp,
    TemperatureOffset,
    Other(String),
}

/// Whether a cooling device's native state is zero or non-zero.
///
/// `Active` means mitigation/cooling work is selected by the provider. It does
/// not imply that a CPU/GPU is currently throttled, and it is not normalized
/// into a guessed percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalCoolingActivity {
    Inactive,
    Active,
}

/// One independently fallible trip-point observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThermalTripPoint {
    pub id: String,
    pub kind: ScalarObservation<ThermalTripKind>,
    /// Absolute threshold in milli-degrees Celsius.
    pub temperature_millicelsius: ScalarObservation<i64>,
    /// Hysteresis delta in milli-degrees Celsius.
    pub hysteresis_millicelsius: ScalarObservation<u64>,
}

impl ThermalTripPoint {
    #[must_use]
    pub fn transition_failure(mut self, failure: FailureKind) -> Self {
        self.kind = self.kind.transition_failure(failure);
        self.temperature_millicelsius = self.temperature_millicelsius.transition_failure(failure);
        self.hysteresis_millicelsius = self.hysteresis_millicelsius.transition_failure(failure);
        self
    }
}

/// Trip-point inventory truth is independent from each trip's field truth.
///
/// A current empty list confirms that the zone has no exposed trip points.
/// `Unavailable` means the provider could not enumerate the directory and
/// therefore cannot claim that the list is empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThermalTripPointSet {
    pub points: Vec<ThermalTripPoint>,
    pub availability: ScalarAvailability,
    pub last_success_ms: Option<u64>,
}

impl Default for ThermalTripPointSet {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            availability: ScalarAvailability::Unknown,
            last_success_ms: None,
        }
    }
}

impl ThermalTripPointSet {
    #[must_use]
    pub const fn available(points: Vec<ThermalTripPoint>, observed_at_ms: u64) -> Self {
        Self {
            points,
            availability: ScalarAvailability::Available,
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn partial(
        points: Vec<ThermalTripPoint>,
        observed_at_ms: u64,
        failure: FailureKind,
    ) -> Self {
        Self {
            points,
            availability: ScalarAvailability::Partial(failure),
            last_success_ms: Some(observed_at_ms),
        }
    }

    #[must_use]
    pub const fn unavailable(failure: FailureKind) -> Self {
        Self {
            points: Vec::new(),
            availability: ScalarAvailability::Unavailable(failure),
            last_success_ms: None,
        }
    }

    #[must_use]
    pub fn transition_failure(mut self, failure: FailureKind) -> Self {
        if self.last_success_ms.is_some() {
            self.points = self
                .points
                .into_iter()
                .map(|point| point.transition_failure(failure))
                .collect();
            self.availability = ScalarAvailability::Stale(failure);
        } else {
            self.points.clear();
            self.availability = ScalarAvailability::Unavailable(failure);
        }
        self
    }

    #[must_use]
    pub fn current_points(&self) -> Option<&[ThermalTripPoint]> {
        self.availability
            .is_current()
            .then_some(self.points.as_slice())
    }
}

/// Control-plane facts for one physical thermal zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThermalZoneStatus {
    /// Stable zone identity within the owning physical device.
    pub id: String,
    pub device_id: DeviceId,
    #[serde(default)]
    pub device_generation: DeviceGeneration,
    pub label: ScalarObservation<String>,
    pub mode: ScalarObservation<ThermalZoneMode>,
    pub policy: ScalarObservation<ThermalPolicy>,
    pub trip_points: ThermalTripPointSet,
}

/// Control-plane facts for one physical cooling/mitigation device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThermalCoolingDeviceStatus {
    /// Stable cooling-channel identity within the owning physical device.
    pub id: String,
    pub device_id: DeviceId,
    #[serde(default)]
    pub device_generation: DeviceGeneration,
    pub kind: ScalarObservation<ThermalCoolingKind>,
    /// Provider-native ordinal. It is deliberately not normalized to percent.
    pub current_state: ScalarObservation<u64>,
    pub maximum_state: ScalarObservation<u64>,
    pub activity: ScalarObservation<ThermalCoolingActivity>,
}

/// Thermal control sidecar carried with the sensor snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ThermalControlSnapshot {
    pub zones: Vec<ThermalZoneStatus>,
    pub cooling_devices: Vec<ThermalCoolingDeviceStatus>,
    /// Cumulative CPU/package events when the platform exposes them.
    ///
    /// Historical non-zero counts do not prove current throttling.
    pub throttle: ThermalThrottleSnapshot,
}

#[cfg(test)]
#[path = "../../../tests/headless/sensors/thermal_control/lifecycle_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
#[path = "../../../tests/headless/core_core_sensors_thermal_control_tests.rs"]
mod tests;
