//! Reusable provider-to-application execution mechanics for native adapters.
//!
//! This crate owns bounded typed request lanes, correlated event publication,
//! capability health, and fair control/observation multiplexing. Native adapter
//! crates inject provider identities, clocks, and provider execution closures;
//! operating-system paths, commands, registries, and hardware features do not
//! belong here. Wall-clock observation timestamps and monotonic lifecycle
//! scheduling are separate runtime authorities.

#![forbid(unsafe_code)]

mod ecs;

mod absent;
pub use absent::capability_absent_handle;
mod assembly;
pub use assembly::{NativeProviderSet, RuntimeExecutors, assemble_native_runtime};
mod channel;
pub use channel::{
    ChannelRuntime, Queued, RuntimeBudgetField, RuntimeConstructionError, RuntimeLanes,
};
mod composition;
pub use composition::{CompleteChannelRuntime, CompleteRuntimeLanes, CompositionError};
mod config;
pub use config::{
    EnvironmentProviderBindings, IntegrationProviderBindings, PowerProviderBindings,
    ProcessProviderBindings, ProcessProviderBindingsInput, QueueCapacities, RuntimeBudgets,
    RuntimeConfig, RuntimeProviderBindings, SensorProviderBindings, ServiceProviderBindings,
    StorageProviderBindings, SystemProviderBindings, SystemProviderBindingsInput,
    monotonic_clock_ms,
};
mod delivery;
pub use delivery::{
    DEFAULT_WORKER_LIMIT, PROCESS_WORKER_LIMIT, RuntimeEventPublisher, WorkerRuntime,
    WorkerSpawnError, spawn_health_observation_lane, spawn_lane, spawn_observation_lane,
    spawn_typed_outcome_lane,
};
pub(crate) use delivery::{
    spawn_lazy_health_observation_lane, spawn_lazy_lane, spawn_lazy_observation_lane,
    spawn_lazy_typed_outcome_lane,
};
mod environment;
pub use environment::{
    EnvironmentExecutors, EnvironmentRuntimeLanes, PendingEnvironmentRuntimeLanes,
    spawn_environment_lanes,
};
mod health;
pub use health::{
    CapabilityHealth, ObservationHealth, degraded_health, device_source_health,
    device_state_health, source_health,
};
mod integration;
pub use integration::{
    IntegrationExecutors, IntegrationRuntimeLanes, PendingIntegrationRuntimeLanes,
    spawn_integration_lanes,
};
mod lifecycle;
pub use lifecycle::discovery_refresh_outcome;
mod power;
pub use power::{PendingPowerRuntimeLanes, PowerExecutors, PowerRuntimeLanes, spawn_power_lanes};
mod process;
pub use process::{
    PendingProcessControlLanes, PendingProcessObservationLanes, PendingProcessRuntimeLanes,
    ProcessControlCompletion, ProcessControlExecutors, ProcessExecutors,
    ProcessObservationExecutors, ProcessRuntimeLanes, spawn_process_lanes,
};
mod registration;
pub use registration::{ProviderBinding, ProviderRegistration};
mod sensor;
pub use sensor::{
    PendingSensorRuntimeLanes, SensorExecutors, SensorRuntimeLanes, spawn_sensor_lanes,
};
mod service;
pub use service::{
    PendingServiceRuntimeLanes, ServiceExecutors, ServiceRuntimeLanes, spawn_service_lanes,
};
mod storage;
pub use storage::{
    DEFAULT_SMART_JOB_RETENTION_MS, MAX_TRACKED_SMART_JOBS, PendingStorageRuntimeLanes,
    SharedSmartRuntimeState, SmartCommitStatus, SmartInstallResult, SmartJobSnapshot,
    SmartJobToken, SmartStateSnapshot, SmartTargetKey, StorageExecutors, StorageRuntimeLanes,
    spawn_storage_lanes,
};
mod system;
pub use system::{
    PendingSystemAuxiliaryLanes, PendingSystemObservationLanes, PendingSystemRuntimeLanes,
    SystemAuxiliaryExecutors, SystemExecutors, SystemObservationExecutors, SystemRuntimeLanes,
    spawn_system_lanes,
};
