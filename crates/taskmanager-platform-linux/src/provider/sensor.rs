//! Linux sensor-center provider implementation.

use taskforest_contract::{DeviceSourceSnapshot, ProviderFailure};
use taskforest_core::SensorCenterSnapshot;

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
