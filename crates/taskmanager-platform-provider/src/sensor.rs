use taskmanager_core::SensorCenterSnapshot;
use taskmanager_platform_contract::{DeviceSourceSnapshot, ProviderFailure};

/// Dynamic thermal, fan, and power-channel observation.
pub trait SensorProvider: Send + 'static {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<SensorCenterSnapshot>, ProviderFailure>;
}
