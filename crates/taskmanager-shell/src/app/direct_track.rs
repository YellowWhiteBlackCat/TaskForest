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

use taskmanager_application::process_sort::compare_processes;
use taskmanager_application::{InteractionState, ServiceDependenciesLifecycle};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessItem};
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::startup::StartupEntry;
use taskmanager_platform_contract::{CapabilitySnapshot, RequestId};

use super::process_rows::ProcessRowId;
use super::sort_axis::sort_axis;
use super::sorting::{InfoSortCol, InfoTable, SortCol, SortDir};
use crate::ProcessStatusFilter;
use taskmanager_core::core::process::ProcessLiveKey;

mod inventory_sort;
mod process_selection;
mod process_viewing;

pub use process_selection::identity_range;

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
            crate::fixture::DirectTrackSeedFact::Processes(processes) => {
                self.projection.processes = Some(std::sync::Arc::new(processes));
                self.projection.processes_observed_at_ms = 0;
                self.projection.process_revision =
                    self.projection.process_revision.saturating_add(1);
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
                .capability_status(&taskmanager_platform_contract::CapabilityId::PROCESS_CONTROL),
        )
    }

    #[must_use]
    pub fn process_control_capability_allowed(&self) -> bool {
        super::process_control::process_control_capability_allowed(
            self.projection
                .capability_status(&taskmanager_platform_contract::CapabilityId::PROCESS_CONTROL),
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
    pub fn process_control_intent(
        &self,
        action: taskmanager_core::core::process::ProcessBatchAction,
    ) -> Option<taskmanager_core::core::process::ProcessBatchIntent> {
        let selected: Vec<_> = self.selection.rows().iter().copied().collect();
        super::process_control::process_control_intent(
            self.projection.processes_slice(),
            self.selection.active_row(),
            &selected,
            action,
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
        sink: Option<std::sync::Arc<dyn taskmanager_core::core::history::HistoryRecordSink>>,
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
            let processes = match &correlated.event {
                taskmanager_application::ProcessEvent::Snapshot(processes) => processes,
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
        edit: taskmanager_application::ManagedAlertRuleEdit,
    ) -> Result<
        taskmanager_application::ManagedAlertRuleEditOutcome,
        taskmanager_core::core::alerts::AlertRuleTransferError,
    > {
        self.projection.alert_center.edit_rules(edit)
    }

    pub fn set_alert_policy(&mut self, policy: taskmanager_core::core::alerts::NotificationPolicy) {
        self.projection.alert_center.set_policy(policy);
    }

    /// Clear the shared alert transition history without changing the active
    /// evaluator or notification policy.
    pub fn clear_alert_event_history(&mut self) {
        self.projection.alert_center.clear_event_history();
    }

    /// Install deterministic alert transition history through the direct
    /// track's fixture boundary; production events still come only from the
    /// shared evaluator.
    pub fn replace_alert_event_history(
        &mut self,
        events: Vec<taskmanager_core::core::alerts::AlertEvent>,
    ) {
        self.projection.alert_center.replace_event_history(events);
    }

    #[must_use]
    pub fn evaluate_alerts(
        &mut self,
        snapshot: &taskmanager_core::core::metrics::SystemSnapshot,
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
        active: Vec<taskmanager_core::core::alerts::Alert>,
    ) -> u64 {
        self.projection.alert_active = active;
        self.projection.refresh_count = self.projection.refresh_count.saturating_add(1);
        self.projection.refresh_count
    }

    pub fn begin_process_control(
        &mut self,
        request_id: RequestId,
        target: taskmanager_core::core::process::FrozenProcessIdentity,
        kind: super::ProcessControlKind,
    ) {
        self.projection
            .begin_process_control(request_id, target, kind);
    }

    #[must_use]
    pub fn begin_process_affinity_read(
        &mut self,
        target: taskmanager_core::core::process::FrozenProcessIdentity,
    ) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_affinity(target)
    }

    pub fn accept_process_affinity_read(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions.accept_affinity(attempt, request_id)
    }

    pub fn reject_process_affinity_read(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: FailureKind,
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
        intent: taskmanager_core::core::process::ProcessBatchIntent,
    ) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_batch(intent)
    }

    pub fn accept_process_batch(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions.accept_batch(attempt, request_id)
    }

    pub fn reject_process_batch(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: FailureKind,
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
        intent: taskmanager_core::core::system_health::SmartSelfTestIntent,
    ) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_smart_self_test(intent)
    }

    pub fn accept_smart_self_test(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_smart_self_test(attempt, request_id)
    }

    pub fn reject_smart_self_test(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: FailureKind,
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
        device_id: taskmanager_core::core::identity::DeviceId,
    ) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_gpu_engine_rows(device_id)
    }

    pub fn accept_gpu_engine_rows_request(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_gpu_engine_rows(attempt, request_id)
    }

    pub fn reject_gpu_engine_rows_request(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: FailureKind,
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

    #[must_use]
    pub fn begin_smbios_memory_request(&mut self) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_smbios_memory()
    }

    pub fn accept_smbios_memory_request(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_smbios_memory(attempt, request_id)
    }

    pub fn reject_smbios_memory_request(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions.reject_smbios_memory(attempt, failure)
    }

    pub fn close_smbios_memory_request(&mut self) {
        self.request_sessions.close_smbios_memory();
    }

    #[must_use]
    pub const fn smbios_memory_state(&self) -> &taskmanager_application::SmbiosMemoryState {
        self.request_sessions.smbios_memory()
    }

    #[must_use]
    pub fn begin_rapl_power_request(&mut self) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_rapl_power()
    }

    pub fn accept_rapl_power_request(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions.accept_rapl_power(attempt, request_id)
    }

    pub fn reject_rapl_power_request(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions.reject_rapl_power(attempt, failure)
    }

    pub fn close_rapl_power_request(&mut self) {
        self.request_sessions.close_rapl_power();
    }

    #[must_use]
    pub const fn rapl_power_state(&self) -> &taskmanager_application::RaplPowerState {
        self.request_sessions.rapl_power()
    }

    #[must_use]
    pub fn begin_msr_readout_request(&mut self) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_msr_readout()
    }

    pub fn accept_msr_readout_request(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_msr_readout(attempt, request_id)
    }

    pub fn reject_msr_readout_request(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions.reject_msr_readout(attempt, failure)
    }

    pub fn close_msr_readout_request(&mut self) {
        self.request_sessions.close_msr_readout();
    }

    #[must_use]
    pub const fn msr_readout_state(&self) -> &taskmanager_application::MsrReadoutState {
        self.request_sessions.msr_readout()
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
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_shell_ui_action(attempt, request_id)
    }

    pub fn reject_shell_ui_action(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: FailureKind,
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
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_network_escalation(attempt, request_id)
    }

    pub fn reject_network_escalation(
        &mut self,
        attempt: taskmanager_application::RequestAttemptId,
        failure: FailureKind,
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
        service_id: taskmanager_core::core::target::ServiceId,
        action: taskmanager_core::core::services::ServiceAction,
    ) -> taskmanager_application::ControlRequestId {
        self.projection
            .service_control_requests
            .begin(service_id, action)
    }

    #[must_use]
    pub fn accept_service_control(
        &mut self,
        request_id: taskmanager_application::ControlRequestId,
        service_id: &taskmanager_core::core::target::ServiceId,
        action: taskmanager_core::core::services::ServiceAction,
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
