//! Direct-track interactive state (ADR-027): the shell-owned selection,
//! inventory-sort, and typed-feedback rules for frontends that fold
//! `PlatformEventBatch` directly into [`SystemProjectionStore`] instead of
//! driving the full [`ShellApp`] view state (the TUI/Iced shell track).
//!
//! GPUI is the direct track: it holds one [`DirectTrackState`] per window and
//! routes every selection write and inventory-table header click through the
//! reducers here, so "which pids are selected", "what column / what order",
//! and "what was the last typed control outcome" have ONE implementation in
//! the shell crate even though the GPUI window does not run the shell state
//! machine. The rules mirror the `ShellApp` counterparts exactly:
//!
//! - selection: the live-identity set + the anchor identity follow the same
//!   plain-click-collapse / ctrl-toggle / shift-range / token-aware reconcile
//!   semantics as `ShellApp::selected_rows` (see `app/selection.rs` and the
//!   gpui per-row handler docs);
//! - inventory sorts: `None` keeps provider order, a same-column click flips
//!   the direction, a new column starts ascending — the same post-conditions
//!   as `ShellApp::set_info_sort` in `app/sorting.rs`;
//! - feedback: one [`FeedbackState`](super::FeedbackState) owns typed control
//!   outcomes and runtime notices; there is no parallel lifecycle mirror.
//!
//! The row-order projections here apply the same comparison semantics as the
//! `ShellApp::sorted_*` accessors; the two must not drift (sorting.rs owns the
//! canonical `ShellApp` projection, this module owns the direct-track
//! projection over the same `InfoSortCol`/`SortDir` vocabulary).

use std::collections::HashSet;

use taskforest_contract::{CapabilitySnapshot, RequestId};
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessItem,
};
use taskforest_core::core::services::{ServiceItem, ServiceStatus};
use taskforest_core::core::session::{SessionControlAction, SessionIdentity, SessionItem};
use taskforest_core::core::startup::StartupEntry;
use taskmanager_application::process_sort::compare_processes;
use taskmanager_application::{InteractionState, ServiceDependenciesLifecycle};

use super::process_rows::ProcessRowId;
use super::sort_axis::sort_axis;
use super::sorting::{InfoSortCol, InfoTable, SortCol, SortDir};
use crate::ProcessStatusFilter;
use crate::fixture::DirectTrackSeedFact;
use taskforest_core::core::process::ProcessLiveKey;

mod inventory_sort;
mod process_selection;
mod process_viewing;
mod request_sessions;
mod service_viewing;
mod session_control;

pub use process_selection::identity_range;
pub use service_viewing::ServiceViewing;
use taskforest_contract::CapabilityId;
use taskforest_core::core::alerts::{AlertRuleTransferError, NotificationPolicy};
use taskforest_core::core::history::HistoryRecordSink;
use taskforest_core::core::metrics::SystemSnapshot;
use taskforest_core::core::services::ServiceAction;
use taskforest_core::core::target::ServiceId;
use taskmanager_application::{
    AlertEvaluation, AlertEventSeed, AlertRuleEditResult, ControlRequestId,
    DesktopNotificationRequest, ManagedAlertRuleEdit, NetworkEscalationState,
    PersistentApplicationHistoryRecorder, PlatformEventBatch, ProcessEvent, RequestAttemptId,
    ShellUiActionIntent, ShellUiActionState,
};

/// The Applications-table viewing state for a direct-track window: the active
/// sort, the status bucket, and the search query — the same authoritative
/// fields [`ShellApp`](super::ShellApp) owns for the shell track
/// (`process_sort` / `process_status_filter` / `query`), with the same
/// reducer semantics. A direct-track frontend renders its own table chrome
/// (headers, pills, search box) but never keeps a second copy of these
/// values: click/restore paths go through the reducers here so the cached
/// row projection, keyboard paging, and persistence all read one state.
///
/// Unlike the shell track, the reducers own NO selection side effects: a
/// direct-track window anchors selection by live row identity
/// ([`ProcessSelection`]), so re-ordering rows never needs a cursor reset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessViewing {
    sort: (SortCol, SortDir),
    status_filter: ProcessStatusFilter,
    query: String,
}

