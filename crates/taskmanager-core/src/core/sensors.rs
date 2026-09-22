//! Platform-neutral sensor snapshots and lifecycle reconciliation.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::core::device_state::{
    DeviceLifecycle, DeviceLifecycleDelta, DeviceLifecycleRegistry, DeviceRefreshOutcome,
    DeviceState, DeviceStatus,
};
use crate::core::metrics::{ScalarAvailability, ScalarObservation};
use crate::core::{DeviceGeneration, DeviceId};

mod availability;
mod measurement;
mod thermal_control;

pub use measurement::{
    SensorDescriptor, SensorMagnitude, SensorMeasurementObservation, SensorModelError,
    SensorQuantity, SensorScale, SensorUnit,
};
pub use thermal_control::{
    ThermalControlSnapshot, ThermalCoolingActivity, ThermalCoolingDeviceStatus, ThermalCoolingKind,
    ThermalPolicy, ThermalTripKind, ThermalTripPoint, ThermalTripPointSet, ThermalZoneMode,
    ThermalZoneStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum SensorKind {
    Temperature,
    Fan,
    Power,
    /// Compatibility projection for quantities the legacy model cannot carry.
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SensorValue {
    TemperatureC(f32),
    FanRpm(u32),
    PowerW(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SensorReading {
    device_id: DeviceId,
    device_generation: DeviceGeneration,
    id: String,
    label: String,
    /// The only writable measurement authority. The observation validates the
    /// quantity/unit/value shape and owns freshness, failure, and success time.
    measurement_observation: SensorMeasurementObservation,
}

/// Compatibility-only snapshot shape. Legacy kind/value/state projections are
/// never stored in the domain model or exposed to providers.
#[derive(Serialize, Deserialize)]
struct SensorReadingWire {
    #[serde(default)]
    device_id: DeviceId,
    #[serde(default)]
    device_generation: DeviceGeneration,
    id: String,
    label: String,
    #[serde(default)]
    kind: SensorKind,
    #[serde(default)]
    value: Option<SensorValue>,
    #[serde(default)]
    value_observation: ScalarObservation<SensorValue>,
    #[serde(default)]
    measurement_observation: SensorMeasurementObservation,
    #[serde(default)]
    state: DeviceState,
}

impl Serialize for SensorReading {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value_observation =
            availability::project_legacy_observation(&self.measurement_observation);
        SensorReadingWire {
            device_id: self.device_id.clone(),
            device_generation: self.device_generation,
            id: self.id.clone(),
            label: self.label.clone(),
            kind: availability::legacy_kind(self.measurement_observation.descriptor()),
            value: value_observation.current_value().copied(),
            value_observation,
            measurement_observation: self.measurement_observation.clone(),
            state: availability::observation_state(&self.measurement_observation),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SensorReading {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SensorReadingWire::deserialize(deserializer)?;
        let measurement_observation = if wire.measurement_observation.availability()
            != ScalarAvailability::Unknown
        {
            wire.measurement_observation
        } else {
            let legacy = if wire.value_observation.availability() != ScalarAvailability::Unknown {
                wire.value_observation
            } else {
                availability::compatibility_observation(wire.value, wire.state)
            };
            availability::measurement_from_legacy(wire.kind, legacy)
        };
        Ok(Self {
            device_id: wire.device_id,
            device_generation: wire.device_generation,
            id: wire.id,
            label: wire.label,
            measurement_observation,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SensorCenterSnapshot {
    pub state: DeviceState,
    pub timestamp_ms: u64,
    pub readings: Vec<SensorReading>,
    /// Thermal control-plane facts, kept separate from measurement readings.
    #[serde(default)]
    pub thermal_control: ThermalControlSnapshot,
    /// Retained physical-device lifecycle records keyed by `device_id`.
    #[serde(default)]
    pub device_lifecycles: HashMap<String, DeviceLifecycle>,
}

pub fn refresh_sensor_center_state(
    previous: DeviceState,
    snapshot: &mut SensorCenterSnapshot,
    now_ms: u64,
) {
    snapshot.state = previous.transition(snapshot.state.status, now_ms);
}

/// Stateful hot-plug reconciliation for platform-neutral sensor snapshots.
///
/// The current snapshot never replays an absent sensor's last value. Lifecycle
/// records remain queryable during the grace period, while a reappearing stable
/// physical-device ID advances its generation. Individual channels retain
/// independent health timestamps but do not create device generations. A
/// failed center enumeration is recorded as unavailable rather than falsely
/// declaring every prior physical device absent.
#[derive(Debug, Clone)]
pub struct SensorLifecycleTracker {
    registry: DeviceLifecycleRegistry,
    center_state: DeviceState,
    channel_observations: HashMap<String, RetainedSensorObservation>,
}

#[derive(Debug, Clone)]
struct RetainedSensorObservation {
    device_id: DeviceId,
    device_generation: DeviceGeneration,
    observation: SensorMeasurementObservation,
}

impl SensorLifecycleTracker {
    #[must_use]
    pub fn new(retention_ms: u64) -> Self {
        Self {
            registry: DeviceLifecycleRegistry::new(retention_ms),
            center_state: DeviceState::default(),
            channel_observations: HashMap::new(),
        }
    }

    pub fn reconcile(&mut self, snapshot: &mut SensorCenterSnapshot) -> DeviceLifecycleDelta {
        let outcome = if snapshot.readings.is_empty()
            && matches!(
                snapshot.state.status,
                DeviceStatus::Stale | DeviceStatus::PermissionDenied | DeviceStatus::MissingTool
            ) {
            DeviceRefreshOutcome::Unavailable(snapshot.state.status)
        } else {
            DeviceRefreshOutcome::Complete
        };
        self.reconcile_with_outcome(snapshot, outcome)
    }

    pub fn reconcile_with_outcome(
        &mut self,
        snapshot: &mut SensorCenterSnapshot,
        outcome: DeviceRefreshOutcome,
    ) -> DeviceLifecycleDelta {
        let discovered_devices = snapshot
            .readings
            .iter()
            .map(sensor_device_id)
            .map(DeviceId::new)
            .collect::<Vec<_>>();
        self.reconcile_discovered(snapshot, &discovered_devices, outcome)
    }

    /// Reconcile readings against the provider's explicit discovery authority.
    ///
    /// `snapshot.readings` may retain stale rows for presentation, but only
    /// IDs in `discovered_devices` are evidence that a physical device was
    /// present in this refresh.
    pub fn reconcile_discovered(
        &mut self,
        snapshot: &mut SensorCenterSnapshot,
        discovered_devices: &[DeviceId],
        outcome: DeviceRefreshOutcome,
    ) -> DeviceLifecycleDelta {
        let now_ms = snapshot.timestamp_ms;
        self.center_state = self.center_state.merge_observation(snapshot.state, now_ms);
        snapshot.state = self.center_state;

        self.registry.begin_refresh();
        let discovered_devices = discovered_devices
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect::<HashSet<_>>();
        let mut observed_channels = HashSet::new();
        let mut device_states = HashMap::<String, DeviceState>::new();
        for reading in &mut snapshot.readings {
            let observation = reading.measurement_observation().clone();
            let device_id = sensor_device_id(reading);
            let generation = self
                .registry
                .get(&device_id)
                .map_or(reading.device_generation, |lifecycle| lifecycle.generation);
            let observation = self
                .channel_observations
                .get(reading.id())
                .filter(|retained| {
                    retained.device_id.as_str() == device_id
                        && retained.device_generation == generation
                        && retained.observation.descriptor() == observation.descriptor()
                })
                .map_or(observation.clone(), |retained| {
                    observation.retain_previous(&retained.observation)
                });
            self.channel_observations.insert(
                reading.id().to_owned(),
                RetainedSensorObservation {
                    device_id: DeviceId::new(device_id.clone()),
                    device_generation: generation,
                    observation: observation.clone(),
                },
            );
            reading.replace_measurement_observation(observation);
            let channel_state = reading.state();
            if !discovered_devices.contains(&device_id) {
                continue;
            }
            observed_channels.insert(reading.id().to_owned());
            device_states
                .entry(device_id)
                .and_modify(|state| *state = aggregate_sensor_state(*state, channel_state))
                .or_insert(channel_state);
        }
        for device_id in &discovered_devices {
            device_states
                .entry(device_id.clone())
                .or_insert(snapshot.state);
        }
        for (device_id, state) in device_states {
            self.registry.observe(device_id, state, now_ms);
        }
        for reading in &mut snapshot.readings {
            let device_id = sensor_device_id(reading);
            if reading.device_id().as_str().is_empty() {
                reading.set_device_id(DeviceId::new(device_id));
            }
        }
        let delta = self.registry.finish_refresh(outcome, now_ms);
        for reading in &mut snapshot.readings {
            let device_id = sensor_device_id(reading);
            if let Some(lifecycle) = self.registry.get(&device_id) {
                reading.set_device_generation(lifecycle.generation);
                if let Some(retained) = self.channel_observations.get_mut(reading.id()) {
                    retained.device_generation = reading.device_generation();
                }
                if !discovered_devices.contains(&device_id) {
                    let failure = lifecycle
                        .state
                        .status
                        .failure()
                        .unwrap_or(crate::core::FailureKind::ProviderFault);
                    let observation = reading
                        .measurement_observation()
                        .clone()
                        .transition_failure(failure);
                    reading.replace_measurement_observation(observation);
                }
            }
        }
        for zone in &mut snapshot.thermal_control.zones {
            if let Some(lifecycle) = self.registry.get(zone.device_id.as_str()) {
                zone.device_generation = lifecycle.generation;
            }
        }
        for device in &mut snapshot.thermal_control.cooling_devices {
            if let Some(lifecycle) = self.registry.get(device.device_id.as_str()) {
                device.device_generation = lifecycle.generation;
            }
        }
        if outcome == DeviceRefreshOutcome::Complete {
            self.channel_observations
                .retain(|id, _| observed_channels.contains(id));
        }
        snapshot.device_lifecycles = self
            .registry
            .iter()
            .map(|(id, lifecycle)| (id.to_owned(), *lifecycle))
            .collect();
        delta
    }

    #[must_use]
    pub fn lifecycle(&self, sensor_id: &str) -> Option<&DeviceLifecycle> {
        self.registry.get(sensor_id)
    }
}

fn sensor_device_id(reading: &SensorReading) -> String {
    if reading.device_id().as_str().is_empty() {
        reading.id().to_owned()
    } else {
        reading.device_id().as_str().to_owned()
    }
}

fn aggregate_sensor_state(current: DeviceState, observed: DeviceState) -> DeviceState {
    let status = if observed.status.severity() > current.status.severity() {
        observed.status
    } else {
        current.status
    };
    let last_success_ms = match (current.last_success_ms, observed.last_success_ms) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    };
    DeviceState {
        status,
        last_success_ms,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/sensors.rs"]
mod tests;
