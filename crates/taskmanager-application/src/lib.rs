//! Toolkit-neutral application commands, state transitions, and platform ports.

#![forbid(unsafe_code)]

mod action;
mod alert_center;
mod alert_dispatch;
pub use alert_center::{AlertCenter, AlertEvaluation};
pub use alert_dispatch::AlertDispatcher;

/// Maximum number of container summaries a frontend materializes in its
/// presentation list. The domain rollup remains complete; this shared cap
/// keeps every renderer's widget/row cost bounded and exposes the omitted
/// count explicitly.
pub const MAX_CONTAINER_ROWS: usize = 200;

/// Split a container inventory into the materialized prefix and the explicit
/// overflow count used by all frontends.
#[must_use]
pub const fn container_row_window(total: usize) -> (usize, usize) {
    let shown = if total < MAX_CONTAINER_ROWS {
        total
    } else {
        MAX_CONTAINER_ROWS
    };
    (shown, total - shown)
}
mod alert_suggestion_window;
mod application_history_projection;
mod boot_baseline;
/// Toolkit-neutral history-series decimation kernels (LTTB run selection and
/// the stride max-envelope) — the single source every frontend's replay and
/// pixel-budgeted downsampling delegates to.
pub mod history_decimation;
mod history_replay;
mod persistent_app_history;
/// Frontends reach the alert engine (rules, transfer, and the rolling-statistic
/// threshold suggestions) only through this facade — the workspace dependency
/// firewall forbids a direct `taskmanager-core` dependency in any frontend, so
/// the alert module is re-exported here alongside the rest of the core surface.
pub use taskmanager_core::core::alerts;
pub use taskmanager_core::core::hardware::{CoreBreakdown, CpuType};
pub use taskmanager_core::core::text;
pub use taskmanager_core::core::{
    ApplicationHistoryIdentity, DiskPartition, DiskPartitionScalarObservations,
    DiskScalarObservations, HistoricalSample, HistoryMetric, HistoryRecordSink, HistorySeriesKey,
    HistoryWindow, LocalDateTime, LocalTimeOffset, LocalTimeRules, LocalTimeRulesCacheKey,
    LocalTimeRulesChange, LocalTimeRulesError, LocalTimeRulesObservation,
};
// Neutral system-tray vocabulary (spec, menu, icon, events) shared by every
// frontend and OS adapter. Re-exported here because the workspace dependency
// firewall forbids a direct `taskmanager-core` dep in the TUI.
pub use taskmanager_core::core::tray;
// Neutral unit-preference formatting (bytes/bits × base-2/base-10 ladder):
// the single source the firewalled frontends (TUI/Iced) render quantities
// through, re-exported here like `alerts`/`text`/`tray` above.
pub use taskmanager_core::core::units;
// Per-application grouping primitives shared by the App-history foundation and
// by frontends that render app rows. Re-exported here because the workspace
// dependency firewall forbids a direct `taskmanager-core` dep in any frontend.
pub use taskmanager_core::core::process::{
    AppGroup, ProcessApplicationIdentity, ProcessCategory, ProcessMetadataObservation,
    ProcessMetadataObservations, ProcessOwner, ProcessOwnerIdentity, ProcessScalarObservations,
    ProcessType, aggregate_apps, aggregate_by_type, application_group_name, classify_process_type,
    descendant_pids, process_category, process_type_label,
};
mod command;
mod config_runtime;
mod config_store;
mod control;
mod device_lifecycle;
mod diagnostics;
/// Self-contained i18n (embedded locale catalogs + `t`). Lives in this shared
/// crate so every frontend (gpui/tui/iced) can consume `taskmanager_application::i18n`;
/// the root crate re-exports it for existing `crate::i18n` call sites.
pub mod i18n;
mod interaction;
mod managed_alert_rules;
mod model;
mod platform;
mod ports;
/// Neutral category-bucket projection shared by every frontend's canonical
/// process hierarchy (bucket order, empty-bucket omission, member order,
/// aggregate helpers, and the locale-neutral expansion key).
pub mod process_category_projection;
/// Neutral process-details row ViewModel: the single fold from a typed
/// `ProcessItem` to the label/value rows every frontend's details panel and
/// properties dialog renders (field vocabulary, missing semantics, unit and
/// time formats).
pub mod process_details_vm;
pub mod process_resource_projection;
pub use process_resource_projection::{ProjectedProcessResources, project_process_resources};
/// Neutral process-table sort comparator shared by every frontend: the single
/// source of axis semantics (missing-value handling, case folding, direction,
/// pid tie-break) that the shell and GPUI column enums delegate to.
pub mod process_sort;
mod reducer;
mod refresh;
mod request_session;
mod router;
mod service_lifecycle;
pub mod snapshot_export;
mod source_status;
mod telemetry_refresh_policy;

