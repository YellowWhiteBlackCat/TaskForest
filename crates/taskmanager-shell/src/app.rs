//! Renderer-independent shell state and shared-command adapter (ADR-027).

use taskmanager_application::{
    AlertCenter, AlertEvaluation, AppAction, AppPage, AppState, CommandContext, CommandScope,
    ConfirmationKind, ContainerRollupEvent, DesktopNotificationRequest,
    DeviceLifecycleDiagnosticHistory, DeviceLifecycleProjection, DeviceLifecycleSnapshotRevision,
    DirectoryUsageEvent, HardwareInventoryEvent, KeyCode, LatestControlRequest,
    LatestServiceControlRequest, Modifiers, PendingConfirmation, PlatformEffect,
    PlatformEventBatch, PowerSupplyEvent, ProcessAffinityEvent, ProcessEvent,
    ProjectedProcessInsights, ProjectedSystemTelemetry, Reduction, RefreshRequest, SensorEvent,
    ServiceControlTarget, ServiceEvent, ServiceUpdate, SessionControlConfirmation, SessionEvent,
    SmartEvent, SmartObservationProjection, SmartProjectionApplyResult, StartupControlRequest,
    StartupEvent, StartupEvidenceUnavailable, StorageHealthEvent, SurfaceKind, SurfaceTransition,
    TelemetryRefreshPolicy, reduce,
};
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::{
    ContainerRollup, DirectoryUsageSnapshot, FrozenProcessIdentity, HardwareInfo,
    NpuInventorySnapshot, PowerSupplySnapshot, ProcessBatchAction, ProcessBatchIntent, ProcessItem,
    ProcessSignal, SensorCenterSnapshot, ServiceAction, ServiceItem, SessionItem,
    StartupBootEvidenceSnapshot, StartupEntry, SystemSnapshot,
};
use taskmanager_platform_contract::{
    CapabilityId, CapabilitySnapshot, CapabilityStatus, RequestId,
};

use crate::{InputDispatch, ProcessStatusFilter, ShellKeyEvent, matches_process_query, route_key};
use selection::VisibleProcessesMemo;

mod batch_fold;
mod confirmation_gates;
mod direct_track;
mod effect_dispatch;
mod effects;
mod frame;
mod gpu_chart_metric;
mod input_mode;
mod inventory_source;
mod lifecycle;
mod local_keys;
mod npu_inventory;
mod on_demand;
mod platform_feedback;
mod process_control;
mod process_requests;
pub mod process_rows;
mod request_sessions;
mod row_summary;
pub mod search_input;
mod selection;
pub mod service_log;
mod session_control;
mod sort_axis;
mod sorting;
mod system_telemetry;

pub use self::direct_track::{
    DirectTrackState, InventorySorts, ProcessSelection, ProcessViewing, identity_range,
    order_service_rows, order_session_rows, order_startup_rows,
};
pub use self::selection::selected_rows_range;

use self::effect_dispatch::{MAX_PENDING_NOTIFICATIONS, submission_time_ms};
pub use self::effect_dispatch::{queue_effect, queue_effect_result};
pub use self::frame::{
    BatchFoldChanges, BatchFoldOutput, FrameCommit, ProcessAffinityResult, SmartSelfTestResult,
    TelemetryFrameState,
};
pub use self::gpu_chart_metric::gpu_chart_metric_gate;
pub use self::input_mode::ShellInputMode;
use self::lifecycle::ShellLifecycleState;
pub use self::lifecycle::{
    FeedbackBatchLifetime, FeedbackLifecycle, FeedbackNotice, FeedbackSeverity, FeedbackSource,
    FeedbackState, QuitReason, QuitRequestOutcome, QuitState,
};
pub use self::platform_feedback::process_control_notice_text;
pub use self::process_control::{ProcessControlFeedback, ProcessControlKind};
use self::process_rows::{ProcessProjectionGeneration, ProcessRowId, ProcessRowIdentity};
pub use self::service_log::OpenServiceLog;
pub use self::sort_axis::{aggregate_sort_key, sort_axis};
pub use self::sorting::{InfoSortCol, InfoTable, SortCol, SortDir};

/// The Applications-table page size for PageUp/PageDown motion. Shared by
/// every frontend's page-step navigation; the TUI reuses it for the
/// grouped/tree visual-row paging so a page is the same stride in every mode.
pub const PAGE_STEP: usize = 10;

/// Correlation stamp of the latest dynamic-device observation retained by the
/// shared fold. Frontends pair this with the value/source snapshot only after
/// the fold returns, and never reconstruct it from raw event vectors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicDeviceProjectionStamp {
    pub sequence: u64,
    pub observed_at_ms: u64,
}

