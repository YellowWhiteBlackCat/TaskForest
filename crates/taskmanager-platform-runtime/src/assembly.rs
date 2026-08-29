//! Shared native-adapter assembly over application-typed runtime executors.
//!
//! Native crates keep provider discovery, provider SPI adaptation, identities,
//! bindings, clocks, and OS policy. This module owns only the repeated final
//! promotion from a complete provider set into the eight bounded runtime lane
//! groups and their common worker lifetime.

use std::sync::Arc;

use taskmanager_application::PlatformHandle;

use crate::{
    ChannelRuntime, CompleteChannelRuntime, CompleteRuntimeLanes, CompositionError,
    EnvironmentExecutors, IntegrationExecutors, PowerExecutors, ProcessExecutors, RuntimeConfig,
    RuntimeProviderBindings, SensorExecutors, ServiceExecutors, StorageExecutors, SystemExecutors,
    WorkerRuntime, spawn_environment_lanes, spawn_integration_lanes, spawn_power_lanes,
    spawn_process_lanes, spawn_sensor_lanes, spawn_service_lanes, spawn_storage_lanes,
    spawn_system_lanes,
};

/// The eight application-domain executor groups produced by one native
/// provider registry.
///
/// These closures already carry the provider-SPI adaptation owned by the OS
/// crate. The shared runtime therefore never imports provider traits or native
/// implementations.
pub struct RuntimeExecutors {
    pub system: SystemExecutors,
    pub process: ProcessExecutors,
    pub service: ServiceExecutors,
    pub environment: EnvironmentExecutors,
    pub integration: IntegrationExecutors,
    pub storage: StorageExecutors,
    pub sensor: SensorExecutors,
    pub power: PowerExecutors,
}

/// Native provider-registry seam consumed by the shared final assembly.
///
/// Implementations remain in the OS crates. Borrowing first derives the
/// provider bindings from the exact registrations; consuming second converts
/// those same registrations into executor closures, preserving ADR-008's
/// single provider-identity authority.
pub trait NativeProviderSet: Sized {
    fn runtime_provider_bindings(&self) -> RuntimeProviderBindings;
    fn into_runtime_executors(self) -> RuntimeExecutors;
}

/// Promote one native provider set into a complete runtime and spawn every
/// bounded lane under one worker owner.
pub fn assemble_native_runtime(
    providers: impl NativeProviderSet,
    config: RuntimeConfig,
) -> Result<PlatformHandle, CompositionError> {
    let provider_bindings = providers.runtime_provider_bindings();
    let CompleteChannelRuntime {
        handle,
        publisher: events,
        lanes,
        lane_starters,
    } = ChannelRuntime::try_new(provider_bindings, config)
        .map_err(CompositionError::runtime_construction)?
        .try_complete()?;
    let CompleteRuntimeLanes {
        system,
        process,
        service,
        environment,
        integration,
        storage,
        sensor,
        power,
    } = lanes;
    let executors = providers.into_runtime_executors();
    let workers = Arc::new(WorkerRuntime::default());
    lane_starters.bind_worker(&workers);
    workers.install_lane_starters(lane_starters);

    spawn_system_lanes(
        workers.as_ref(),
        system,
        executors.system,
        events.clone(),
        config.clock_ms,
    )
    .map_err(CompositionError::worker_spawn)?;
    spawn_process_lanes(
        workers.as_ref(),
        process,
        executors.process,
        events.clone(),
        config.clock_ms,
    )
    .map_err(CompositionError::worker_spawn)?;
    spawn_service_lanes(
        workers.as_ref(),
        service,
        executors.service,
        events.clone(),
        config.clock_ms,
    )
    .map_err(CompositionError::worker_spawn)?;
    spawn_environment_lanes(
        workers.as_ref(),
        environment,
        executors.environment,
        events.clone(),
        config.clock_ms,
    )
    .map_err(CompositionError::worker_spawn)?;
    spawn_integration_lanes(
        workers.as_ref(),
        integration,
        executors.integration,
        events.clone(),
    )
    .map_err(CompositionError::worker_spawn)?;
    spawn_storage_lanes(
        workers.as_ref(),
        storage,
        executors.storage,
        events.clone(),
        config.clock_ms,
    )
    .map_err(CompositionError::worker_spawn)?;
    spawn_sensor_lanes(
        workers.as_ref(),
        sensor,
        executors.sensor,
        events.clone(),
        config.clock_ms,
    )
    .map_err(CompositionError::worker_spawn)?;
    spawn_power_lanes(
        workers.as_ref(),
        power,
        executors.power,
        events,
        config.clock_ms,
    )
    .map_err(CompositionError::worker_spawn)?;

    Ok(handle.with_runtime_lifetime(workers))
}
