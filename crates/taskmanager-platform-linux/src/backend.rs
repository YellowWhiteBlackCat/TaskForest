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
pub use process::{ProcessControlProviders, ProcessObservationProviders, ProcessProviders};
mod sensor;
pub use sensor::SensorProviders;
mod service;
pub use service::ServiceProviders;
mod system;
pub use system::{SystemAuxiliaryProviders, SystemObservationProviders, SystemProviders};
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

impl LinuxProviderRegistry {
    // The eight arguments are the eight independent application change axes.
    // Nesting unrelated domains only to satisfy an argument-count heuristic
    // would recreate the aggregate provider bag this registry prevents.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        system: SystemProviders,
        processes: ProcessProviders,
        services: ServiceProviders,
        environment: EnvironmentProviders,
        integrations: IntegrationProviders,
        storage: StorageProviders,
        sensors: SensorProviders,
        power: PowerProviders,
    ) -> Self {
        Self {
            system,
            processes,
            services,
            environment,
            integrations,
            storage,
            sensors,
            power,
        }
    }
}