/// Renderer-neutral system data projection shared by every frontend
/// (ADR-027 dual-track convergence): the single fold target for
/// `PlatformEventBatch`. UI state (selection, scroll, modals, Entity handles)
/// never lives here; only data truth, typed revisions, lifecycle projections,
/// the alert center, and control-outcome correlation do.
#[derive(Clone, Debug, Default)]
pub struct SystemProjectionStore {
    /// Latest read-only runtime capability inventory. Native composition and
    /// correlated provider health are its only writers; renderers query typed
    /// status by capability ID and never probe an OS path themselves.
    capabilities: CapabilitySnapshot,
    pub snapshot: Option<SystemSnapshot>,
    pub hardware: Option<HardwareInfo>,
    /// Source truth paired with the latest hardware inventory snapshot, or
    /// the typed failure that left the last usable value in place.
    pub hardware_source: Option<Vec<SourceStatus>>,
    pub processes: Option<Vec<ProcessItem>>,
    pub services: Option<Vec<ServiceItem>>,
    pub startup_entries: Option<Vec<StartupEntry>>,
    pub sessions: Option<Vec<SessionItem>>,
    /// Source-status diagnostics for the Services inventory. The row list may
    /// remain usable when one provider is partial, so frontends must render
    /// this beside the rows instead of collapsing the page into a blank
    /// success-looking state.
    pub services_source: Option<Vec<SourceStatus>>,
    /// Source-status diagnostics for the Startup inventory. This remains
    /// independent from startup boot evidence and startup control outcomes.
    pub startup_source: Option<Vec<SourceStatus>>,
    /// Source-status diagnostics for the sessions inventory (which provider
    /// answered, and with what outcome). An empty list from a FAILED source
    /// must not read as "no sessions": frontends render the typed reason
    /// instead (GPUI `empty_state_failure` parity). `None` while no session
    /// snapshot has ever arrived.
    pub sessions_source: Option<Vec<SourceStatus>>,
    /// cgroup-v2 container rollup (typed health + aggregated containers).
    pub containers: Option<ContainerRollup>,
    /// Latest power/battery snapshot (per-battery capacity, charge rate, state).
    /// Arrives via `PlatformEventBatch::power_supply_events`; stored here so
    /// any frontend can render a Battery view without the gpui-only health path.
    pub power_supplies: Option<PowerSupplySnapshot>,
    power_supply_source: Option<Vec<SourceStatus>>,
    power_supply_stamp: Option<DynamicDeviceProjectionStamp>,
    /// Latest sensor-center snapshot (hwmon fan/temperature/power readings).
    /// Arrives via `PlatformEventBatch::sensor_events`; stored here so any
    /// frontend can render fan/temperature views without the gpui-only health
    /// path. The same snapshot feeds the shared device-lifecycle projection.
    pub sensors: Option<SensorCenterSnapshot>,
    sensor_source: Option<Vec<SourceStatus>>,
    sensor_stamp: Option<DynamicDeviceProjectionStamp>,
    /// Latest application-owned per-process insight projection (network / GPU /
    /// resources / isolation / threads facets for the frozen target). Crosses
    /// the batch boundary as `PlatformEventBatch::process_insight_projections`;
    /// last-wins here so any frontend can render the Properties insight cards
    /// without owning the request/revision correlation.
    pub process_insights: Option<ProjectedProcessInsights>,
    /// Latest directory-usage scan snapshot (progress while `Scanning`, the
    /// bounded largest-first result otherwise). Arrives via
    /// `PlatformEventBatch::directory_usage_events`; stored here so any
    /// frontend can render the Disk-page scan panel without the gpui-only
    /// request correlation. The snapshot's own root/scan_id identifies which
    /// mount it belongs to.
    pub directory_usage: Option<DirectoryUsageSnapshot>,
    /// Latest startup boot-evidence projection (BN-05 boot timeline).
    /// Latest-wins from `PlatformEventBatch::startup_evidence_projections`;
    /// a typed unavailable arrives as a marked snapshot so the timeline block
    /// stays honest (silent) instead of rendering stale segments.
    pub startup_boot_evidence: Option<StartupBootEvidenceSnapshot>,
    /// Typed unavailable marker for the startup boot-evidence projection
    /// (a marked snapshot keeps the timeline honest/silent instead of stale).
    pub startup_evidence_unavailable: Option<StartupEvidenceUnavailable>,
    /// Latest NPU accelerator inventory answer (capability
    /// `accelerator.npu`), latest-wins from
    /// `PlatformEventBatch::npu_inventory_events`. An empty device list is
    /// the honest no-NPU host; utilization facts stay typed.
    pub npu_inventory: Option<NpuInventorySnapshot>,
    /// Latest filesystem-health facts and their independent provider status.
    storage_health: Option<taskmanager_core::core::storage_health::FilesystemHealthSnapshot>,
    storage_health_source: Option<Vec<SourceStatus>>,
    /// Application-owned anti-resurrection projection of the latest SMART
    /// batch. The request id is retained only to resolve the matching control
    /// affordance; renderers consume observations, not raw events.
    smart_observations: SmartObservationProjection,
    smart_subject: Option<taskmanager_core::core::storage::StorageDeviceTarget>,
    /// Latest application-correlated six-domain state. `snapshot` is the
    /// complete render model and changes only when all domains are current.
    pub system_telemetry: Option<ProjectedSystemTelemetry>,
    /// Lifecycle state accepted by monotonic platform event sequence.
    pub device_lifecycle_projection: DeviceLifecycleProjection,
    /// Bounded applied/ignored lifecycle outcomes for headless diagnostics.
    pub device_lifecycle_diagnostics: DeviceLifecycleDiagnosticHistory,
    /// Process-list generation used by renderer projections. Unlike
    /// `refresh_count`, this does not advance when an unrelated service,
    /// hardware, or system-telemetry domain changes.
    pub process_revision: u64,
    /// Services inventory generation used by renderer projections.
    pub services_revision: u64,
    /// Startup inventory generation used by renderer projections.
    pub startup_revision: u64,
    /// Login-session inventory generation used by renderer projections.
    pub sessions_revision: u64,
    /// System/performance-domain generation used by hardware, sensor, power,
    /// container, and system-telemetry projections.
    pub system_revision: u64,
    /// Shared alert evaluation + delivery (BN-07): the rule engine and
    /// notification gate run here so every frontend gets identical "what
    /// fired" and "should the desktop be told" decisions. `alert_active`
    /// mirrors the latest evaluation for the in-app surfaces; notifications
    /// accumulate in a bounded queue the frontend drains and submits.
    pub alert_center: AlertCenter,
    pub alert_active: Vec<taskmanager_core::core::alerts::Alert>,
    pub(crate) pending_notifications: std::collections::VecDeque<DesktopNotificationRequest>,
    /// Timestamp of the last snapshot folded into the rolling suggestion
    /// window. Used to avoid double-counting a snapshot that did not change
    /// when a batch only refreshed, say, the process list.
    pub(crate) last_recorded_snapshot_ms: u64,
    /// Global update counter retained for status text and semantic snapshot
    /// identity. Domain projections must use the narrower revisions above.
    pub refresh_count: u64,
    /// Latest-wins correlation for renderer-neutral login-session actions.
    pub session_control_requests: LatestControlRequest,
    /// The last accepted session-control outcome, for point-of-action
    /// feedback in any frontend's action bar. Cleared when a new action is
    /// requested so stale feedback expires.
    pub session_control_feedback: Option<taskmanager_application::SessionControlOutcome>,
    /// Latest-wins correlation for renderer-neutral startup-entry actions.
    pub startup_control_requests: LatestControlRequest,
    /// Latest-wins correlation for renderer-neutral service-control actions,
    /// including exact target authority for accepted outcomes.
    pub service_control_requests: LatestServiceControlRequest,
    /// Latest-wins correlation for renderer-neutral process-control
    /// submissions (end task / affinity control / resource limits): the
    /// platform envelope request id plus the frozen target, echoed back by
    /// `ProcessEvent` completions. Fail-closed acceptance lives in
    /// the `process_control` module.
    pub process_control_requests: self::process_control::LatestProcessControlRequest,
    /// Coalesced typed refresh requested by an accepted control completion.
    /// Keeping the request payload here avoids a separate bool whose meaning
    /// would have to be reconstructed by every draining frontend.
    pub(crate) process_refresh_request: Option<RefreshRequest>,
}

