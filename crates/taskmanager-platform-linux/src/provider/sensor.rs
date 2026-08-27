//! Linux sensor-center provider implementation.

use taskmanager_core::SensorCenterSnapshot;
use taskmanager_platform_contract::{DeviceSourceSnapshot, ProviderFailure};

use taskmanager_platform_provider::SensorProvider;

pub(super) struct NativeSensorProvider;

impl SensorProvider for NativeSensorProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<SensorCenterSnapshot>, ProviderFailure> {
        Ok(crate::engine::sensors::collect_sensor_center_source(
            observed_at_ms,
        ))
    }
}
