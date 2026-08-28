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
//! - selection: the pid set + the anchor pid follow the same
//!   plain-click-collapse / ctrl-toggle / shift-range / live-prune semantics
//!   as `ShellApp::selected_pids` (see `app/selection.rs` and the gpui
//!   per-row handler docs);
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

use taskmanager_application::{
    CapabilitySnapshot, InteractionState, ProcessCategory, ServiceDependenciesLifecycle,
    ServiceItem, ServiceStatus, SessionItem, StartupEntry,
};

use super::sorting::{InfoSortCol, InfoTable, SortCol, SortDir};
use crate::ProcessStatusFilter;

mod inventory_sort;
mod process_selection;
mod process_viewing;

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
/// direct-track window anchors selection by pid
/// ([`ProcessSelection`]), so re-ordering rows never needs a cursor reset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessViewing {
    sort: (SortCol, SortDir),
    status_filter: ProcessStatusFilter,
    query: String,
}

/// Stable semantic identity of one projected Applications-table row.
///
/// Category rows are structural tree headers. Application rows are PID-less
/// aggregates keyed by their process-tree root; the root pid is a live lookup
/// key, never a representative process identity. Process rows retain their
/// real pid. Exact authority is frozen only when an action is submitted.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ProcessRowKey {
    Category(ProcessCategory),
    Application(u32),
    Process(u32),
}

/// Row-anchored multi-select over the Applications process table: the batch
/// PID set plus the active semantic row.
///
/// The anchor is the direct-track counterpart of `ShellApp::selected` (the
/// index cursor): grouped/tree frontends resolve their own visual-row
/// projection to pids and drive this struct, so a group header (no pid) can be
/// excluded before reaching here. Single-select is "the set contains exactly
/// the anchor pid"; the set is the authoritative batch target and the anchor
/// is the fallback target when the set is empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessSelection {
    pids: HashSet<u32>,
    anchor: Option<u32>,
    active_row: Option<ProcessRowKey>,
}

/// The pid range spanning `anchor` → `end` (inclusive, in display order).
/// A missing endpoint yields an empty range (the caller keeps its prior set);
/// this is the `&[u32]` counterpart of [`super::selected_pids_range`], which
/// resolves the same span against `&ProcessItem` rows.
#[must_use]
pub fn pid_range(display_pids: &[u32], anchor: u32, end: u32) -> Vec<u32> {
    let start = display_pids.iter().position(|pid| *pid == anchor);
    let end_pos = display_pids.iter().position(|pid| *pid == end);
    match (start, end_pos) {
        (Some(start), Some(end_pos)) => {
            display_pids[start.min(end_pos)..=start.max(end_pos)].to_vec()
        }
        _ => Vec::new(),
    }
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
    persistent_application_history:
        Option<taskmanager_application::PersistentApplicationHistoryRecorder>,
}

impl DirectTrackState {
    pub(crate) fn seed_fixture_fact(&mut self, fact: crate::fixture::DirectTrackSeedFact) {
        match fact {
            crate::fixture::DirectTrackSeedFact::NpuInventory(snapshot) => {
                self.projection.npu_inventory = Some(snapshot);
                self.projection.system_revision = self.projection.system_revision.saturating_add(1);
            }
        }
    }

    #[must_use]
    pub const fn projection(&self) -> &super::SystemProjectionStore {
        &self.projection
    }

    pub fn apply_capability_snapshot(&mut self, snapshot: CapabilitySnapshot) -> bool {
        self.projection.replace_capability_snapshot(snapshot)
    }

    /// Fold one correlated platform batch through the same canonical reducer
    /// used by ShellApp's composed track.
    #[must_use]
    pub fn apply_platform_batch(
        &mut self,
        mut batch: taskmanager_application::PlatformEventBatch,
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
        sink: Option<std::sync::Arc<dyn taskmanager_application::HistoryRecordSink>>,
    ) {
        if self.persistent_application_history.is_some() == sink.is_some() {
            return;
        }
        self.persistent_application_history = sink.as_ref().map(|sink| {
            taskmanager_application::PersistentApplicationHistoryRecorder::new(
                std::sync::Arc::clone(sink),
            )
        });
    }