impl SystemProjectionStore {
    /// Return the process projection's typed generation token. This is a
    /// cache/geometry token, not a provider start token or a control request
    /// id; unrelated domain folds do not change it.
    #[must_use]
    pub const fn process_projection_generation(&self) -> ProcessProjectionGeneration {
        ProcessProjectionGeneration::new(self.process_revision)
    }

    #[must_use]
    pub fn capability_status(&self, id: &CapabilityId) -> Option<CapabilityStatus> {
        self.capabilities
            .get(id)
            .map(|descriptor| descriptor.status)
    }

    fn replace_capability_snapshot(&mut self, snapshot: CapabilitySnapshot) -> bool {
        if self.capabilities == snapshot {
            return false;
        }
        self.capabilities = snapshot;
        true
    }
}

#[derive(Clone, Debug, Default)]
pub struct ShellApp {
    pub application: AppState,
    /// Sole renderer-neutral projection authority for the Iced/TUI track.
    ///
    /// Platform batches and semantic reducers are the only writers. Frontends
    /// receive an immutable view through [`Self::projection`]; deterministic
    /// data injection lives in the crate's typed [`crate::fixture`] seam.
    data: SystemProjectionStore,
    /// Single typed authority for affinity reads, process batches, and SMART
    /// self-tests on the composed shell track.
    request_sessions: self::request_sessions::RequestSessions,
    /// Keyboard anchor index into [`Self::visible_processes`] (single-select
    /// cursor for the TUI arrow path). Derived state: it follows the
    /// identity-authoritative [`Self::selected_row`] across reorder and is
    /// clamped positionally when that row disappears.
    pub selected: usize,
    /// Multi-select target set for the Applications process table, keyed by
    /// validated live identity (pid + provider start token, CORE-01). A plain
    /// click / bare arrow resets this to exactly the anchor identity (single
    /// select); Ctrl-click toggles an identity; Shift-click grows a range —
    /// mirrors the gpui per-row handler (`processes_view/rows.rs`).
    /// `request_process_batch` freezes the whole set exactly so a batch verb
    /// (Kill / Suspend / Resume / SetPriority) reaches every selected row and
    /// never a pid-reuse impostor.
    pub selected_rows: std::collections::HashSet<ProcessRowIdentity>,
    /// Semantic primary row. PID-less application aggregates live here
    /// without being rewritten into the root process identity.
    pub selected_row: Option<ProcessRowId>,
    pub query: String,
    /// The active Applications state bucket. The renderer owns the segmented
    /// control; the shell owns the filtered row set consumed by every action
    /// and keyboard path.
    pub process_status_filter: ProcessStatusFilter,
    input_mode: ShellInputMode,
    /// Active process-table sort. Surfaced in the table header and changed by
    /// the `s` (cycle column) / `S` (reverse direction) terminal bindings.
    pub process_sort: (SortCol, SortDir),
    /// Memoized visible-row projection: indices keyed on the process-domain
    /// revision, the trimmed query, the sort, and the table length. The filter
    /// and sort run once per process input change instead of per call —
    /// `visible_processes` has about a dozen call sites across the fold and
    /// all four frontends, several on 10 Hz poll cadences.
    pub(crate) visible_processes_memo: std::cell::RefCell<Option<VisibleProcessesMemo>>,
    /// Active Services-table sort; `None` keeps provider order until a header
    /// click picks a column. Single source: every frontend projects rows
    /// through [`ShellApp::sorted_services`].
    pub services_sort: Option<(InfoSortCol, SortDir)>,
    /// Active Startup-table sort; semantics mirror [`ShellApp::services_sort`].
    pub startup_sort: Option<(InfoSortCol, SortDir)>,
    /// Active Users-table sort; semantics mirror [`ShellApp::services_sort`].
    pub sessions_sort: Option<(InfoSortCol, SortDir)>,
    /// Single renderer-neutral live graph store. Its private paired writer is
    /// fed only from application-correlated outcomes below.
    pub history: taskmanager_telemetry_store::live_graph::LiveGraphHistory,
    /// Bounded evidence for alert-threshold suggestions and the SMART detail
    /// trend. It intentionally owns no general live graph series.
    pub alert_suggestions: taskmanager_application::AlertSuggestionWindow,
    /// Write capability paired with `history`. Kept private so renderers can
    /// only read accepted telemetry.
    history_ingestor: Option<taskmanager_telemetry_store::CorrelatedSystemTelemetryIngestor>,
    /// Optional durable application-history fan-out. The capability is
    /// installed only while the owning frontend session has history enabled.
    persistent_application_history:
        Option<taskmanager_application::PersistentApplicationHistoryRecorder>,
    /// Single quit + typed feedback lifecycle authority. All transitions go
    /// through the lifecycle reducer in `app/lifecycle.rs`.
    lifecycle: ShellLifecycleState,
    /// Open service-log stream state (one service at a time): the frozen
    /// service identity plus the core-owned bounded feed (follow/pause/
    /// level/time filters, cursor dedup). `None` means no log panel is open.
    pub service_log: Option<OpenServiceLog>,
    /// Correlated dependency-panel lifecycle. Frontends render this shared
    /// state and never keep parallel loading/data/failure fields.
    pub service_dependencies: taskmanager_application::ServiceDependenciesLifecycle,
    /// Monotonic wall-clock (ms) of the last follow request the shell emitted,
    /// so `poll_service_log` throttles incremental streams to ~1 Hz without
    /// touching a clock itself.
    last_service_log_poll_ms: u64,
    /// Frontend-local automatic telemetry cadence. Native adapters never see
    /// pause or interval state; other capability families keep their own
    /// independent schedules while telemetry is paused.
    telemetry_refresh_policy: TelemetryRefreshPolicy,
    /// Session GPU headline-chart metric selection (ADR-034 stage 2): one
    /// instance per composed window-track, consumed by the Iced and TUI
    /// frontends through the named folds in `app::gpu_chart_metric`. Never
    /// persisted, never mirrored by a renderer.
    pub(crate) gpu_chart_metric: crate::presentation::gpu_chart_metric::GpuChartMetricSelection,
}