/// Row-anchored multi-select over the Applications process table: the batch
/// identity set plus the active semantic row.
///
/// The anchor is the direct-track counterpart of the composed track's
/// cursor: grouped/tree frontends resolve their own visual-row projection to
/// live identities and drive this struct, so a group header (no process
/// target) can be excluded before reaching here. Single-select is "the set
/// contains exactly the anchor identity"; the set is the authoritative
/// batch target and the anchor is the fallback target when the set is empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessSelection {
    rows: HashSet<ProcessLiveKey>,
    anchor: Option<ProcessLiveKey>,
    active_row: Option<ProcessRowId>,
}

/// The three inventory-table sort slots (Services / Startup / Users), each
/// `None` until the user picks a column — `None` preserves provider order.
/// One struct so a direct-track window carries exactly one sort authority.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InventorySorts {
    services: Option<(InfoSortCol, SortDir)>,
    startup: Option<(InfoSortCol, SortDir)>,
    sessions: Option<(InfoSortCol, SortDir)>,
}

/// Deterministic Services rank (active before inactive before failed before
/// unknown) — mirrors the canonical rank in `app/sorting.rs`.
const fn service_status_rank(status: ServiceStatus) -> u8 {
    match status {
        ServiceStatus::Active => 0,
        ServiceStatus::Inactive => 1,
        ServiceStatus::Failed => 2,
        ServiceStatus::Unknown => 3,
    }
}

/// Apply the shared direction to a base ascending ordering (mirrors
/// `app/sorting.rs`).
const fn apply_direction(ordering: std::cmp::Ordering, direction: SortDir) -> std::cmp::Ordering {
    match direction {
        SortDir::Asc => ordering,
        SortDir::Desc => ordering.reverse(),
    }
}

/// Order the Services rows in place by the shared inventory sort. `None`
/// keeps the caller's (provider / filtered) order; the comparison semantics
/// match `ShellApp::sorted_services` (status rank, then name, direction
/// applied; stable, so equal rows keep provider order).
pub fn order_service_rows(rows: &mut [ServiceItem], sort: Option<(InfoSortCol, SortDir)>) {
    let Some((column, direction)) = sort else {
        return;
    };
    rows.sort_by(|left, right| {
        let ordering = match column {
            InfoSortCol::Name => left.name.cmp(&right.name),
            InfoSortCol::Status => {
                service_status_rank(left.status).cmp(&service_status_rank(right.status))
            }
            InfoSortCol::Session | InfoSortCol::Seat => std::cmp::Ordering::Equal,
        };
        apply_direction(ordering, direction)
    });
}

/// Order the Startup rows in place by the shared inventory sort; semantics
/// mirror `ShellApp::sorted_startup_entries` (enabled-first under ascending
/// status, then name).
pub fn order_startup_rows(rows: &mut [StartupEntry], sort: Option<(InfoSortCol, SortDir)>) {
    let Some((column, direction)) = sort else {
        return;
    };
    rows.sort_by(|left, right| {
        let ordering = match column {
            InfoSortCol::Name => left.name.cmp(&right.name),
            InfoSortCol::Status => right.enabled.cmp(&left.enabled),
            InfoSortCol::Session | InfoSortCol::Seat => std::cmp::Ordering::Equal,
        };
        apply_direction(ordering, direction)
    });
}

/// Order the Users (login-session) rows in place by the shared inventory
/// sort; semantics mirror `ShellApp::sorted_sessions` (name = logon user,
/// session id, seat).
pub fn order_session_rows(rows: &mut [SessionItem], sort: Option<(InfoSortCol, SortDir)>) {
    let Some((column, direction)) = sort else {
        return;
    };
    rows.sort_by(|left, right| {
        let ordering = match column {
            InfoSortCol::Name => left.user.cmp(&right.user),
            InfoSortCol::Session => left.id.cmp(&right.id),
            InfoSortCol::Seat => left.seat.cmp(&right.seat),
            InfoSortCol::Status => std::cmp::Ordering::Equal,
        };
        apply_direction(ordering, direction)
    });
}

