//! macOS power provider backed by the shared safe battery assembler.

use taskmanager_application::PowerSupplyRequest;
use taskmanager_core::{PowerSupplySnapshot, ProviderId};
use taskmanager_platform_contract::{DeviceSourceSnapshot, ProviderFailure};
use taskmanager_platform_portable::collect_battery_snapshot;
use taskmanager_platform_provider::PowerSupplyProvider;
use taskmanager_platform_runtime::{PowerExecutors, PowerProviderBindings, ProviderRegistration};

const POWER_SUPPLY_CAPABILITY_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.power-supply.registry");

pub struct MacPowerSupplyProvider;

impl PowerSupplyProvider for MacPowerSupplyProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<DeviceSourceSnapshot<PowerSupplySnapshot>, ProviderFailure> {
        collect_battery_snapshot("macos", POWER_SUPPLY_CAPABILITY_PROVIDER, observed_at_ms)
    }
}

pub struct MacPowerProviders {
    supplies: ProviderRegistration<PowerSupplyRequest, Box<dyn PowerSupplyProvider>>,
}

impl MacPowerProviders {
    #[must_use]
    pub fn new<P>(supplies: ProviderRegistration<PowerSupplyRequest, P>) -> Self
    where
        P: PowerSupplyProvider,
    {
        Self {
            supplies: supplies
                .map_provider(|provider| Box::new(provider) as Box<dyn PowerSupplyProvider>),
        }
    }

    pub(crate) fn runtime_bindings(&self) -> PowerProviderBindings {
        PowerProviderBindings::from_registration(&self.supplies)
    }

    pub(crate) fn into_runtime(self) -> PowerExecutors {
        let Self { supplies } = self;
        let mut supplies = supplies.into_provider();
        PowerExecutors::new(move |observed_at_ms| supplies.refresh(observed_at_ms))
    }
}

#[cfg(test)]
#[path = "../../tests/headless/macos_provider_power.rs"]
mod tests;