impl ShellApp {
    #[must_use]
    pub fn new() -> Self {
        Self {
            lifecycle: ShellLifecycleState::new("Waiting for telemetry…"),
            ..Self::default()
        }
    }

    /// Immutable renderer projection. This deliberately has no mutable twin:
    /// facts enter through [`Self::apply_platform_batch`] or named semantic
    /// reducers, never through frontend field assignment.
    #[must_use]
    pub const fn projection(&self) -> &SystemProjectionStore {
        &self.data
    }

    /// Cache the runtime's read-only capability inventory. This is a named
    /// reducer so every renderer consumes the same platform authority without
    /// receiving mutable access to the projection store.
    pub fn apply_capability_snapshot(&mut self, snapshot: CapabilitySnapshot) -> bool {
        self.data.replace_capability_snapshot(snapshot)
    }

    /// Install deterministic facts from the crate-owned fixture boundary.
    /// Production callers cannot reach this method and no mutable projection
    /// reference escapes it.
    pub(crate) fn seed_fixture_projection(&mut self, seed: crate::fixture::ProjectionSeed) {
        self.data.snapshot = seed.snapshot;
        self.data.hardware = seed.hardware;
        self.data.processes = seed.processes;
        self.data.services = seed.services;
        self.data.startup_entries = seed.startup_entries;
        self.data.sessions = seed.sessions;
        self.data.services_source = seed.services_source;
        self.data.startup_source = seed.startup_source;
        self.data.sessions_source = seed.sessions_source;
        self.data.last_recorded_snapshot_ms = self
            .data
            .snapshot
            .as_ref()
            .map_or(0, |snapshot| snapshot.timestamp_ms);
    }