    fn record_persistent_application_history(&mut self, output: &super::BatchFoldOutput) {
        let Some(recorder) = self.persistent_application_history.as_mut() else {
            return;
        };
        for correlated in &output.process_events {
            let taskmanager_application::ProcessEvent::Snapshot(processes) = &correlated.event
            else {
                continue;
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
        edit: taskmanager_application::ManagedAlertRuleEdit,
    ) -> Result<
        taskmanager_application::ManagedAlertRuleEditOutcome,
        taskmanager_application::alerts::AlertRuleTransferError,
    > {
        self.projection.alert_center.edit_rules(edit)
    }

    pub fn set_alert_policy(
        &mut self,
        policy: taskmanager_application::alerts::NotificationPolicy,
    ) {
        self.projection.alert_center.set_policy(policy);
    }

    #[must_use]
    pub fn evaluate_alerts(
        &mut self,
        snapshot: &taskmanager_application::SystemSnapshot,
        observed_at_ms: u64,
    ) -> taskmanager_application::AlertEvaluation {
        self.projection
            .alert_center
            .evaluate(snapshot, observed_at_ms)
    }

    /// Commit a synchronous alert-rule evaluation through a named reducer.
    /// Returns the new revision used by renderer materialization.
    pub fn accept_alert_evaluation(
        &mut self,
        active: Vec<taskmanager_application::alerts::Alert>,
    ) -> u64 {
        self.projection.alert_active = active;
        self.projection.refresh_count = self.projection.refresh_count.saturating_add(1);
        self.projection.refresh_count
    }

    pub fn begin_process_control(
        &mut self,
        request_id: taskmanager_application::RequestId,
        target: taskmanager_application::FrozenProcessIdentity,
        kind: super::ProcessControlKind,
    ) {
        self.projection
            .begin_process_control(request_id, target, kind);
    }

    #[must_use]
    pub fn begin_process_affinity_read(
        &mut self,
        target: taskmanager_application::FrozenProcessIdentity,
    ) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_affinity(target)
    }

    pub fn accept_process_affinity_read(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: taskmanager_application::RequestId,
    ) -> bool {
        self.request_sessions.accept_affinity(attempt, request_id)
    }

    pub fn reject_process_affinity_read(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: taskmanager_application::FailureKind,
    ) -> bool {
        self.request_sessions.reject_affinity(attempt, failure)
    }

    pub fn close_process_affinity(&mut self) {
        self.request_sessions.close_affinity();
    }

    #[must_use]
    pub const fn process_affinity_state(&self) -> &taskmanager_application::ProcessAffinityState {
        self.request_sessions.affinity()
    }

    #[must_use]
    pub fn begin_process_batch(
        &mut self,
        intent: taskmanager_application::ProcessBatchIntent,
    ) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_batch(intent)
    }

    pub fn accept_process_batch(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: taskmanager_application::RequestId,
    ) -> bool {
        self.request_sessions.accept_batch(attempt, request_id)
    }

    pub fn reject_process_batch(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: taskmanager_application::FailureKind,
    ) -> bool {
        self.request_sessions.reject_batch(attempt, failure)
    }

    pub fn close_process_batch(&mut self) {
        self.request_sessions.close_batch();
    }

    #[must_use]
    pub const fn process_batch_state(&self) -> &taskmanager_application::ProcessBatchState {
        self.request_sessions.batch()
    }

