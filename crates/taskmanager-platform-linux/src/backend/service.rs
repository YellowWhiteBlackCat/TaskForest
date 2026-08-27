//! Linux service providers bound to shared `ServiceExecutors`.
//!
//! Owns `ServiceProviders`, which adapts inventory, dependencies, control, log
//! snapshot, and log stream registrations into `ServiceProviderBindings`.

use taskmanager_application::{
    ServiceControlRequest, ServiceDependenciesRequest, ServiceInventoryRequest,
    ServiceLogSnapshotRequest, ServiceLogStreamRequest,
};
use taskmanager_platform_provider::{
    ServiceControlProvider, ServiceDependenciesProvider, ServiceInventoryProvider,
    ServiceLogSnapshotProvider, ServiceLogStreamProvider,
};
use taskmanager_platform_runtime::{
    ProviderRegistration, ServiceExecutors, ServiceProviderBindings,
};

type InventoryRegistration =
    ProviderRegistration<ServiceInventoryRequest, Box<dyn ServiceInventoryProvider>>;
type DependenciesRegistration =
    ProviderRegistration<ServiceDependenciesRequest, Box<dyn ServiceDependenciesProvider>>;
type ControlRegistration =
    ProviderRegistration<ServiceControlRequest, Box<dyn ServiceControlProvider>>;
type LogSnapshotRegistration =
    ProviderRegistration<ServiceLogSnapshotRequest, Box<dyn ServiceLogSnapshotProvider>>;
type LogStreamRegistration =
    ProviderRegistration<ServiceLogStreamRequest, Box<dyn ServiceLogStreamProvider>>;

/// Linux service providers adapted to the shared service executors.
pub struct ServiceProviders {
    inventory: InventoryRegistration,
    dependencies: DependenciesRegistration,
    control: ControlRegistration,
    log_snapshot: LogSnapshotRegistration,
    log_stream: LogStreamRegistration,
}

impl ServiceProviders {
    #[must_use]
    pub fn new<I, D, C, L, S>(
        inventory: ProviderRegistration<ServiceInventoryRequest, I>,
        dependencies: ProviderRegistration<ServiceDependenciesRequest, D>,
        control: ProviderRegistration<ServiceControlRequest, C>,
        log_snapshot: ProviderRegistration<ServiceLogSnapshotRequest, L>,
        log_stream: ProviderRegistration<ServiceLogStreamRequest, S>,
    ) -> Self
    where
        I: ServiceInventoryProvider,
        D: ServiceDependenciesProvider,
        C: ServiceControlProvider,
        L: ServiceLogSnapshotProvider,
        S: ServiceLogStreamProvider,
    {
        Self {
            inventory: inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn ServiceInventoryProvider>),
            dependencies: dependencies.map_provider(|provider| {
                Box::new(provider) as Box<dyn ServiceDependenciesProvider>
            }),
            control: control
                .map_provider(|provider| Box::new(provider) as Box<dyn ServiceControlProvider>),
            log_snapshot: log_snapshot
                .map_provider(|provider| Box::new(provider) as Box<dyn ServiceLogSnapshotProvider>),
            log_stream: log_stream
                .map_provider(|provider| Box::new(provider) as Box<dyn ServiceLogStreamProvider>),
        }
    }

    pub(crate) fn runtime_bindings(&self) -> ServiceProviderBindings {
        ServiceProviderBindings::from_registrations(
            &self.inventory,
            &self.dependencies,
            &self.control,
            &self.log_snapshot,
            &self.log_stream,
        )
    }

    pub(crate) fn into_runtime(self) -> ServiceExecutors {
        let Self {
            inventory,
            dependencies,
            control,
            log_snapshot,
            log_stream,
        } = self;
        let mut inventory = inventory.into_provider();
        let mut dependencies = dependencies.into_provider();
        let mut control = control.into_provider();
        let mut log_snapshot = log_snapshot.into_provider();
        let mut log_stream = log_stream.into_provider();
        ServiceExecutors::new(
            move || inventory.refresh(),
            move |service_id| dependencies.dependencies(&service_id),
            move |service_id, action| control.control(&service_id, action),
            move |service_id| log_snapshot.snapshot(&service_id),
            move |query, observed_at_ms| log_stream.stream(&query, observed_at_ms),
        )
    }
}
