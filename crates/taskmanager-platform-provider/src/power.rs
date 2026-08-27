use taskmanager_core::PowerSupplySnapshot;
use taskmanager_platform_contract::{DeviceSourceSnapshot, ProviderFailure};

/// Dynamic power-supply discovery, including every battery in one OS build.
pub trait PowerSupplyProvider: Send + 'static {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<PowerSupplySnapshot>, ProviderFailure>;
}
