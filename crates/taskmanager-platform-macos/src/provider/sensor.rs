//! macOS sensor-domain provider built on `sysinfo` Components.
//!
//! sysinfo reads temperatures through the Apple SMC event service
//! (IOHIDServiceClient) behind its safe API on both Intel and Apple Silicon.
//! Fan speeds have no safe accessor and stay absent (ADR-019).

use taskmanager_application::SensorRequest;
use taskmanager_core::{
    DeviceGeneration, DeviceState, DeviceStatus, FailureKind, ProviderId, SensorCenterSnapshot,
    SensorDescriptor, SensorMagnitude, SensorMeasurementObservation, SensorReading, SensorScale,
};
use taskmanager_platform_contract::{DeviceDiscovery, DeviceSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::SensorProvider;
use taskmanager_platform_runtime::{ProviderRegistration, SensorExecutors, SensorProviderBindings};

/// Temperature sensors from `sysinfo::Components` (SMC event service).
pub struct MacSensorProvider;
const SENSOR_CAPABILITY_PROVIDER: ProviderId = ProviderId::borrowed("macos.sensor.registry");

impl SensorProvider for MacSensorProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<SensorCenterSnapshot>, ProviderFailure> {
        let components = sysinfo::Components::new_with_refreshed_list();
        let mut samples = components
            .list()
            .iter()
            .map(|component| (component.label().trim().to_owned(), component.temperature()))
            .filter(|(label, _)| !label.is_empty())
            .collect::<Vec<_>>();
        samples.sort_by(|left, right| left.0.cmp(&right.0));
        let mut readings = Vec::new();
        let mut identities = std::collections::HashSet::<String>::new();
        let mut ambiguous_identities = 0_usize;
        let mut invalid_values = 0_usize;
        for (label, temperature) in samples {
            let normalized = label.to_ascii_lowercase();
            if !identities.insert(normalized.clone()) {
                ambiguous_identities += 1;
                continue;
            }
            let device_id = taskmanager_core::DeviceId::new(format!("macos:sensor:{normalized}"));
            let valid_temperature =
                temperature.filter(|value| value.is_finite() && (-100.0..=250.0).contains(value));
            if temperature.is_some() && valid_temperature.is_none() {
                invalid_values += 1;
            }
            let descriptor = SensorDescriptor::temperature(SensorScale::IDENTITY);
            let measurement = match valid_temperature {
                Some(value) => SensorMeasurementObservation::available(
                    descriptor.clone(),
                    SensorMagnitude::Decimal(f64::from(value)),
                    observed_at_ms,
                )
                .unwrap_or_else(|_| {
                    SensorMeasurementObservation::unavailable(
                        descriptor,
                        FailureKind::ProviderFault,
                    )
                }),
                None => SensorMeasurementObservation::unavailable(
                    descriptor,
                    if temperature.is_some() {
                        FailureKind::ProviderFault
                    } else {
                        FailureKind::TemporarilyUnavailable
                    },
                ),
            };
            readings.push(
                SensorReading::from_measurement_observation(
                    device_id,
                    format!("sensor:{normalized}"),
                    label,
                    measurement,
                )
                .with_device_generation(DeviceGeneration::INITIAL),
            );
        }

        let device_ids = readings
            .iter()
            .map(|reading| reading.device_id().clone())
            .collect::<Vec<_>>();
        let discovery = if readings.is_empty() {
            DeviceDiscovery::Empty
        } else {
            // Component labels are not native serials/registry instance IDs.
            // Publish the useful readings but make identity degradation
            // explicit; ambiguous duplicates were dropped above.
            DeviceDiscovery::Partial {
                discovered_devices: device_ids,
                failure: FailureKind::Unsupported,
            }
        };
        let state = if invalid_values > 0 || ambiguous_identities > 0 || !readings.is_empty() {
            DeviceState {
                status: DeviceStatus::Stale,
                last_success_ms: Some(observed_at_ms),
            }
        } else {
            DeviceState::healthy(observed_at_ms)
        };
        Ok(DeviceSourceSnapshot::from_discovery(
            SensorCenterSnapshot {
                state,
                timestamp_ms: observed_at_ms,
                readings,
                thermal_control: Default::default(),
                device_lifecycles: Default::default(),
            },
            SENSOR_CAPABILITY_PROVIDER,
            discovery,
            Vec::new(),
        ))
    }
}

pub struct MacSensorProviders {
    observation: ProviderRegistration<SensorRequest, Box<dyn SensorProvider>>,
}

impl MacSensorProviders {
    #[must_use]
    pub fn new<P>(observation: ProviderRegistration<SensorRequest, P>) -> Self
    where
        P: SensorProvider,
    {
        Self {
            observation: observation
                .map_provider(|provider| Box::new(provider) as Box<dyn SensorProvider>),
        }
    }

    pub(crate) fn runtime_bindings(&self) -> SensorProviderBindings {
        SensorProviderBindings::from_registration(&self.observation)
    }

    pub(crate) fn into_runtime(self) -> SensorExecutors {
        let Self { observation } = self;
        let mut observation = observation.into_provider();
        SensorExecutors::new(move |observed_at_ms| observation.refresh(observed_at_ms))
    }
}

#[cfg(test)]
#[path = "../../tests/headless/macos_provider_sensor.rs"]
mod tests;
