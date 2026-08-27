//! Windows sensor-domain provider.
//!
//! Thermal-zone readings come from the audited boundary's WMI/COM ACPI
//! thermal-zone query (`MSAcpi_ThermalZoneTemperature` and the
//! LibreHardwareMonitor/OpenHardwareMonitor namespaces), with sysinfo
//! Components as a fallback source. Readings are validated, deduplicated
//! and sorted; when only the fallback was reachable or identities stay
//! ambiguous, discovery degrades to a typed `Partial(Unsupported)` state
//! instead of fabricating empty devices. Fans have no user-mode Windows
//! source and stay typed absent.

use taskmanager_application::SensorRequest;
use taskmanager_core::{
    DeviceGeneration, DeviceState, DeviceStatus, FailureKind, ProviderId, SensorCenterSnapshot,
    SensorDescriptor, SensorMagnitude, SensorMeasurementObservation, SensorReading, SensorScale,
};
use taskmanager_platform_contract::{DeviceDiscovery, DeviceSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::SensorProvider;
use taskmanager_platform_runtime::{ProviderRegistration, SensorExecutors, SensorProviderBindings};

const SENSOR_CAPABILITY_PROVIDER: ProviderId = ProviderId::borrowed("windows.sensor.native");

pub struct WinSensorProvider;

impl WinSensorProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WinSensorProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorProvider for WinSensorProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<SensorCenterSnapshot>, ProviderFailure> {
        let acpi = taskmanager_windows_api::query_acpi_thermal_zones();
        let acpi_failed = acpi.is_err();
        let mut thermal_zones = acpi.unwrap_or_default();
        let mut used_component_fallback = false;
        if thermal_zones.is_empty() {
            used_component_fallback = true;
            let components = sysinfo::Components::new_with_refreshed_list();
            for c in components.iter() {
                if let Some(temp) = c.temperature()
                    && temp > 0.0
                    && temp < 120.0
                {
                    thermal_zones.push(taskmanager_windows_api::WindowsThermalZoneReading {
                        name: c.label().to_string(),
                        temperature_c: temp,
                        critical_trip_point_c: c.critical(),
                    });
                }
            }
        }

        thermal_zones.sort_by(|left, right| left.name.cmp(&right.name));
        let mut identities = std::collections::HashSet::<String>::new();
        let mut ambiguous_identities = 0_usize;
        let mut rejected_readings = 0_usize;
        let readings = thermal_zones
            .into_iter()
            .filter_map(|z| {
                if z.name.trim().is_empty()
                    || !z.temperature_c.is_finite()
                    || !(-100.0..=250.0).contains(&z.temperature_c)
                {
                    rejected_readings += 1;
                    return None;
                }
                let normalized = z.name.trim().to_ascii_lowercase();
                if !identities.insert(normalized.clone()) {
                    ambiguous_identities += 1;
                    return None;
                }
                let id = format!("thermal-zone:{normalized}");
                let device_id = taskmanager_core::DeviceId::from(format!("windows:{id}"));
                let Ok(measurement) = SensorMeasurementObservation::available(
                    SensorDescriptor::temperature(SensorScale::IDENTITY),
                    SensorMagnitude::Decimal(f64::from(z.temperature_c)),
                    observed_at_ms,
                ) else {
                    rejected_readings += 1;
                    return None;
                };
                Some(
                    SensorReading::from_measurement_observation(device_id, id, z.name, measurement)
                        .with_device_generation(DeviceGeneration::INITIAL),
                )
            })
            .collect::<Vec<_>>();

        let device_ids = readings
            .iter()
            .map(|reading| reading.device_id().clone())
            .collect::<Vec<_>>();
        let failure = acpi_failed
            .then_some(FailureKind::TemporarilyUnavailable)
            .or_else(|| (rejected_readings > 0).then_some(FailureKind::ProviderFault))
            .or_else(|| {
                (used_component_fallback || ambiguous_identities > 0)
                    .then_some(FailureKind::Unsupported)
            });
        let discovery = if let Some(failure) = failure {
            if readings.is_empty() {
                DeviceDiscovery::Unavailable(failure)
            } else {
                DeviceDiscovery::Partial {
                    discovered_devices: device_ids,
                    failure,
                }
            }
        } else if readings.is_empty() {
            DeviceDiscovery::Empty
        } else {
            DeviceDiscovery::Available(device_ids)
        };
        let state = match &discovery {
            DeviceDiscovery::Available(_) | DeviceDiscovery::Empty => {
                DeviceState::healthy(observed_at_ms)
            }
            DeviceDiscovery::Partial { .. } => DeviceState {
                status: DeviceStatus::Stale,
                last_success_ms: Some(observed_at_ms),
            },
            DeviceDiscovery::Unavailable(_) => DeviceState::default(),
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

pub struct WinSensorProviders {
    observation: ProviderRegistration<SensorRequest, Box<dyn SensorProvider>>,
}

impl WinSensorProviders {
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
#[path = "../../tests/headless/platform_windows_provider_sensor.rs"]
mod tests;
