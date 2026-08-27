//! Registration-derived Windows bindings for platform-neutral runtime
//! capabilities.

use taskmanager_platform_runtime::RuntimeProviderBindings;

use crate::provider::WindowsProviderRegistry;

pub(super) fn runtime_provider_bindings(
    providers: &WindowsProviderRegistry,
) -> RuntimeProviderBindings {
    RuntimeProviderBindings {
        system: providers.system.runtime_bindings(),
        process: providers.processes.runtime_bindings(),
        service: providers.services.runtime_bindings(),
        environment: providers.environment.runtime_bindings(),
        integration: providers.integrations.runtime_bindings(),
        storage: providers.storage.runtime_bindings(),
        sensor: providers.sensors.runtime_bindings(),
        power: providers.power.runtime_bindings(),
    }
}