    pub(crate) fn seed_fixture_fact(&mut self, fact: crate::fixture::ProjectionSeedFact) {
        use crate::fixture::ProjectionSeedFact;
        match fact {
            ProjectionSeedFact::Snapshot(value) => {
                self.data.snapshot = *value;
                self.data.system_revision = self.data.system_revision.saturating_add(1);
            }
            ProjectionSeedFact::Hardware(value) => {
                self.data.hardware = value.map(|value| *value);
                self.data.system_revision = self.data.system_revision.saturating_add(1);
            }
            ProjectionSeedFact::Processes(value) => {
                self.data.processes = value;
                self.data.process_revision = self.data.process_revision.saturating_add(1);
            }
            ProjectionSeedFact::Services(value) => {
                self.data.services = value;
                self.data.services_revision = self.data.services_revision.saturating_add(1);
            }
            ProjectionSeedFact::StartupEntries(value) => {
                self.data.startup_entries = value;
                self.data.startup_revision = self.data.startup_revision.saturating_add(1);
            }
            ProjectionSeedFact::Sessions(value) => {
                self.data.sessions = value;
                self.data.sessions_revision = self.data.sessions_revision.saturating_add(1);
            }
            ProjectionSeedFact::Containers(value) => self.data.containers = value,
            ProjectionSeedFact::PowerSupplies(value) => self.data.power_supplies = value,
            ProjectionSeedFact::Sensors(value) => self.data.sensors = value,
            ProjectionSeedFact::NpuInventory(value) => {
                self.data.npu_inventory = value;
                self.data.system_revision = self.data.system_revision.saturating_add(1);
            }
            ProjectionSeedFact::DirectoryUsage(value) => self.data.directory_usage = value,
            ProjectionSeedFact::StartupBootEvidence(value) => {
                self.data.startup_boot_evidence = value;
            }
            ProjectionSeedFact::ServicesSource(value) => self.data.services_source = value,
            ProjectionSeedFact::StartupSource(value) => self.data.startup_source = value,
            ProjectionSeedFact::SessionsSource(value) => self.data.sessions_source = value,
            ProjectionSeedFact::ProcessAffinity(value) => {
                self.request_sessions.seed_affinity(value);
            }
            ProjectionSeedFact::ProcessInsights(value) => self.data.process_insights = *value,
            ProjectionSeedFact::ActiveAlerts(value) => self.data.alert_active = value,
            ProjectionSeedFact::AdvanceRevision(domain) => match domain {
                crate::fixture::ProjectionSeedDomain::Processes => {
                    self.data.process_revision = self.data.process_revision.saturating_add(1);
                }
                crate::fixture::ProjectionSeedDomain::Services => {
                    self.data.services_revision = self.data.services_revision.saturating_add(1);
                }
                crate::fixture::ProjectionSeedDomain::Startup => {
                    self.data.startup_revision = self.data.startup_revision.saturating_add(1);
                }
                crate::fixture::ProjectionSeedDomain::Sessions => {
                    self.data.sessions_revision = self.data.sessions_revision.saturating_add(1);
                }
                crate::fixture::ProjectionSeedDomain::System => {
                    self.data.system_revision = self.data.system_revision.saturating_add(1);
                }
            },
            ProjectionSeedFact::AdvanceRefresh => {}
        }
        self.data.refresh_count = self.data.refresh_count.saturating_add(1);
    }

