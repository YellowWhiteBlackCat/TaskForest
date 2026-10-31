use taskforest_contract::{DeviceSourceSnapshot, ProviderFailure};
use taskforest_core::SensorCenterSnapshot;

/// Dynamic thermal, fan, and power-channel observation.
pub trait SensorProvider: Send + 'static {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<SensorCenterSnapshot>, ProviderFailure>;
}
