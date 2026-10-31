//! Linux power-supply provider implementation.

use taskforest_contract::{DeviceSourceSnapshot, ProviderFailure};
use taskforest_core::PowerSupplySnapshot;

use taskmanager_platform_provider::PowerSupplyProvider;

pub(super) struct NativePowerSupplyProvider;

impl PowerSupplyProvider for NativePowerSupplyProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<PowerSupplySnapshot>, ProviderFailure> {
        Ok(crate::engine::power::collect_power_supplies(observed_at_ms))
    }
}