    pub(crate) fn edit_fixture_snapshot(&mut self, edit: impl FnOnce(&mut Option<SystemSnapshot>)) {
        edit(&mut self.data.snapshot);
        self.data.system_revision = self.data.system_revision.saturating_add(1);
        self.data.refresh_count = self.data.refresh_count.saturating_add(1);
    }

    pub(crate) fn edit_fixture_processes(
        &mut self,
        edit: impl FnOnce(&mut Option<Vec<ProcessItem>>),
    ) {
        edit(&mut self.data.processes);
        self.data.process_revision = self.data.process_revision.saturating_add(1);
        self.data.refresh_count = self.data.refresh_count.saturating_add(1);
    }

    pub(crate) fn edit_fixture_hardware(&mut self, edit: impl FnOnce(&mut Option<HardwareInfo>)) {
        edit(&mut self.data.hardware);
        self.data.system_revision = self.data.system_revision.saturating_add(1);
        self.data.refresh_count = self.data.refresh_count.saturating_add(1);
    }

    pub(crate) fn edit_fixture_containers(
        &mut self,
        edit: impl FnOnce(&mut Option<ContainerRollup>),
    ) {
        edit(&mut self.data.containers);
        self.data.system_revision = self.data.system_revision.saturating_add(1);
        self.data.refresh_count = self.data.refresh_count.saturating_add(1);
    }

    pub(crate) fn seed_fixture_process_batch_loading(
        &mut self,
        intent: ProcessBatchIntent,
        request_id: RequestId,
    ) {
        let attempt = self.request_sessions.begin_batch(intent);
        let _ = self.request_sessions.accept_batch(attempt, request_id);
    }

    /// Apply one semantic edit to the canonical managed alert-rule set.
    pub fn edit_alert_rules(
        &mut self,
        edit: taskmanager_application::ManagedAlertRuleEdit,
    ) -> Result<
        taskmanager_application::ManagedAlertRuleEditOutcome,
        taskmanager_core::core::alerts::AlertRuleTransferError,
    > {
        self.data.alert_center.edit_rules(edit)
    }

    /// Replace desktop-notification policy without exposing the alert engine.
    pub fn set_alert_policy(&mut self, policy: taskmanager_core::core::alerts::NotificationPolicy) {
        self.data.alert_center.set_policy(policy);
    }

    /// Clear the shared alert transition history without changing the active
    /// evaluator or its notification policy.
    pub fn clear_alert_event_history(&mut self) {
        self.data.alert_center.clear_event_history();
    }

    /// Install deterministic alert transition history through the shared
    /// fixture/capture seam; live history still comes only from evaluation.
    pub fn replace_alert_event_history(
        &mut self,
        events: Vec<taskmanager_core::core::alerts::AlertEvent>,
    ) {
        self.data.alert_center.replace_event_history(events);
    }

    #[must_use]
    pub fn evaluate_alerts(
        &mut self,
        snapshot: &SystemSnapshot,
        observed_at_ms: u64,
    ) -> AlertEvaluation {
        self.data.alert_center.evaluate(snapshot, observed_at_ms)
    }

    #[must_use]
    pub const fn page(&self) -> AppPage {
        self.application.active_page
    }

    /// The sole shared dangerous intent awaiting confirmation.
    #[must_use]
    pub const fn pending_confirmation(&self) -> Option<&PendingConfirmation> {
        self.application.interaction.pending_confirmation()
    }

    #[must_use]
    pub const fn confirmation_kind(&self) -> Option<ConfirmationKind> {
        self.application.interaction.confirmation_kind()
    }

    #[must_use]
    pub const fn interaction_surface(&self) -> Option<SurfaceKind> {
        self.application.interaction.kind()
    }

    #[must_use]
    pub const fn process_properties_target(&self) -> Option<&FrozenProcessIdentity> {
        self.application.interaction.process_properties()
    }

    #[must_use]
    pub const fn pending_end(&self) -> Option<&FrozenProcessIdentity> {
        match self.pending_confirmation() {
            Some(PendingConfirmation::EndTask(target)) => Some(target),
            _ => None,
        }
    }

    #[must_use]
    pub const fn pending_service_control(&self) -> Option<&ServiceControlTarget> {
        match self.pending_confirmation() {
            Some(PendingConfirmation::ServiceControl(target)) => Some(target),
            _ => None,
        }
    }

