//! Platform-neutral service-provider interfaces implemented by native adapters.
//!
//! These traits form the blocking provider boundary below application ports.
//! They are named after independently replaceable product capabilities, never
//! operating systems, commands, hardware vendors, or release variants.
//!
//! Native OS crates own discovery, concrete implementations, error
//! classification, provider registries, and execution lanes. Frontends and the
//! application layer must not depend on this SPI.

#![forbid(unsafe_code)]

mod environment;
mod integration;
mod power;
mod process;
mod sensor;
mod service;
mod storage;
mod system;

pub use environment::{
    SessionControlProvider, SessionInventoryProvider, StartupControlProvider,
    StartupEvidenceProvider, StartupInventoryProvider,
};
pub use integration::{
    CommandLaunchProvider, DesktopAppearanceProvider, DesktopNotificationProvider,
    ResourceRevealProvider, SetupScriptProvider, UrlOpenProvider,
};
pub use power::PowerSupplyProvider;
pub use process::{
    ProcessAffinityControlProvider, ProcessAffinityProvider, ProcessControlProvider,
    ProcessEnvironmentProvider, ProcessGpuProvider, ProcessIsolationProvider, ProcessListProvider,
    ProcessNetworkEscalationProvider, ProcessNetworkProvider, ProcessOpenFilesProvider,
    ProcessResourceControlProvider, ProcessResourcesProvider, ProcessThreadsProvider,
};
pub use sensor::SensorProvider;
pub use service::{
    ServiceControlProvider, ServiceDependenciesProvider, ServiceInventoryProvider,
    ServiceLogSnapshotProvider, ServiceLogStreamProvider,
};
pub use storage::{
    DirectoryUsageProvider, FilesystemHealthProvider, SmartSelfTestControlProvider,
    SmartSelfTestObservationProvider,
};
pub use system::{
    ContainerRollupProvider, CpuTelemetryProvider, GpuEngineRowsProvider, GpuTelemetryProvider,
    HardwareInventoryProvider, HostTelemetryProvider, MemoryTelemetryProvider,
    NetworkTelemetryProvider, NpuInventoryProvider, StorageTelemetryProvider,
};
