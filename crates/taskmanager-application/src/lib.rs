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
mod command;
mod config_runtime;
mod config_store;
mod control;
mod device_lifecycle;
mod diagnostics;
/// Toolkit-neutral history-series decimation kernels (LTTB run selection and
/// the stride max-envelope) — the single source every frontend's replay and
/// pixel-budgeted downsampling delegates to.
pub mod history_decimation;
mod history_replay;
/// Self-contained i18n (embedded locale catalogs + `t`). Lives in this shared
/// crate so every frontend (gpui/tui/iced/bevy) imports the same catalog
/// directly from `taskmanager_application::i18n`.
pub mod i18n;
mod interaction;
mod managed_alert_rules;
mod persistent_app_history;

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
    CorrelatedHardwareInventoryEvent, CorrelatedMsrReadoutEvent, CorrelatedNpuInventoryEvent,
    CorrelatedPowerSupplyEvent, CorrelatedProcessAffinityEvent, CorrelatedProcessEvent,
    CorrelatedRaplPowerEvent, CorrelatedSensorEvent, CorrelatedServiceEvent,
    CorrelatedSessionEvent, CorrelatedSetupScriptEvent, CorrelatedShellEvent, CorrelatedSmartEvent,
    CorrelatedSmbiosMemoryEvent, CorrelatedStartupEvent, CorrelatedStorageHealthEvent,
    CorrelatedSystemTelemetryOutcome, CpuTelemetryRequest, CpuTelemetryRequestPort,
    DesktopAppearanceEvent, DesktopAppearanceRequest, DesktopAppearanceRequestPort,
    DesktopNotificationRequest, DesktopNotificationRequestPort, DirectoryUsageEvent,
    DirectoryUsageRequest, DirectoryUsageRequestPort, EnvironmentFacets, GpuEngineRowsEvent,
    GpuEngineRowsRequest, GpuEngineRowsRequestPort, GpuTelemetryRequest, GpuTelemetryRequestPort,
    HardwareInventoryEvent, HardwareInventoryRequest, HardwareInventoryRequestPort,
    HostTelemetryRequest, HostTelemetryRequestPort, IntegrationFacets, MemoryTelemetryRequest,
    MemoryTelemetryRequestPort, MsrReadoutEvent, MsrReadoutRequest, MsrReadoutRequestPort,
    NetworkTelemetryRequest, NetworkTelemetryRequestPort, NpuInventoryEvent, NpuInventoryRequest,
    NpuInventoryRequestPort, PlatformClient, PlatformEvent, PlatformEventBatch,
    PlatformEventContext, PlatformEventPort, PlatformFacets, PlatformHandle, PowerFacets,
    PowerSupplyEvent, PowerSupplyRequest, PowerSupplyRequestPort, ProcessAffinityControlRequest,
    ProcessAffinityControlRequestPort, ProcessAffinityEvent, ProcessAffinityRequest,
    ProcessAffinityRequestPort, ProcessControlRequest, ProcessControlRequestPort,
    ProcessEnvironmentRequest, ProcessEnvironmentRequestPort, ProcessEvent, ProcessFacets,
    ProcessGpuRequest, ProcessGpuRequestPort, ProcessInsightFacet, ProcessInsightFacetEvent,
    ProcessInsightFacetState, ProcessInsightObservation, ProcessInsightUnavailable,
    ProcessInsightsProjection, ProcessInsightsProjectionApplyResult,
    ProcessInsightsProjectionRejection, ProcessInsightsRevision, ProcessInsightsSubmission,
    ProcessInsightsSubmissionError, ProcessIsolationRequest, ProcessIsolationRequestPort,
    ProcessListRequest, ProcessListRequestPort, ProcessNetworkEscalationRequest,
    ProcessNetworkEscalationRequestPort, ProcessNetworkRequest, ProcessNetworkRequestPort,
    ProcessOpenFilesRequest, ProcessOpenFilesRequestPort, ProcessResourceControlRequest,
    ProcessResourceControlRequestPort, ProcessResourcesRequest, ProcessResourcesRequestPort,
    ProcessThreadsRequest, ProcessThreadsRequestPort, ProjectedProcessInsights,
    ProjectedStartupEvidence, ProjectedSystemTelemetry, ProjectionAcceptance, RaplPowerEvent,
    RaplPowerRequest, RaplPowerRequestPort, ResourceRevealRequest, ResourceRevealRequestPort,
    SensorEvent, SensorFacets, SensorRequest, SensorRequestPort, ServiceControlRequest,
    ServiceControlRequestPort, ServiceDependenciesRequest, ServiceDependenciesRequestPort,
    ServiceEvent, ServiceFacets, ServiceInventoryRequest, ServiceInventoryRequestPort,
    ServiceLogSnapshotRequest, ServiceLogSnapshotRequestPort, ServiceLogStreamRequest,
    ServiceLogStreamRequestPort, SessionControlRequestPort, SessionEvent, SessionInventoryRequest,
    SessionInventoryRequestPort, SetupScriptRequest, SetupScriptRequestPort, ShellEvent,
    SmartControlRequest, SmartControlRequestPort, SmartEvent, SmartObservationBatch,
    SmartObservationIssue, SmartObservationProjection, SmartObservationRequest,
    SmartObservationRequestPort, SmartProjectionApplyResult, SmartStateRevision, SmartTrackingEnd,
    SmartTrackingEndReason, SmbiosMemoryEvent, SmbiosMemoryRequest, SmbiosMemoryRequestPort,
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
    GpuEngineRowsState, MsrReadoutFailed, MsrReadoutReady, MsrReadoutRequestFailure,
    MsrReadoutSession, MsrReadoutState, NetworkEscalationFailed, NetworkEscalationReady,
    NetworkEscalationSession, NetworkEscalationState, ProcessAffinityReady, ProcessAffinitySession,
    ProcessAffinityState, ProcessBatchFailed, ProcessBatchLoading, ProcessBatchReady,
    ProcessBatchSession, ProcessBatchState, RaplPowerFailed, RaplPowerReady,
    RaplPowerRequestFailure, RaplPowerSession, RaplPowerState, RequestAttemptId,
    RequestCorrelation, ShellUiActionFailed, ShellUiActionIntent, ShellUiActionReady,
    ShellUiActionReceipt, ShellUiActionSession, ShellUiActionState, SmartSelfTestFailed,
    SmartSelfTestLoading, SmartSelfTestReady, SmartSelfTestSession, SmartSelfTestState,
    SmbiosMemoryFailed, SmbiosMemoryReady, SmbiosMemoryRequestFailure, SmbiosMemorySession,
    SmbiosMemoryState, request_submission_failure,
};

pub use platform::{
    AutomaticSchedule, AutomaticScheduleProfile, automatic_cadence_ms, automatic_schedules,
    default_automatic_cadence_ms,
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
