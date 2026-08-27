//! Stable model surface consumed by frontends and platform adapters.
//!
//! The model is implemented by `taskmanager-core`, but presentation crates
//! deliberately import it through this application boundary. This keeps UI
//! dependency graphs independent from the model's physical ownership.

pub use taskmanager_core::core::config::{ColumnWidthConfig, ProcessViewPresetConfig};
pub use taskmanager_core::{
    BatteryInfo, BatteryScalarObservations, Config, ConnectionAddressFamily, ConnectionEndpoint,
    ConnectionProviderKey, ConnectionState, ConnectionTransport, ContainerRollup, ContainerSummary,
    CpuFrequencySource, CpuMetrics, CpuPerformancePolicy, CpuScalarObservations,
    CpuTelemetryObservation, CpuTemperatureSource, DesktopAppearance, DesktopFamily,
    DeviceLifecycle, DevicePresence, DeviceState, DeviceStatus, DiskMetrics, DisplayInfo,
    DisplayRuntimeInfo, FilesystemHealthSnapshot, FlatTreeNode, FrozenProcessIdentity, GpuEngine,
    GpuEngineKind, GpuEngineMetric, GpuEngineMetricPoint, GpuEngineRowsFailure,
    GpuEngineRowsSnapshot, GpuMetrics, GpuScalarObservations, GpuTelemetryObservation,
    GpuThrottleReason, HardwareInfo, HostRuntimeFacts, HostRuntimeObservation, IsolationKind,
    LimitValue, MemoryCompositionObservations, MemoryCompressionObservations, MemoryMetrics,
    MemoryModuleObservations, MemoryOptionalObservations, MemoryScalarObservations,
    MemoryTelemetryObservation, NetworkAdapterType, NetworkMetrics, NetworkScalarObservations,
    NetworkTelemetryObservation, NetworkWirelessObservations, NpuDevice, NpuEngineKind,
    NpuEngineUsage, NpuInventoryFailure, NpuInventorySnapshot, NpuMemoryReport, OpenFileEntry,
    OpenFileKind, OptionalObservation, OptionalObservationState, PowerSupplyKind,
    PowerSupplySnapshot, PreferredColorScheme, PriorityTier, ProcessBatchAction,
    ProcessBatchIntent, ProcessBatchResult, ProcessBatchTargetResult, ProcessConnection,
    ProcessEnvironment, ProcessEnvironmentEntry, ProcessGpuDevice, ProcessGpuSnapshot,
    ProcessGroupScope, ProcessIdentity, ProcessInsightSnapshot, ProcessIsolation, ProcessItem,
    ProcessNetworkSnapshot, ProcessNode, ProcessOpenFiles, ProcessResourceSnapshot, ProcessSignal,
    ProcessSortKey, ProcessTelemetrySnapshot, ProcessThreadInfo, ProcessThreads,
    ProviderRuntimeState, ResourceGroupCpuLimit, ResourceGroupLimitRequest,
    ResourceGroupMembership, ResourceLimit, ResourceLimitKind, ScalarAvailability,
    ScalarObservation, ScalarObservationGroup, ScalarObservationSlot, SensorCenterSnapshot,
    SensorDescriptor, SensorMagnitude, SensorMeasurementObservation, SensorQuantity, SensorReading,
    SensorScale, ServiceAction, ServiceDeps, ServiceId, ServiceItem, ServiceLogAvailability,
    ServiceLogEntries, ServiceLogEntry, ServiceLogErrorKind, ServiceLogFailure, ServiceLogFeed,
    ServiceLogLevel, ServiceLogLevelFilter, ServiceLogLines, ServiceLogProviderState,
    ServiceLogQuery, ServiceLogSnapshot, ServiceLogState, ServiceLogStreamEnd,
    ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter, ServiceRelationEdge,
    ServiceRelationGraph, ServiceRelationKind, ServiceStatus, SessionControlAction, SessionId,
    SessionItem, SmartAvailability, SmartSelfTestIntent, SmartSelfTestKind,
    SmartSelfTestObservation, StartupBootEvidenceSnapshot, StartupControlPolicy, StartupEntry,
    StartupEntryId, StartupEntryLocator, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource, StorageConnection, StorageDeviceKey,
    StorageDeviceKind, StorageDeviceTarget, StorageIdentityStability, StorageInterconnect,
    StorageProtocol, StorageTelemetryObservation, SystemHealthSnapshot, SystemObservationState,
    SystemSnapshot, SystemTelemetryDomains, ThermalControlSnapshot, ThermalCoolingActivity,
    ThermalCoolingDeviceStatus, ThermalCoolingKind, ThermalPolicy, ThermalThrottleSnapshot,
    ThermalTripKind, ThermalTripPoint, ThermalTripPointSet, ThermalZoneMode, ThermalZoneStatus,
    ThreadState, VirtualMemoryCommitObservations, build_process_tree, compare_process_items,
    flatten_tree_visible, sort_apps, sort_nodes,
};

// Boot-timeline types (BN-05) are exposed through the same application
// boundary for the firewalled frontends (ADR-020): the TUI/Iced may not
// depend on taskmanager-core, so the projection vocabulary they render is
// re-exported here.
pub use taskmanager_core::core::startup::{
    BootTimeline, BootTimelineSegment, DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS,
    DEFAULT_BOOT_TIMELINE_MAX_UNTIMED, StartupCriticalChainNode, StartupEvidenceFailure,
    StartupFailedUnit,
};

// Directory-usage scan vocabulary (BN-01) is exposed through the same
// application boundary for the firewalled frontends (ADR-020): the TUI/Iced
// may not depend on taskmanager-core, so the snapshot types they render and
// fixture in headless tests are re-exported here. `DirectoryUsageEvent` itself
// lives in this crate (`platform/facets/directory_usage.rs`) and is already
// re-exported from `lib.rs`.
pub use taskmanager_core::{
    DirectoryScanBounds, DirectoryScanId, DirectoryScanSpec, DirectoryScanStatus,
    DirectoryScanTotals, DirectoryUsageEntry, DirectoryUsageSnapshot,
};