/// The per-window direct-track interactive state: shell-owned selection,
/// process-table viewing state (sort/status/query), inventory sorts, and
/// typed action feedback plus the application-owned primary interaction in
/// one value.
#[derive(Clone, Debug, Default)]
pub struct DirectTrackState {
    /// Sole canonical projection authority for the GPUI direct track. Raw
    /// platform facts can enter only through [`Self::apply_platform_batch`];
    /// renderer code receives an immutable projection.
    projection: super::SystemProjectionStore,
    /// Single typed authority for affinity reads, process batches, and SMART
    /// self-tests on this direct renderer track.
    request_sessions: super::request_sessions::RequestSessions,
    /// Applications-table selection (pid set + anchor).
    pub selection: ProcessSelection,
    /// Applications-table viewing state: sort, status bucket, search query.
    pub processes: ProcessViewing,
    /// Services-page search query (shared match rule; one shell-owned slot).
    pub services: ServiceViewing,
    /// Services / Startup / Users interactive sorts.
    pub sorts: InventorySorts,
    /// Sole typed feedback authority: latest inventory outcomes and runtime
    /// notices reduce into the same state.
    pub feedback: super::FeedbackState,
    /// Sole semantic owner for process properties and every dangerous
    /// confirmation. GPUI renders this state; it must not retain parallel
    /// confirmation payloads in its window-local surface machine.
    pub interaction: InteractionState,
    /// Correlated dependency lifecycle for the GPUI direct track. This is the
    /// shared application state machine, not a renderer-local flag/data mirror.
    pub service_dependencies: ServiceDependenciesLifecycle,
    /// Session GPU headline-chart metric selection for this window track
    /// (ADR-034 stage 2). The same pure contract `ShellApp` holds for the
    /// composed track; the GPUI root reconciles it per tick against the
    /// viewed device's gate and renders its projection — never a second
    /// copy.
    pub(crate) gpu_chart_metric: crate::presentation::gpu_chart_metric::GpuChartMetricSelection,
    /// Optional durable application-history fan-out owned by this frontend
    /// track while the history preference is enabled.
    persistent_application_history: Option<PersistentApplicationHistoryRecorder>,
}

impl DirectTrackState {
    /// Construct the direct-track projection used by the deterministic demo
    /// surface. The fixture remains the single source for demo facts; this
    /// method only installs its already-built projection into the direct
    /// track and does not expose a mutable projection to renderers.
    pub(crate) fn from_fixture_projection(projection: super::SystemProjectionStore) -> Self {
        Self {
            projection,
            ..Self::default()
        }
    }

    /// Ingest a telemetry snapshot from the background daemon into the direct-track projection store.
    pub fn apply_daemon_telemetry_snapshot(
        &mut self,
        timestamp_ms: u64,
        cpu_usage_pct: Option<f32>,
        memory_used_bytes: Option<u64>,
        memory_total_bytes: Option<u64>,
    ) {
        let snapshot = self
            .projection
            .snapshot
            .get_or_insert_with(SystemSnapshot::default);
        snapshot.timestamp_ms = timestamp_ms;
        if let Some(cpu) = cpu_usage_pct {
            snapshot.cpu.set_global_usage(cpu, timestamp_ms);
        }
        if let (Some(used), Some(total)) = (memory_used_bytes, memory_total_bytes) {
            snapshot
                .memory
                .set_used_and_total(used, total, timestamp_ms);
        }
        self.projection.system_revision = self.projection.system_revision.saturating_add(1);
        self.projection.refresh_count = self.projection.refresh_count.saturating_add(1);
    }

