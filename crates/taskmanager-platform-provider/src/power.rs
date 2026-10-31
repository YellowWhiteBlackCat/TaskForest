use taskforest_contract::{DeviceSourceSnapshot, ProviderFailure};
use taskforest_core::PowerSupplySnapshot;

/// Dynamic power-supply discovery, including every battery in one OS build.
pub trait PowerSupplyProvider: Send + 'static {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<PowerSupplySnapshot>, ProviderFailure>;
}
