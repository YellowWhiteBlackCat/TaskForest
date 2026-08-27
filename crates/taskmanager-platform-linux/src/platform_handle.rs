//! Linux composition from independent capability providers and execution lanes.

use std::time::{SystemTime, UNIX_EPOCH};

use taskmanager_application::PlatformHandle;
use taskmanager_platform_runtime::{
    CompositionError, NativeProviderSet, RuntimeConfig, RuntimeExecutors, RuntimeProviderBindings,
    assemble_native_runtime,
};

use crate::backend::LinuxProviderRegistry;
use crate::provider::real_provider_registry;

mod bindings;

fn wall_clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Composition entry point for the facet-based Linux runtime.
pub struct LinuxPlatformRuntime;

impl LinuxPlatformRuntime {
    pub fn spawn() -> Result<PlatformHandle, CompositionError> {
        Self::spawn_with_providers(real_provider_registry())
    }

    pub fn spawn_with_providers(
        providers: LinuxProviderRegistry,
    ) -> Result<PlatformHandle, CompositionError> {
        spawn_runtime(providers)
    }
}

/// The target-neutral composition name exposed by
/// `taskmanager-platform-native`.
pub struct NativePlatformRuntime;

impl NativePlatformRuntime {
    pub fn spawn() -> Result<PlatformHandle, CompositionError> {
        LinuxPlatformRuntime::spawn()
    }
}

fn spawn_runtime(providers: LinuxProviderRegistry) -> Result<PlatformHandle, CompositionError> {
    assemble_native_runtime(providers, RuntimeConfig::new(wall_clock_ms))
}

impl NativeProviderSet for LinuxProviderRegistry {
    fn runtime_provider_bindings(&self) -> RuntimeProviderBindings {
        bindings::runtime_provider_bindings(self)
    }

    fn into_runtime_executors(self) -> RuntimeExecutors {
        let LinuxProviderRegistry {
            system,
            processes,
            services,
            environment,
            integrations,
            storage,
            sensors,
            power,
        } = self;
        RuntimeExecutors {
            system: system.into_runtime(),
            process: processes.into_runtime(),
            service: services.into_runtime(),
            environment: environment.into_runtime(),
            integration: integrations.into_runtime(),
            storage: storage.into_runtime(),
            sensor: sensors.into_runtime(),
            power: power.into_runtime(),
        }
    }
}