    pub(crate) fn seed_fixture_fact(&mut self, fact: DirectTrackSeedFact) {
        match fact {
            DirectTrackSeedFact::NpuInventory(snapshot) => {
                self.projection.seed_npu_inventory_fixture(snapshot);
                self.projection.system_revision = self.projection.system_revision.saturating_add(1);
            }
            DirectTrackSeedFact::Processes(processes) => {
                self.projection.processes = Some(std::sync::Arc::new(processes));
                self.projection.processes_observed_at_ms = 0;
                self.projection.process_revision =
                    self.projection.process_revision.saturating_add(1);
            }
            DirectTrackSeedFact::Sessions(sessions) => {
                self.projection.sessions = sessions;
                self.projection.sessions_revision =
                    self.projection.sessions_revision.saturating_add(1);
            }
            DirectTrackSeedFact::SessionsSource(source) => {
                self.projection.sessions_source = source;
            }
        }
    }

    #[must_use]
    pub const fn projection(&self) -> &super::SystemProjectionStore {
        &self.projection
    }

    /// Return the filtered and sorted Applications list for the direct-track
    /// frontend. The shell owns the query, status bucket, and ordering; a
    /// renderer receives this list and only adapts it into toolkit rows.
    #[must_use]
    pub fn visible_processes(&self) -> Vec<&ProcessItem> {
        let query = self.processes.query();
        let filter = self.processes.status_filter();
        let (column, direction) = self.processes.sort();
        let ascending = direction == SortDir::Asc;
        let axis = sort_axis(column);
        let mut visible: Vec<_> = self
            .projection
            .processes_slice()
            .iter()
            .filter(|process| filter.matches(&process.status))
            .filter(|process| crate::matches_process_query(process, query))
            .collect();
        visible.sort_by(|left, right| compare_processes(left, right, axis, ascending));
        visible
    }

    /// Project the direct-track selection into the shared process-control
    /// availability state used by GPUI action surfaces.
    #[must_use]
    pub fn process_control_availability(&self) -> super::ProcessControlAvailability {
        let selected: Vec<_> = self.selection.rows().iter().copied().collect();
        super::process_control::process_control_availability(
            self.projection.processes_slice(),
            self.selection.active_row(),
            &selected,
            self.projection
                .capability_status(&CapabilityId::PROCESS_CONTROL),
        )
    }

    #[must_use]
    pub fn process_control_capability_allowed(&self) -> bool {
        super::process_control::process_control_capability_allowed(
            self.projection
                .capability_status(&CapabilityId::PROCESS_CONTROL),
        )
    }

    #[must_use]
    pub fn process_control_targets(&self) -> Vec<ProcessLiveKey> {
        let selected: Vec<_> = self.selection.rows().iter().copied().collect();
        super::process_control::process_control_targets(
            self.projection.processes_slice(),
            self.selection.active_row(),
            &selected,
        )
    }

    #[must_use]
    pub fn process_control_intent(&self, action: ProcessBatchAction) -> Option<ProcessBatchIntent> {
        let selected: Vec<_> = self.selection.rows().iter().copied().collect();
        super::process_control::process_control_intent(
            self.projection.processes_slice(),
            self.selection.active_row(),
            &selected,
            action,
        )
    }

    /// Freeze a process tree through the same shell helper used by the
    /// composed track. The direct frontend receives the frozen intent and
    /// submits it; it never expands descendants itself.
    #[must_use]
    pub fn process_tree_end_intent(&self, root: ProcessLiveKey) -> Option<ProcessBatchIntent> {
        super::process_control::process_tree_intent(
            self.projection.processes_slice(),
            root,
            ProcessBatchAction::EndProcessTree,
        )
    }

    pub fn apply_capability_snapshot(&mut self, snapshot: CapabilitySnapshot) -> bool {
        self.projection.replace_capability_snapshot(snapshot)
    }

    /// Fold one correlated platform batch through the same canonical reducer
    /// used by ShellApp's composed track.
    #[must_use]
    pub fn apply_platform_batch(
        &mut self,
        mut batch: PlatformEventBatch,
    ) -> super::BatchFoldOutput {
        self.request_sessions.filter_platform_terminals(&mut batch);
        let mut output = self.projection.apply_platform_batch(batch);
        self.request_sessions.accept_fold_terminals(&mut output);
        self.record_persistent_application_history(&output);
        output
    }

