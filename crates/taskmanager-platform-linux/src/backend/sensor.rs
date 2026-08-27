use taskmanager_application::SensorRequest;
use taskmanager_platform_provider::SensorProvider;
use taskmanager_platform_runtime::{ProviderRegistration, SensorExecutors, SensorProviderBindings};

type SensorRegistration = ProviderRegistration<SensorRequest, Box<dyn SensorProvider>>;

/// Linux sensor provider registration adapted to its independent shared
/// observation executor.
pub struct SensorProviders {
    observation: SensorRegistration,
}

impl SensorProviders {
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
        let mut observation = self.observation.into_provider();
        SensorExecutors::new(move |observed_at_ms| observation.refresh(observed_at_ms))
    }
}