    #[must_use]
    pub const fn pending_batch(&self) -> Option<&ProcessBatchIntent> {
        match self.pending_confirmation() {
            Some(PendingConfirmation::ProcessBatch(intent)) => Some(intent),
            _ => None,
        }
    }

    #[must_use]
    pub const fn pending_startup(&self) -> Option<&StartupControlRequest> {
        match self.pending_confirmation() {
            Some(PendingConfirmation::StartupControl(request)) => Some(request),
            _ => None,
        }
    }

    #[must_use]
    pub const fn pending_session(&self) -> Option<&SessionControlConfirmation> {
        match self.pending_confirmation() {
            Some(PendingConfirmation::SessionControl(pending)) => Some(pending),
            _ => None,
        }
    }

    /// Return the shared lifecycle of the visible telemetry frame.
    ///
    /// Every renderer uses this accessor for its first-frame gate. Platform
    /// adapters never participate in this decision: they only emit correlated
    /// observations into the application batch seam.
    #[must_use]
    pub const fn telemetry_frame_state(&self) -> TelemetryFrameState {
        self.data.telemetry_frame_state()
    }

    /// Return the authoritative row count for the active typed table.
    ///
    /// Renderer adapters use this only to decide whether a renderer-local
    /// focus target can exist. The selected index and all row projections stay
    /// owned by the shell; performance and system pages intentionally return
    /// `None` because their panels are not selectable tables.
    #[must_use]
    pub fn table_row_count(&self) -> Option<usize> {
        match self.page() {
            AppPage::Applications => Some(self.visible_process_count()),
            AppPage::Services => Some(self.data.services.as_deref().unwrap_or_default().len()),
            AppPage::Startup => Some(
                self.data
                    .startup_entries
                    .as_deref()
                    .unwrap_or_default()
                    .len(),
            ),
            AppPage::Users => Some(self.data.sessions.as_deref().unwrap_or_default().len()),
            AppPage::Performance | AppPage::System | AppPage::AppHistory => None,
        }
    }

    #[must_use]
    pub fn visible_processes(&self) -> Vec<&ProcessItem> {
        let rows = self.visible_processes_indices();
        let processes = self.data.processes.as_deref().unwrap_or_default();
        rows.iter().map(|&index| &processes[index]).collect()
    }

    /// Select the Applications state bucket and reset the cursor to the first
    /// row in the new projection. The default `All` keeps legacy frontends
    /// unchanged while Iced can expose the six-state control safely.
    pub fn set_process_status_filter(&mut self, filter: ProcessStatusFilter) {
        self.process_status_filter = filter;
        self.selected = 0;
        self.sync_application_selection();
        self.report_notice(
            FeedbackSource::Navigation,
            FeedbackSeverity::Info,
            FeedbackLifecycle::SHORT,
            format!(
                "{}: {}",
                taskmanager_application::i18n::t("proc.status_filter"),
                filter.label()
            ),
        );
    }

    /// The memoized visible-row indices for the CURRENT query + sort. A hit
    /// skips the filter and sort entirely; a miss recomputes and stores. The
    /// indices are borrowed against the current process generation, which the
    /// key pins — a new snapshot invalidates the memo before any index can
    /// dangle.
    fn visible_processes_indices(&self) -> std::rc::Rc<Vec<usize>> {
        let query = self.query.trim().to_owned();
        // Single-threaded per frontend and this fn is the memo's only
        // borrower (the filter/sort closures never re-enter), so the plain
        // borrow cannot conflict — mirrors the iced `ProjectionMemo` pattern.
        let mut memo = self.visible_processes_memo.borrow_mut();
        let source_len = self.data.processes.as_ref().map_or(0, Vec::len);
        if let Some(cache) = memo.as_ref()
            && cache.process_revision == self.data.process_revision
            && cache.source_len == source_len
            && cache.query == query
            && cache.status_filter == self.process_status_filter
            && cache.sort == self.process_sort
        {
            return cache.indices.clone();
        }
        let mut indices: Vec<usize> = self
            .data
            .processes
            .as_deref()
            .unwrap_or_default()
            .iter()
            .enumerate()
            .filter(|(_, process)| {
                self.process_status_filter.matches(&process.status)
                    && matches_process_query(process, &query)
            })
            .map(|(index, _)| index)
            .collect();
        let processes = self.data.processes.as_deref().unwrap_or_default();
        indices.sort_by(|&left, &right| {
            let (column, direction) = self.process_sort;
            let left_item = &processes[left];
            let right_item = &processes[right];
            let primary = column.ascending(left_item, right_item);
            let ordered = match direction {
                SortDir::Asc => primary,
                SortDir::Desc => primary.reverse(),
            };
            // Stable, direction-independent tiebreaker so equal rows keep a
            // deterministic order regardless of the active sort column.
            ordered.then_with(|| left_item.pid.cmp(&right_item.pid))
        });
        let indices = std::rc::Rc::new(indices);
        *memo = Some(VisibleProcessesMemo {
            process_revision: self.data.process_revision,
            query,
            status_filter: self.process_status_filter,
            sort: self.process_sort,
            source_len,
            indices: indices.clone(),
        });
        indices
    }