    /// Attach or detach the process-owned durable-history sink without
    /// exposing persistence handles to the renderer.
    pub fn set_history_persistence_sink(
        &mut self,
        sink: Option<std::sync::Arc<dyn HistoryRecordSink>>,
    ) {
        if self.persistent_application_history.is_some() == sink.is_some() {
            return;
        }
        self.persistent_application_history = sink
            .as_ref()
            .map(|sink| PersistentApplicationHistoryRecorder::new(std::sync::Arc::clone(sink)));
    }

    fn record_persistent_application_history(&mut self, output: &super::BatchFoldOutput) {
        let Some(recorder) = self.persistent_application_history.as_mut() else {
            return;
        };
        for correlated in &output.process_events {
            let processes = match &correlated.event {
                ProcessEvent::Snapshot(processes) => processes,
                _ => continue,
            };
            let _ = recorder.record_process_snapshot(
                processes,
                correlated.sequence.get(),
                correlated.observed_at_ms,
            );
        }
    }

    pub fn edit_alert_rules(
        &mut self,
        edit: ManagedAlertRuleEdit,
    ) -> Result<AlertRuleEditResult, AlertRuleTransferError> {
        let result = self.projection.alert_center.edit_rules(edit)?;
        self.projection
            .record_alert_projection_changes(result.changes);
        Ok(result)
    }

    pub fn set_alert_policy(&mut self, policy: NotificationPolicy) {
        self.projection.alert_center.set_policy(policy);
    }

    /// Clear the shared alert transition history without changing the active
    /// evaluator or notification policy.
    pub fn clear_alert_event_history(&mut self) {
        let changes = self.projection.alert_center.clear_event_history();
        self.projection.record_alert_projection_changes(changes);
    }

    /// Install deterministic alert transition history through the direct
    /// track's fixture boundary; production events still come only from the
    /// shared evaluator.
    pub fn replace_alert_event_history(&mut self, events: Vec<AlertEventSeed>) {
        let changes = self.projection.alert_center.replace_event_history(events);
        self.projection.record_alert_projection_changes(changes);
    }

    pub fn reset_alert_center(&mut self) {
        let changes = self.projection.alert_center.reset();
        self.projection.record_alert_projection_changes(changes);
    }

    #[must_use]
    pub fn evaluate_alerts(
        &mut self,
        snapshot: &SystemSnapshot,
        observed_at_ms: u64,
    ) -> AlertEvaluation {
        let evaluation = self
            .projection
            .alert_center
            .evaluate(snapshot, observed_at_ms);
        self.projection
            .record_alert_projection_changes(evaluation.changes);
        evaluation
    }

    pub fn begin_process_control(
        &mut self,
        request_id: RequestId,
        target: FrozenProcessIdentity,
        kind: super::ProcessControlKind,
    ) {
        self.projection
            .begin_process_control(request_id, target, kind);
    }

    /// Per-tick chart-metric fold (ADR-034 stage 2) — the GPUI twin of
    /// `ShellApp::reconcile_gpu_chart_metric`, over this window track's own
    /// selection. A no-viewed-device gate leaves the selection untouched.
    pub fn reconcile_gpu_chart_metric(
        &mut self,
        gate: &crate::presentation::gpu_chart_metric::GpuChartMetricGate,
    ) -> bool {
        self.gpu_chart_metric.reconcile_gate(gate)
    }

    /// Select one family through the viewed device's gate (the GPUI
    /// pill/keyboard activation path).
    pub fn select_gpu_chart_metric(
        &mut self,
        metric: crate::presentation::gpu_chart_metric::GpuChartMetric,
        gate: &crate::presentation::gpu_chart_metric::GpuChartMetricGate,
    ) -> bool {
        self.gpu_chart_metric.select_gate(metric, gate)
    }

    /// The selector projection this window renders for the viewed device.
    #[must_use]
    pub fn gpu_chart_metric_projection(
        &self,
        gate: &crate::presentation::gpu_chart_metric::GpuChartMetricGate,
    ) -> crate::presentation::gpu_chart_metric::GpuChartMetricProjection {
        self.gpu_chart_metric.projection_gate(gate)
    }

