use taskforest_contract::{PartialSourceSnapshot, ProviderFailure};
use taskforest_core::{
    ServiceAction, ServiceDeps, ServiceId, ServiceItem, ServiceLogQuery, ServiceLogState,
    ServiceLogStreamState,
};

pub trait ServiceInventoryProvider: Send + 'static {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<ServiceItem>, ProviderFailure>;
}

pub trait ServiceDependenciesProvider: Send + 'static {
    fn dependencies(&mut self, service_id: &ServiceId) -> Result<ServiceDeps, ProviderFailure>;
}

pub trait ServiceControlProvider: Send + 'static {
    fn control(
        &mut self,
        service_id: &ServiceId,
        action: ServiceAction,
    ) -> Result<(), ProviderFailure>;
}

pub trait ServiceLogSnapshotProvider: Send + 'static {
    fn snapshot(&mut self, service_id: &ServiceId) -> Result<ServiceLogState, ProviderFailure>;
}

pub trait ServiceLogStreamProvider: Send + 'static {
    fn stream(
        &mut self,
        query: &ServiceLogQuery,
        observed_at_ms: u64,
    ) -> Result<ServiceLogStreamState, ProviderFailure>;
}
