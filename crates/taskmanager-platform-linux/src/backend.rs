//! Linux construction registry for platform-neutral capability providers.
//!
//! Provider interfaces live in `taskmanager-platform-provider` so another OS
//! adapter implements the same capability vocabulary without depending on
//! Linux. This module only groups concrete providers for Linux construction;
//! the groups never imply a shared execution lane.

mod environment;
pub use environment::EnvironmentProviders;
mod integration;
pub use integration::IntegrationProviders;
mod power;
pub use power::PowerProviders;
mod process;
pub use process::{
    ProcessControlProviders, ProcessObservationProviders, ProcessObservationProvidersParams,
    ProcessProviders,
};
mod sensor;
pub use sensor::SensorProviders;
mod service;
pub use service::ServiceProviders;
mod system;
pub use system::{
    SystemAuxiliaryProviders, SystemAuxiliaryProvidersParams, SystemObservationProviders,
    SystemProviders,
};
mod storage;
pub use storage::StorageProviders;

/// Runtime registry consumed into isolated execution lanes.
///
/// A registry contains capability implementations, not a product SKU. Hardware
/// and optional system services remain runtime-discovered inside providers.
pub struct LinuxProviderRegistry {
    pub(crate) system: SystemProviders,
    pub(crate) processes: ProcessProviders,
    pub(crate) services: ServiceProviders,
    pub(crate) environment: EnvironmentProviders,
    pub(crate) integrations: IntegrationProviders,
    pub(crate) storage: StorageProviders,
    pub(crate) sensors: SensorProviders,
    pub(crate) power: PowerProviders,
}

/// Named composition input for [`LinuxProviderRegistry`].
///
/// Each field is one independently scheduled provider domain consumed once into
/// its own execution lane. The names keep the eight application change axes
/// explicit without a positional argument list, and the input stays a set of
/// domain groups rather than one aggregate provider bag.
pub struct LinuxProviderRegistryParams {
    /// System telemetry and hardware inventory providers.
    pub system: SystemProviders,
    /// Process observation and control providers.
    pub processes: ProcessProviders,
    /// Service inventory, dependency, control, and log providers.
    pub services: ServiceProviders,
    /// Startup and session providers.
    pub environment: EnvironmentProviders,
    /// Shell and desktop integration providers.
    pub integrations: IntegrationProviders,
    /// Storage health and SMART providers.
    pub storage: StorageProviders,
    /// Sensor center providers.
    pub sensors: SensorProviders,
    /// Power supply providers.
    pub power: PowerProviders,
}

impl LinuxProviderRegistry {
    #[must_use]
    pub fn new(params: LinuxProviderRegistryParams) -> Self {
        Self {
            system: params.system,
            processes: params.processes,
            services: params.services,
            environment: params.environment,
            integrations: params.integrations,
            storage: params.storage,
            sensors: params.sensors,
            power: params.power,
        }
    }
}