    /// The currently selected family (the headline chart's series identity).
    #[must_use]
    pub const fn gpu_chart_metric_selected(
        &self,
    ) -> crate::presentation::gpu_chart_metric::GpuChartMetric {
        self.gpu_chart_metric.selected()
    }

    #[must_use]
    pub fn begin_shell_ui_action(&mut self, intent: ShellUiActionIntent) -> RequestAttemptId {
        self.request_sessions.begin_shell_ui_action(intent)
    }

    pub fn accept_shell_ui_action(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_shell_ui_action(attempt, request_id)
    }

    pub fn reject_shell_ui_action(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions
            .reject_shell_ui_action(attempt, failure)
    }

    pub fn close_shell_ui_action(&mut self) {
        self.request_sessions.close_shell_ui_action();
    }

    #[must_use]
    pub const fn shell_ui_action_state(&self) -> &ShellUiActionState {
        self.request_sessions.shell_ui_action()
    }

    #[must_use]
    pub fn begin_network_escalation(&mut self) -> RequestAttemptId {
        self.request_sessions.begin_network_escalation()
    }

    pub fn accept_network_escalation(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_network_escalation(attempt, request_id)
    }

    pub fn reject_network_escalation(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions
            .reject_network_escalation(attempt, failure)
    }

    pub fn close_network_escalation(&mut self) {
        self.request_sessions.close_network_escalation();
    }

    #[must_use]
    pub const fn network_escalation_state(&self) -> &NetworkEscalationState {
        self.request_sessions.network_escalation()
    }

    #[must_use]
    pub fn begin_startup_control(&mut self) -> ControlRequestId {
        self.projection.startup_control_requests.begin()
    }

    #[must_use]
    pub fn accept_startup_control(&mut self, request_id: ControlRequestId) -> bool {
        self.projection.startup_control_requests.accept(request_id)
    }

    #[must_use]
    pub fn begin_session_control(
        &mut self,
        identity: SessionIdentity,
        action: SessionControlAction,
    ) -> ControlRequestId {
        self.projection
            .session_control_requests
            .begin(identity, action)
    }

    #[must_use]
    pub fn accept_session_control(
        &mut self,
        request_id: ControlRequestId,
        identity: &SessionIdentity,
        action: SessionControlAction,
    ) -> bool {
        self.projection
            .session_control_requests
            .accept(request_id, identity, action)
    }

    #[must_use]
    pub fn begin_service_control(
        &mut self,
        service_id: ServiceId,
        action: ServiceAction,
    ) -> ControlRequestId {
        self.projection
            .service_control_requests
            .begin(service_id, action)
    }

    #[must_use]
    pub fn accept_service_control(
        &mut self,
        request_id: ControlRequestId,
        service_id: &ServiceId,
        action: ServiceAction,
    ) -> bool {
        self.projection
            .service_control_requests
            .accept(request_id, service_id, action)
    }

    #[must_use]
    pub fn drain_alert_notifications(&mut self) -> Vec<DesktopNotificationRequest> {
        self.projection.drain_alert_notifications()
    }

    pub fn report_notice(
        &mut self,
        source: super::FeedbackSource,
        severity: super::FeedbackSeverity,
        lifecycle: super::FeedbackLifecycle,
        text: impl Into<String>,
    ) {
        self.feedback
            .report_notice(source, severity, lifecycle, text);
    }

    #[must_use]
    pub const fn feedback_notice(&self) -> Option<&super::FeedbackNotice> {
        self.feedback.notice()
    }

    pub fn clear_feedback_notice(&mut self) {
        self.feedback.clear_notice();
    }

    pub fn advance_feedback_time(&mut self, elapsed: std::time::Duration) {
        self.feedback.advance_time(elapsed);
    }
}

#[cfg(test)]
#[path = "../../tests/headless/shell_app_direct_track.rs"]
mod tests;