    #[must_use]
    pub fn selected_process_identity(&self) -> Option<FrozenProcessIdentity> {
        // Exact-identity freeze (CORE-01): a Process row freezes from its own
        // validated identity. Application aggregates carry no process target
        // (never a representative member) and structural rows have none — the
        // positional fallback applies only when no semantic row is set.
        match self.selected_row {
            Some(ProcessRowId::Process(identity)) => {
                let processes = self.data.processes.as_deref().unwrap_or_default();
                processes
                    .iter()
                    .find(|process| {
                        process.pid == identity.pid()
                            && process.current_start_token() == Some(identity.start_token())
                    })
                    .and_then(FrozenProcessIdentity::from_process)
            }
            Some(ProcessRowId::Application(_)) | Some(ProcessRowId::Category(_)) => None,
            None => self
                .visible_process_at(self.selected)
                .and_then(FrozenProcessIdentity::from_process),
        }
    }

    fn apply_reduction(&mut self, reduction: Reduction) -> Option<PlatformEffect> {
        if let Some(effect) = reduction.ui {
            self.apply_ui_effect(effect);
        }
        self.apply_surface_transition(reduction.surface);
        if let Some(effect) = reduction.platform.as_ref() {
            self.report_effect_queued(effect);
        }
        reduction.platform
    }

    fn arm_confirmation(&mut self, pending: PendingConfirmation) {
        let reduction = self.application.interaction.reduce(
            taskmanager_application::InteractionEvent::ArmConfirmation(pending),
        );
        self.apply_surface_transition(reduction.transition);
        debug_assert!(reduction.effect.is_none());
    }

    fn confirm_confirmation(&mut self, expected: ConfirmationKind) -> Option<PlatformEffect> {
        let reduction = self
            .application
            .interaction
            .reduce(taskmanager_application::InteractionEvent::Confirm(expected));
        self.apply_surface_transition(reduction.transition);
        if let Some(effect) = reduction.effect.as_ref() {
            self.report_effect_queued(effect);
        }
        reduction.effect
    }

    fn apply_surface_transition(&mut self, transition: SurfaceTransition) {
        match transition {
            SurfaceTransition::Opened(surface)
            | SurfaceTransition::Replaced {
                current: surface, ..
            } => {
                self.reset_input_mode();
                if surface == SurfaceKind::ProcessProperties
                    && let Some(target) = self.process_properties_target()
                {
                    self.report_notice(
                        FeedbackSource::Interaction,
                        FeedbackSeverity::Info,
                        FeedbackLifecycle::SHORT,
                        format!(
                            "{} ({}) · start {}",
                            target.name, target.pid, target.start_time_secs
                        ),
                    );
                }
            }
            SurfaceTransition::Unchanged
            | SurfaceTransition::Confirmed(_)
            | SurfaceTransition::Dismissed { .. } => {}
        }
    }

    /// Re-sync the shared application selection from the shell cursor.
    /// Frontends that mutate `query`/`selected` directly call this to keep
    /// the application layer honest.
    /// Take the queued desktop notifications for submission. Frontends call
    /// this after `apply_platform_batch` and route each request through
    /// `queue_effect`; an empty result is the common case.
    pub fn drain_alert_notifications(&mut self) -> Vec<DesktopNotificationRequest> {
        self.data.drain_alert_notifications()
    }

    pub fn sync_application_selection(&mut self) {
        self.application.selected_process = if self.page() == AppPage::Applications {
            self.selected_process_identity()
        } else {
            None
        };
    }

    /// Capture the provider-issued service target and action for the gated
    /// confirmation flow. Rejects a read-only row (empty provider target) so
    /// a destructive action can never be authorized from a display-only
    /// snapshot.
    #[must_use]
    pub fn select_service_control(&mut self, service: &ServiceItem, action: ServiceAction) -> bool {
        if service.id.as_str().is_empty() {
            return false;
        }
        self.application.selected_service_control = Some(ServiceControlTarget {
            service_id: service.id.clone(),
            action,
        });
        true
    }

    fn active_row_count(&self) -> usize {
        self.table_row_count().unwrap_or(1)
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.active_row_count().saturating_sub(1));
        self.sync_application_selection();
    }
}

#[cfg(test)]
#[path = "../tests/headless/app.rs"]
mod tests;