pub use action::{AppAction, AppPage, FocusDirection, SelectionDirection};
pub use alert_suggestion_window::{AlertSuggestionWindow, DEFAULT_SUGGESTION_WINDOW_CAPACITY};
pub use application_history_projection::{
    ApplicationHistoryCapability, ApplicationHistoryMetricSeries, ApplicationHistoryProjection,
    ApplicationHistoryRow, ApplicationHistoryStatus, ApplicationHistoryUnavailableReason,
};
pub use boot_baseline::{
    BootBaselineCompletion, BootBaselineCompletionDisposition, BootBaselineCompletionOutcome,
    BootBaselineController, BootBaselineError, BootBaselineErrorKind, BootBaselineReady,
    BootBaselineRecordKind, BootBaselineRequest, BootBaselineRequestId, BootBaselineState,
    BootBaselineSubmission,
};
pub use command::{CommandId, KeyChord, KeyCode, KeyParseError, Modifiers};
pub use config_runtime::{
    ConfigBootstrap, ConfigBootstrapFallback, ConfigClient, ConfigCoordinator, ConfigDrain,
    ConfigPublication, ConfigPublicationId, ConfigPublicationOutcome, ConfigRecovery,
    ConfigRecoveryNotice, ConfigRevision, ConfigRuntimeMonitor, ConfigRuntimeOptions,
    ConfigRuntimeStartError, ConfigSubmissionStatus, ConfigSubmitError, ConfigSynchronizeError,
    ConfigWorkerState, DEFAULT_CONFIG_COMMAND_CAPACITY, DEFAULT_CONFIG_INITIAL_WAIT,
    DEFAULT_CONFIG_PUBLICATION_CAPACITY, DEFAULT_CONFIG_REFRESH_INTERVAL,
};
pub use config_store::{
    ConfigLoadResult, ConfigLoadSource, ConfigStore, ConfigStoreError, ConfigStoreErrorKind,
    MAX_CONFIG_BYTES,
};
pub use control::{
    ControlRequestId, LatestControlRequest, LatestServiceControlRequest, ServiceControlOutcome,
    SessionControlOutcome, SessionControlRequest, StartupControlOutcome, StartupControlRequest,
};
pub use device_lifecycle::{
    DeviceLifecycleApplyResult, DeviceLifecycleChange, DeviceLifecycleChangeKind,
    DeviceLifecycleDiagnosticHistory, DeviceLifecyclePartition, DeviceLifecycleProjection,
    DeviceLifecycleProjectionDelta, DeviceLifecycleProjectionIssue,
    DeviceLifecycleSnapshotRejection, DeviceLifecycleSnapshotRevision, DeviceLifecycleViewState,
    ProjectedDeviceLifecycle,
};
pub use diagnostics::{
    DiagnosticBundleCompletion, DiagnosticBundlePort, DiagnosticBundleRequest,
    DiagnosticBundleRequestId, DiagnosticBundleSession, DiagnosticBundleTarget,
    prepare_service_log_bundle,
};
pub use history_replay::{
    HistoryReplayCompletion, HistoryReplayCompletionDisposition, HistoryReplayCompletionOutcome,
    HistoryReplayController, HistoryReplayError, HistoryReplayErrorKind, HistoryReplayReady,
    HistoryReplayRequest, HistoryReplayRequestId, HistoryReplayRow, HistoryReplayState,
    HistoryReplayTransitionError, MAX_HISTORY_REPLAY_ERROR_CHARS, MAX_HISTORY_REPLAY_POINTS,
};
pub use interaction::{
    ConfirmationKind, InteractionEvent, InteractionReduction, InteractionState,
    PendingConfirmation, ProcessTerminationAction, ProcessTerminationConfirmation,
    SessionControlConfirmation, SurfaceDismissReason, SurfaceKind, SurfaceTransition,
};
pub use managed_alert_rules::{
    AlertRuleImportMode, ManagedAlertRule, ManagedAlertRuleEdit, ManagedAlertRuleEditOutcome,
};
pub use model::{
    BatteryInfo, BatteryScalarObservations, BootTimeline, BootTimelineSegment, Config,
    ConnectionAddressFamily, ConnectionEndpoint, ConnectionProviderKey, ConnectionState,
    ConnectionTransport, ContainerRollup, ContainerSummary, CpuFrequencySource, CpuMetrics,
    CpuPerformancePolicy, CpuScalarObservations, CpuTelemetryObservation, CpuTemperatureSource,
    DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS, DEFAULT_BOOT_TIMELINE_MAX_UNTIMED, DesktopAppearance,
    DesktopFamily, DeviceLifecycle, DevicePresence, DeviceState, DeviceStatus, DirectoryScanBounds,
    DirectoryScanId, DirectoryScanSpec, DirectoryScanStatus, DirectoryScanTotals,
    DirectoryUsageEntry, DirectoryUsageSnapshot, DiskMetrics, DisplayInfo, DisplayRuntimeInfo,
    FilesystemHealthSnapshot, FlatTreeNode, FrozenProcessIdentity, GpuEngine, GpuEngineKind,
    GpuEngineMetric, GpuEngineMetricPoint, GpuEngineRowsFailure, GpuEngineRowsSnapshot, GpuMetrics,
    GpuScalarObservations, GpuTelemetryObservation, GpuThrottleReason, HardwareInfo,
    HostRuntimeFacts, HostRuntimeObservation, IsolationKind, LimitValue,
    MemoryCompositionObservations, MemoryCompressionObservations, MemoryMetrics,
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
    SmartSelfTestObservation, StartupBootEvidenceSnapshot, StartupControlPolicy,
    StartupCriticalChainNode, StartupEntry, StartupEntryId, StartupEntryLocator,
    StartupEvidenceFailure, StartupFailedUnit, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource, StorageConnection, StorageDeviceKey,
    StorageDeviceKind, StorageDeviceTarget, StorageIdentityStability, StorageInterconnect,
    StorageProtocol, StorageTelemetryObservation, SystemHealthSnapshot, SystemObservationState,
    SystemSnapshot, SystemTelemetryDomains, ThermalControlSnapshot, ThermalCoolingActivity,
    ThermalCoolingDeviceStatus, ThermalCoolingKind, ThermalPolicy, ThermalThrottleSnapshot,
    ThermalTripKind, ThermalTripPoint, ThermalTripPointSet, ThermalZoneMode, ThermalZoneStatus,
    ThreadState, VirtualMemoryCommitObservations, build_process_tree, compare_process_items,
    flatten_tree_visible, sort_apps, sort_nodes,
};
pub use model::{ColumnWidthConfig, ProcessViewPresetConfig};
pub use persistent_app_history::{
    MAX_PERSISTED_APPLICATION_IDENTITIES, PersistentApplicationHistoryRecorder,
    PersistentApplicationRecordReport,
};
pub use platform::apply_system_outcome_lifecycle;
pub use platform::boot_timeline_rows;
pub use platform::{
    CommandLaunchRequest, CommandLaunchRequestPort, ContainerRollupEvent, ContainerRollupRequest,
    ContainerRollupRequestPort, CorrelatedContainerRollupEvent, CorrelatedDesktopAppearanceEvent,
    CorrelatedDirectoryUsageEvent, CorrelatedEvent, CorrelatedGpuEngineRowsEvent,
    CorrelatedHardwareInventoryEvent, CorrelatedNpuInventoryEvent, CorrelatedPowerSupplyEvent,
    CorrelatedProcessAffinityEvent, CorrelatedProcessEvent, CorrelatedSensorEvent,
    CorrelatedServiceEvent, CorrelatedSessionEvent, CorrelatedSetupScriptEvent,
    CorrelatedShellEvent, CorrelatedSmartEvent, CorrelatedStartupEvent,
    CorrelatedStorageHealthEvent, CorrelatedSystemTelemetryOutcome, CpuTelemetryRequest,
    CpuTelemetryRequestPort, DesktopAppearanceEvent, DesktopAppearanceRequest,
    DesktopAppearanceRequestPort, DesktopNotificationRequest, DesktopNotificationRequestPort,
    DirectoryUsageEvent, DirectoryUsageRequest, DirectoryUsageRequestPort, EnvironmentFacets,
    GpuEngineRowsEvent, GpuEngineRowsRequest, GpuEngineRowsRequestPort, GpuTelemetryRequest,
    GpuTelemetryRequestPort, HardwareInventoryEvent, HardwareInventoryRequest,
    HardwareInventoryRequestPort, HostTelemetryRequest, HostTelemetryRequestPort,
    IntegrationFacets, MemoryTelemetryRequest, MemoryTelemetryRequestPort, NetworkTelemetryRequest,
    NetworkTelemetryRequestPort, NpuInventoryEvent, NpuInventoryRequest, NpuInventoryRequestPort,
    PlatformClient, PlatformEvent, PlatformEventBatch, PlatformEventContext, PlatformEventPort,
    PlatformFacets, PlatformHandle, PowerFacets, PowerSupplyEvent, PowerSupplyRequest,
    PowerSupplyRequestPort, ProcessAffinityControlRequest, ProcessAffinityControlRequestPort,
    ProcessAffinityEvent, ProcessAffinityRequest, ProcessAffinityRequestPort,
    ProcessControlRequest, ProcessControlRequestPort, ProcessEnvironmentRequest,
    ProcessEnvironmentRequestPort, ProcessEvent, ProcessFacets, ProcessGpuRequest,
    ProcessGpuRequestPort, ProcessInsightFacet, ProcessInsightFacetEvent, ProcessInsightFacetState,
    ProcessInsightObservation, ProcessInsightUnavailable, ProcessInsightsProjection,
    ProcessInsightsProjectionApplyResult, ProcessInsightsProjectionRejection,
    ProcessInsightsRevision, ProcessInsightsSubmission, ProcessInsightsSubmissionError,
    ProcessIsolationRequest, ProcessIsolationRequestPort, ProcessListRequest,
    ProcessListRequestPort, ProcessNetworkEscalationRequest, ProcessNetworkEscalationRequestPort,
    ProcessNetworkRequest, ProcessNetworkRequestPort, ProcessOpenFilesRequest,
    ProcessOpenFilesRequestPort, ProcessResourceControlRequest, ProcessResourceControlRequestPort,
    ProcessResourcesRequest, ProcessResourcesRequestPort, ProcessThreadsRequest,
    ProcessThreadsRequestPort, ProjectedProcessInsights, ProjectedStartupEvidence,
    ProjectedSystemTelemetry, ProjectionAcceptance, ResourceRevealRequest,
    ResourceRevealRequestPort, SensorEvent, SensorFacets, SensorRequest, SensorRequestPort,
    ServiceControlRequest, ServiceControlRequestPort, ServiceDependenciesRequest,
    ServiceDependenciesRequestPort, ServiceEvent, ServiceFacets, ServiceInventoryRequest,
    ServiceInventoryRequestPort, ServiceLogSnapshotRequest, ServiceLogSnapshotRequestPort,
    ServiceLogStreamRequest, ServiceLogStreamRequestPort, SessionControlRequestPort, SessionEvent,
    SessionInventoryRequest, SessionInventoryRequestPort, SetupScriptAction, SetupScriptEvent,
    SetupScriptInfo, SetupScriptRequest, SetupScriptRequestPort, ShellEvent, SmartControlRequest,
    SmartControlRequestPort, SmartEvent, SmartObservationBatch, SmartObservationIssue,
    SmartObservationProjection, SmartObservationRequest, SmartObservationRequestPort,
    SmartProjectionApplyResult, SmartStateRevision, SmartTrackingEnd, SmartTrackingEndReason,
    StartupControlRequestPort, StartupEvent, StartupEvidenceEvent, StartupEvidenceProjection,
    StartupEvidenceProjectionApplyResult, StartupEvidenceProjectionRejection,
    StartupEvidenceRequest, StartupEvidenceRequestPort, StartupEvidenceRevision,
    StartupEvidenceUnavailable, StartupInventoryRequest, StartupInventoryRequestPort,
    StorageFacets, StorageHealthEvent, StorageHealthRequest, StorageHealthRequestPort,
    StorageTelemetryRequest, StorageTelemetryRequestPort, SystemFacets, SystemTelemetryDomain,
    SystemTelemetryDomainEvent, SystemTelemetryDomainOutcome, SystemTelemetryDomainState,
    SystemTelemetryProjection, SystemTelemetryProjectionApplyResult,
    SystemTelemetryProjectionRejection, SystemTelemetryRevision, SystemTelemetrySubmission,
    SystemTelemetrySubmissionError, SystemTelemetryUnavailable, UrlOpenRequest, UrlOpenRequestPort,
};
pub use request_session::{
    GpuEngineRowsFailed, GpuEngineRowsReady, GpuEngineRowsRequestFailure, GpuEngineRowsSession,
    GpuEngineRowsState, NetworkEscalationFailed, NetworkEscalationReady, NetworkEscalationSession,
    NetworkEscalationState, ProcessAffinityReady, ProcessAffinitySession, ProcessAffinityState,
    ProcessBatchFailed, ProcessBatchLoading, ProcessBatchReady, ProcessBatchSession,
    ProcessBatchState, RequestAttemptId, RequestCorrelation, ShellUiActionFailed,
    ShellUiActionIntent, ShellUiActionReady, ShellUiActionReceipt, ShellUiActionSession,
    ShellUiActionState, SmartSelfTestFailed, SmartSelfTestLoading, SmartSelfTestReady,
    SmartSelfTestSession, SmartSelfTestState, request_submission_failure,
};

