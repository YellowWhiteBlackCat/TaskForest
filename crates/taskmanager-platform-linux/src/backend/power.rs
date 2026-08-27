use taskmanager_application::PowerSupplyRequest;
use taskmanager_platform_provider::PowerSupplyProvider;
use taskmanager_platform_runtime::{PowerExecutors, PowerProviderBindings, ProviderRegistration};

type PowerRegistration = ProviderRegistration<PowerSupplyRequest, Box<dyn PowerSupplyProvider>>;

/// Linux power-supply provider registration adapted to its independent shared
/// observation executor.
pub struct PowerProviders {
    supplies: PowerRegistration,
}

impl PowerProviders {
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
        let mut supplies = self.supplies.into_provider();
        PowerExecutors::new(move |observed_at_ms| supplies.refresh(observed_at_ms))
    }
}
