//! Root of the `core` module: aggregates and re-exports all platform-neutral
//! domain submodules (identity, metrics, sensors, process,
//! process-telemetry, alerts, storage, startup, services, and supporting
//! state).

pub mod alerts;
pub mod appearance;
pub mod config;
pub use config::Config;
pub mod cpu_codename;
pub mod cpu_features;
pub mod device_state;
pub mod diagnostics;
pub mod directory_usage;
pub mod export;
pub mod failure;
pub mod hardware;
pub mod history;
pub mod identity;
pub mod kernel_log;
pub mod metrics;
pub mod npu;
pub mod power;
pub mod process;
pub mod process_batch_history;
pub mod process_telemetry;
pub mod sensors;
pub mod services;
pub mod session;
pub mod setup;
pub mod smart;
pub mod source;
pub mod startup;
pub mod storage;
pub mod storage_health;
pub mod system_health;
pub mod target;
pub mod text;
pub mod time;
pub mod tray;
pub mod units;

pub use alerts::{
    ALERT_EVENT_FILE_SCHEMA, ALERT_EVENT_FILE_VERSION, Alert, AlertEngine, AlertEvent,
    AlertEventExportError, AlertEventKind, AlertMetric, AlertRule, AlertSeverity,
    InsufficientReason, MAX_ALERT_EVENTS, RollingStatSnapshot,
    SUGGESTION_CONFIDENCE_HIGH_MIN_SAMPLES, SUGGESTION_MIN_SAMPLES, SUGGESTION_SIGMA_K,
    SuggestedThreshold, SuggestionBasis, SuggestionConfidence, export_alert_events_json,
};
pub use appearance::{DesktopAppearance, DesktopFamily, PreferredColorScheme};
pub use cpu_codename::{CpuVendor, classify_cpu_codename};
pub use cpu_features::CpuInstructionFeature;
pub use device_state::{
    DEFAULT_DEVICE_ABSENCE_RETENTION_MS, DeviceLifecycle, DeviceLifecycleDelta,
    DeviceLifecycleRegistry, DevicePresence, DeviceRefreshOutcome, DeviceState, DeviceStatus,
    StableDeviceSelection,
};
pub use diagnostics::{
    DiagnosticBundleError, DiagnosticBundleErrorKind, DiagnosticBundlePlan, DiagnosticPreview,
    DiagnosticSource, RedactionSummary,
};
pub use directory_usage::{
    DirectoryScanBounds, DirectoryScanControl, DirectoryScanId, DirectoryScanSpec,
    DirectoryScanStatus, DirectoryScanTotals, DirectoryUsageEntry, DirectoryUsageSnapshot,
    MAX_DIRECTORY_SCAN_DEPTH, MAX_DIRECTORY_SCAN_ENTRIES, MAX_DIRECTORY_SCAN_REPORTED,
    report_entries,
};
pub use failure::FailureKind;
pub use hardware::{
    ComputeTopology, CoreBreakdown, CpuIdentity, CpuType, DisplayInfo, DisplayRuntimeInfo,
    FirmwareInfo, HardwareInfo, HostIdentity, KernelInfo, classify_hypervisor_vendor,
};
pub use history::{
    ApplicationHistoryIdentity, HistoricalSample, HistoricalSeries, HistoryMetric,
    HistoryRecordSink, HistorySeriesKey, HistoryWindow, PeakSummary, count_clock_jumps,
};
pub use identity::{DeviceGeneration, DeviceId, ProviderId};
pub use kernel_log::{KernelLogEntry, KernelLogPriority};
pub use metrics::{
    CounterDelta, CpuFrequencySource, CpuIdleState, CpuInterruptSnapshot, CpuMetrics,
    CpuPackageMetrics, CpuPerformancePolicy, CpuScalarObservations, CpuTelemetryObservation,
    CpuTemperatureSource, CumulativeCounter, DiskMetrics, DiskPartition,
    DiskPartitionScalarObservations, DiskScalarObservations, DmiIdentityFacts, GpuEngine,
    GpuEngineKind, GpuEngineMetric, GpuEngineMetricPoint, GpuEngineRowsFailure,
    GpuEngineRowsSnapshot, GpuGraphicsApi, GpuMetricField, GpuMetricProvenance, GpuMetrics,
    GpuScalarObservations, GpuTelemetryObservation, GpuThrottleReason, HostRuntimeFacts,
    HostRuntimeObservation, MAX_TRACKED_LOGICAL_CPUS, MemoryCompositionObservations,
    MemoryCompressionObservations, MemoryMetrics, MemoryModuleObservations,
    MemoryOptionalObservations, MemoryScalarObservations, MemoryTelemetryObservation,
    MsrPackageReadout, MsrReadoutFailure, MsrReadoutSnapshot, NetworkAdapterType, NetworkMetrics,
    NetworkScalarObservations, NetworkTelemetryObservation, NetworkWirelessObservations,
    ObservationWireError, OptionalObservation, OptionalObservationState, PressureWindow,
    ProviderRuntimeState, RaplPackageRow, RaplPowerFailure, RaplPowerSnapshot, ResourcePressure,
    ScalarAvailability, ScalarObservation, ScalarObservationGroup, ScalarObservationSlot,
    SmartAvailability, SmbiosMemoryFailure, SmbiosMemorySnapshot, SmbiosModuleRow,
    StorageTelemetryObservation, SystemLoadAverage, SystemObservationState, SystemPressureSnapshot,
    SystemSnapshot, SystemTelemetryDomains, VirtualMemoryCommitObservations,
    cpu_usage_pct_observation,
};
pub use npu::{
    NpuDevice, NpuEngineKind, NpuEngineUsage, NpuInventoryFailure, NpuInventorySnapshot,
    NpuMemoryReport,
};
pub use power::{
    BatteryInfo, BatteryScalarObservations, PowerSupplyKind, PowerSupplyLifecycleTracker,
    PowerSupplySnapshot,
};
pub use process::{
    ApplicationIconAsset, ApplicationIconFormat, FD_PRESSURE_THRESHOLD, FlatTreeNode,
    FrozenProcessIdentity, MAX_APPLICATION_ICON_BYTES, MEMORY_GROWTH_MIN_SAMPLES, PriorityTier,
    ProcessAnomaly, ProcessAnomalyKind, ProcessApplicationIdentity, ProcessBatchAction,
    ProcessBatchIntent, ProcessBatchResult, ProcessBatchTargetResult, ProcessCategory,
    ProcessGroupScope, ProcessHistorySample, ProcessHistorySnapshot, ProcessHistoryStore,
    ProcessItem, ProcessLiveKey, ProcessMemoryBreakdown, ProcessMetadataAvailability,
    ProcessMetadataFailure, ProcessMetadataObservation, ProcessMetadataObservations, ProcessNode,
    ProcessOwner, ProcessOwnerIdentity, ProcessScalarObservations, ProcessSchedulingPolicy,
    ProcessSignal, ProcessSortKey, ProcessStatusKind, ZOMBIE_STORM_THRESHOLD,
    application_group_name, build_process_tree, compare_process_items, detect_process_anomalies,
    execute_process_batch_with, flatten_tree_visible, fuzzy_filter_processes, fuzzy_match,
    normalize_app_name, process_category, sort_nodes, sort_processes,
};
pub use process_telemetry::{
    CapabilityRiskLevel, ConnectionAddressFamily, ConnectionEndpoint, ConnectionProviderKey,
    ConnectionState, ConnectionTransport, ContainerRollup, ContainerSummary, GpuEngineClass,
    IsolationKind, LimitValue, LinuxCapability, LinuxNamespaceAudit, LinuxNamespaceKind,
    MAX_ENVIRONMENT_BYTES, MAX_ENVIRONMENT_ENTRIES, NamespaceAuditEntry, NamespaceAuditStatus,
    NetworkConnectionCounters, OpenFileEntry, OpenFileItem, OpenFileKind, ProcessCapabilities,
    ProcessConnection, ProcessEnvironment, ProcessEnvironmentEntry, ProcessGpuDevice,
    ProcessGpuEngineClass, ProcessGpuEngineKind, ProcessGpuEngineType, ProcessGpuEngineUsage,
    ProcessGpuEngines, ProcessGpuSnapshot, ProcessIdentity, ProcessInsightSnapshot,
    ProcessIsolation, ProcessNetworkSnapshot, ProcessOpenFiles, ProcessResourceObservations,
    ProcessResourceSnapshot, ProcessTelemetrySnapshot, ProcessThreadInfo, ProcessThreads,
    ResourceGroupCpuLimit, ResourceGroupLimitRequest, ResourceGroupMembership,
    ResourceLastObservation, ResourceLimit, ResourceLimitKind, ResourceObservation, ThreadState,
    ThreadWaitKind, parse_namespace_inode,
};
pub use sensors::{
    SensorCenterSnapshot, SensorDescriptor, SensorLifecycleTracker, SensorMagnitude,
    SensorMeasurementObservation, SensorModelError, SensorQuantity, SensorReading, SensorScale,
    SensorUnit, ThermalControlSnapshot, ThermalCoolingActivity, ThermalCoolingDeviceStatus,
    ThermalCoolingKind, ThermalPolicy, ThermalThrottleSnapshot, ThermalTripKind, ThermalTripPoint,
    ThermalTripPointSet, ThermalZoneMode, ThermalZoneStatus, refresh_sensor_center_state,
};
pub use services::{
    DirectedServiceEdge, ServiceAction, ServiceCycle, ServiceDeps, ServiceDiagnostics,
    ServiceEdgeCategory, ServiceFailureCause, ServiceItem, ServiceLogAvailability,
    ServiceLogEntries, ServiceLogEntry, ServiceLogErrorKind, ServiceLogFailure, ServiceLogFeed,
    ServiceLogLevel, ServiceLogLevelFilter, ServiceLogLines, ServiceLogProviderState,
    ServiceLogQuery, ServiceLogSnapshot, ServiceLogState, ServiceLogStreamEnd,
    ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter, ServiceRelationEdge,
    ServiceRelationGraph, ServiceRelationKind, ServiceStatus, detect_directed_cycles,
    detect_ordering_cycles, detect_requirement_cycles, is_ordering_acyclic, is_requirement_acyclic,
    service_exit_status_label,
};
pub use session::{SessionControlAction, SessionItem};
pub use setup::{SetupScriptAction, SetupScriptEvent, SetupScriptInfo};
pub use smart::self_test::{
    SmartSelfTestFailure, SmartSelfTestKind, SmartSelfTestPhase, SmartSelfTestReport,
};
pub use smart::{AtaSmartAttribute, DiskSmart, SmartProviderFailureKind};
pub use source::{SourceOutcome, SourceStatus};
pub use startup::{
    BootSegmentDelta, BootTimeline, BootTimelineSegment, StartupBootEvidenceSnapshot,
    StartupControlPolicy, StartupCriticalChainNode, StartupEntry, StartupEntryId,
    StartupEntryLocator, StartupEvidenceFailure, StartupFailedUnit, StartupImpact,
    StartupImpactEvidence, StartupImpactUnknownReason, StartupScope, StartupSource, segment_deltas,
};
pub use storage::{
    StorageConnection, StorageDeviceKind, StorageDeviceTarget, StorageIdentityStability,
    StorageInterconnect, StorageProtocol,
};
pub use storage_health::{
    DEFAULT_INODE_USAGE_CRITICAL_PERCENT, DEFAULT_INODE_USAGE_WARNING_PERCENT,
    DEFAULT_NVME_SPARE_CRITICAL_PERCENT, DEFAULT_NVME_SPARE_WARNING_PERCENT,
    DEFAULT_NVME_WEAR_CRITICAL_PERCENT, DEFAULT_NVME_WEAR_WARNING_PERCENT,
    DEFAULT_STORAGE_ALERT_HYSTERESIS_PERCENT, FilesystemBackingKind, FilesystemHealth,
    FilesystemHealthSnapshot, FilesystemHealthStatus, inode_usage_percent, is_nvme_spare_critical,
    is_nvme_spare_warning, is_nvme_wear_critical, is_nvme_wear_warning,
};
pub use system_health::{
    HealthDeduction, HealthDeductionKind, HealthScoreInput, SmartSelfTestIntent,
    SmartSelfTestObservation, SystemHealthScore, SystemHealthSnapshot,
};
pub use target::{ServiceId, SessionId, StorageDeviceKey};
pub use time::{
    LocalDateTime, LocalTimeOffset, LocalTimeRules, LocalTimeRulesCacheKey, LocalTimeRulesChange,
    LocalTimeRulesError, LocalTimeRulesObservation, MAX_LOCAL_TIME_RULE_BYTES, unix_micros,
    unix_millis,
};
pub use tray::{
    MAX_TRAY_ICON_DIMENSION, MAX_TRAY_LABEL_CHARS, MAX_TRAY_MENU_DEPTH, MAX_TRAY_MENU_NODES,
    MAX_TRAY_TITLE_CHARS, MAX_TRAY_TOOLTIP_CHARS, TrayActionId, TrayEvent, TrayIconData,
    TrayIconError, TrayMenuItem, TrayMenuItemKind, TrayMenuSpec, TrayMenuSpecError, TraySpec,
    TraySpecError,
};
pub use units::{
    QuantityFamily, UnitPreferences, format_byte_rate, format_memory, format_quantity,
    format_quantity_f64, format_quantity_pair, format_quantity_with,
};