pub use platform::{
    AutomaticSchedule, AutomaticScheduleProfile, automatic_cadence_ms, automatic_schedules,
};
pub use ports::{PlatformEffect, ServiceControlTarget, SessionControlTarget};
pub use reducer::{AppState, Reduction, UiEffect, reduce};
pub use refresh::{RefreshRequest, ServiceUpdate};
pub use router::{
    CommandBinding, CommandConflict, CommandContext, CommandRouter, CommandScope, RouterError,
    default_bindings, default_router,
};
pub use service_lifecycle::{
    ServiceAttemptId, ServiceDependenciesLifecycle, ServiceLogStreamLifecycle,
    ServiceRequestCorrelation, service_submission_failure,
};
pub use source_status::{
    MergedSourceState, SourceLineProjection, SourceNotice, SourceStateKind, device_source_line,
    merge_source_lines, source_line, source_lines, source_notice,
    source_status_from_operation_failure, truncate_text,
};
pub use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilityRecoveryOutcome,
    CapabilityRecoveryTrigger, CapabilityRequest, CapabilityScheduler, CapabilitySnapshot,
    CapabilityStatus, CompositeSourceSnapshot, DeviceDiscovery, DeviceGeneration, DeviceId,
    DeviceSourceSnapshot, DomainSchedulingSnapshot, EventEnvelope, EventPort, EventPortError,
    EventQueueSchedulingSnapshot, EventSequence, FailureKind, MAX_PROVIDER_PANIC_MESSAGE_CHARS,
    MAX_PROVIDER_PANIC_NOTES, MAX_RECENT_SCHEDULING_STALLS, MAX_REQUEST_SCOPE_BYTES,
    OperationFailure, PartialSourceSnapshot, ProviderFailure, ProviderId, ProviderPanicNote,
    RequestEnvelope, RequestId, RequestIdGenerator, RequestPort, RequestScope, RequestTracking,
    RequestTrackingError, RetryDisposition, RuntimeSchedulingSnapshot, SchedulingAdmissionSnapshot,
    SchedulingBudgetSnapshot, SchedulingDomain, SchedulingScope, SchedulingStall, SidebandPolicy,
    SourceOutcome, SourceStatus, SubmissionError, SubmissionErrorKind,
};
pub use telemetry_refresh_policy::{
    MAX_TELEMETRY_INTERVAL, MIN_TELEMETRY_INTERVAL, TelemetryInterval, TelemetryIntervalError,
    TelemetryRefreshPolicy, TelemetryRefreshPolicyChange,
};

#[cfg(test)]
#[path = "../tests/headless/application_lib_presentation_bounds_tests.rs"]
mod presentation_bounds_tests;

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;