    #[must_use]
    pub fn begin_smart_self_test(
        &mut self,
        intent: taskmanager_application::SmartSelfTestIntent,
    ) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_smart_self_test(intent)
    }

    pub fn accept_smart_self_test(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: taskmanager_application::RequestId,
    ) -> bool {
        self.request_sessions
            .accept_smart_self_test(attempt, request_id)
    }

    pub fn reject_smart_self_test(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: taskmanager_application::FailureKind,
    ) -> bool {
        self.request_sessions
            .reject_smart_self_test(attempt, failure)
    }

    pub fn close_smart_self_test(&mut self) {
        self.request_sessions.close_smart_self_test();
    }

    #[must_use]
    pub const fn smart_self_test_state(&self) -> &taskmanager_application::SmartSelfTestState {
        self.request_sessions.smart_self_test()
    }

    #[must_use]
    pub fn begin_gpu_engine_rows_request(
        &mut self,
        device_id: taskmanager_application::DeviceId,
    ) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_gpu_engine_rows(device_id)
    }

    pub fn accept_gpu_engine_rows_request(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: taskmanager_application::RequestId,
    ) -> bool {
        self.request_sessions
            .accept_gpu_engine_rows(attempt, request_id)
    }

    pub fn reject_gpu_engine_rows_request(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: taskmanager_application::FailureKind,
    ) -> bool {
        self.request_sessions
            .reject_gpu_engine_rows(attempt, failure)
    }

    pub fn close_gpu_engine_rows_request(&mut self) {
        self.request_sessions.close_gpu_engine_rows();
    }

    #[must_use]
    pub const fn gpu_engine_rows_state(&self) -> &taskmanager_application::GpuEngineRowsState {
        self.request_sessions.gpu_engine_rows()
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
    pub fn begin_shell_ui_action(
        &mut self,
        intent: taskmanager_application::ShellUiActionIntent,
    ) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_shell_ui_action(intent)
    }

    pub fn accept_shell_ui_action(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: taskmanager_application::RequestId,
    ) -> bool {
        self.request_sessions
            .accept_shell_ui_action(attempt, request_id)
    }

    pub fn reject_shell_ui_action(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: taskmanager_application::FailureKind,
    ) -> bool {
        self.request_sessions
            .reject_shell_ui_action(attempt, failure)
    }

    pub fn close_shell_ui_action(&mut self) {
        self.request_sessions.close_shell_ui_action();
    }

    #[must_use]
    pub const fn shell_ui_action_state(&self) -> &taskmanager_application::ShellUiActionState {
        self.request_sessions.shell_ui_action()
    }

    #[must_use]
    pub fn begin_network_escalation(&mut self) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_network_escalation()
    }

    pub fn accept_network_escalation(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: taskmanager_application::RequestId,
    ) -> bool {
        self.request_sessions
            .accept_network_escalation(attempt, request_id)
    }

    pub fn reject_network_escalation(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: taskmanager_application::FailureKind,
    ) -> bool {
        self.request_sessions
            .reject_network_escalation(attempt, failure)
    }

    pub fn close_network_escalation(&mut self) {
        self.request_sessions.close_network_escalation();
    }

    #[must_use]
    pub const fn network_escalation_state(
        &self,
    ) -> &taskmanager_application::NetworkEscalationState {
        self.request_sessions.network_escalation()
    }

    #[must_use]
    pub fn begin_startup_control(&mut self) -> taskmanager_application::ControlRequestId {
        self.projection.startup_control_requests.begin()
    }

    #[must_use]
    pub fn accept_startup_control(
        &mut self,
        request_id: taskmanager_application::ControlRequestId,
    ) -> bool {
        self.projection.startup_control_requests.accept(request_id)
    }

    #[must_use]
    pub fn begin_session_control(&mut self) -> taskmanager_application::ControlRequestId {
        self.projection.session_control_requests.begin()
    }

    #[must_use]
    pub fn accept_session_control(
        &mut self,
        request_id: taskmanager_application::ControlRequestId,
    ) -> bool {
        self.projection.session_control_requests.accept(request_id)
    }

    #[must_use]
    pub fn begin_service_control(
        &mut self,
        service_id: taskmanager_application::ServiceId,
        action: taskmanager_application::ServiceAction,
    ) -> taskmanager_application::ControlRequestId {
        self.projection
            .service_control_requests
            .begin(service_id, action)
    }

    #[must_use]
    pub fn accept_service_control(
        &mut self,
        request_id: taskmanager_application::ControlRequestId,
        service_id: &taskmanager_application::ServiceId,
        action: taskmanager_application::ServiceAction,
    ) -> bool {
        self.projection
            .service_control_requests
            .accept(request_id, service_id, action)
    }

    #[must_use]
    pub fn drain_alert_notifications(
        &mut self,
    ) -> Vec<taskmanager_application::DesktopNotificationRequest> {
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
}

#[cfg(test)]
#[path = "../../tests/headless/shell_app_direct_track.rs"]
mod tests;
