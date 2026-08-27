//! Shared command-request lifecycles for renderer tracks.
//!
//! The platform fold publishes correlated terminals once. This component
//! accepts only terminals matching the active application-owned session and
//! removes stale or duplicate terminals before any renderer sees them.

use taskmanager_application::{
    CapabilityId, DeviceId, FailureKind, FrozenProcessIdentity, GpuEngineRowsEvent,
    GpuEngineRowsSession, GpuEngineRowsState, NetworkEscalationSession, NetworkEscalationState,
    PlatformEventBatch, ProcessAffinitySession, ProcessAffinityState, ProcessBatchIntent,
    ProcessBatchSession, ProcessBatchState, ProcessEvent, RequestAttemptId, RequestId, ShellEvent,
    ShellUiActionIntent, ShellUiActionSession, ShellUiActionState, SmartSelfTestIntent,
    SmartSelfTestSession, SmartSelfTestState,
};

use super::{BatchFoldOutput, ShellApp};

#[derive(Clone, Debug, Default)]
pub(crate) struct RequestSessions {
    affinity: ProcessAffinitySession,
    batch: ProcessBatchSession,
    smart_self_test: SmartSelfTestSession,
    gpu_engine_rows: GpuEngineRowsSession,
    shell_ui_action: ShellUiActionSession,
    network_escalation: NetworkEscalationSession,
}

impl ShellApp {
    #[must_use]
    pub const fn process_affinity_state(&self) -> &ProcessAffinityState {
        self.request_sessions.affinity()
    }

    pub fn close_process_affinity(&mut self) {
        self.request_sessions.close_affinity();
    }

    #[must_use]
    pub const fn process_batch_state(&self) -> &ProcessBatchState {
        self.request_sessions.batch()
    }

    #[must_use]
    pub const fn smart_self_test_state(&self) -> &SmartSelfTestState {
        self.request_sessions.smart_self_test()
    }

    #[must_use]
    pub const fn gpu_engine_rows_state(&self) -> &GpuEngineRowsState {
        self.request_sessions.gpu_engine_rows()
    }

    #[must_use]
    pub fn begin_gpu_engine_rows_request(&mut self, device_id: DeviceId) -> RequestAttemptId {
        self.request_sessions.begin_gpu_engine_rows(device_id)
    }

    pub fn accept_gpu_engine_rows_request(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_gpu_engine_rows(attempt, request_id)
    }

    pub fn reject_gpu_engine_rows_request(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions
            .reject_gpu_engine_rows(attempt, failure)
    }

    pub fn close_gpu_engine_rows_request(&mut self) {
        self.request_sessions.close_gpu_engine_rows();
    }

    #[must_use]
    pub const fn shell_ui_action_state(&self) -> &ShellUiActionState {
        self.request_sessions.shell_ui_action()
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
    pub const fn network_escalation_state(&self) -> &NetworkEscalationState {
        self.request_sessions.network_escalation()
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
}

impl RequestSessions {
    pub(crate) fn seed_affinity(
        &mut self,
        ready: Option<taskmanager_application::ProcessAffinityReady>,
    ) {
        self.affinity.close();
        let Some(ready) = ready else {
            return;
        };
        let attempt = self.affinity.begin_attempt(ready.target.clone());
        let _ = self.affinity.accept_attempt(attempt, ready.request_id);
        let _ = self
            .affinity
            .complete(ready.request_id, ready.target, ready.cpus);
    }

    #[must_use]
    pub(crate) const fn affinity(&self) -> &ProcessAffinityState {
        self.affinity.state()
    }

    #[must_use]
    pub(crate) const fn batch(&self) -> &ProcessBatchState {
        self.batch.state()
    }

    #[must_use]
    pub(crate) const fn smart_self_test(&self) -> &SmartSelfTestState {
        self.smart_self_test.state()
    }

    #[must_use]
    pub(crate) const fn gpu_engine_rows(&self) -> &GpuEngineRowsState {
        self.gpu_engine_rows.state()
    }

    #[must_use]
    pub(crate) const fn shell_ui_action(&self) -> &ShellUiActionState {
        self.shell_ui_action.state()
    }

    #[must_use]
    pub(crate) const fn network_escalation(&self) -> &NetworkEscalationState {
        self.network_escalation.state()
    }

    #[must_use]
    pub(crate) fn begin_affinity(&mut self, target: FrozenProcessIdentity) -> RequestAttemptId {
        self.affinity.begin_attempt(target)
    }

    pub(crate) fn accept_affinity(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.affinity.accept_attempt(attempt, request_id)
    }

    pub(crate) fn reject_affinity(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.affinity.reject_attempt(attempt, failure)
    }

    pub(crate) fn close_affinity(&mut self) {
        self.affinity.close();
    }

    #[must_use]
    pub(crate) fn begin_batch(&mut self, intent: ProcessBatchIntent) -> RequestAttemptId {
        self.batch.begin_attempt(intent)
    }

    pub(crate) fn accept_batch(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.batch.accept_attempt(attempt, request_id)
    }

    pub(crate) fn reject_batch(&mut self, attempt: RequestAttemptId, failure: FailureKind) -> bool {
        self.batch.reject_attempt(attempt, failure)
    }

    pub(crate) fn close_batch(&mut self) {
        self.batch.close();
    }

    #[must_use]
    pub(crate) fn begin_smart_self_test(
        &mut self,
        intent: SmartSelfTestIntent,
    ) -> RequestAttemptId {
        self.smart_self_test.begin_attempt(intent)
    }

    pub(crate) fn accept_smart_self_test(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.smart_self_test.accept_attempt(attempt, request_id)
    }

    pub(crate) fn reject_smart_self_test(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.smart_self_test.reject_attempt(attempt, failure)
    }

    pub(crate) fn close_smart_self_test(&mut self) {
        self.smart_self_test.close();
    }

    #[must_use]
    pub(crate) fn begin_gpu_engine_rows(&mut self, device_id: DeviceId) -> RequestAttemptId {
        self.gpu_engine_rows.begin_attempt(device_id)
    }

    pub(crate) fn accept_gpu_engine_rows(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.gpu_engine_rows.accept_attempt(attempt, request_id)
    }

    pub(crate) fn reject_gpu_engine_rows(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.gpu_engine_rows.reject_attempt(attempt, failure)
    }

    pub(crate) fn close_gpu_engine_rows(&mut self) {
        self.gpu_engine_rows.close();
    }

    #[must_use]
    pub(crate) fn begin_shell_ui_action(
        &mut self,
        intent: ShellUiActionIntent,
    ) -> RequestAttemptId {
        self.shell_ui_action.begin_attempt(intent)
    }

    pub(crate) fn accept_shell_ui_action(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.shell_ui_action.accept_attempt(attempt, request_id)
    }

    pub(crate) fn reject_shell_ui_action(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.shell_ui_action.reject_attempt(attempt, failure)
    }

    pub(crate) fn close_shell_ui_action(&mut self) {
        self.shell_ui_action.close();
    }

    #[must_use]
    pub(crate) fn begin_network_escalation(&mut self) -> RequestAttemptId {
        self.network_escalation.begin_attempt()
    }

    pub(crate) fn accept_network_escalation(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.network_escalation.accept_attempt(attempt, request_id)
    }

    pub(crate) fn reject_network_escalation(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.network_escalation.reject_attempt(attempt, failure)
    }

    pub(crate) fn close_network_escalation(&mut self) {
        self.network_escalation.close();
    }

    /// Remove request terminals before the projection fold unless the active
    /// session accepts their exact request, capability and payload identity.
    pub(crate) fn filter_platform_terminals(&mut self, batch: &mut PlatformEventBatch) {
        batch.gpu_engine_rows_events.retain(|correlated| {
            if correlated.capability != CapabilityId::TELEMETRY_GPU_ENGINES {
                return false;
            }
            let GpuEngineRowsEvent::Update(snapshot) = &correlated.event;
            self.gpu_engine_rows
                .complete(correlated.request_id, snapshot.clone())
        });
        batch.process_events.retain(|correlated| {
            if !matches!(correlated.event, ProcessEvent::NetworkCaptureEscalated) {
                return true;
            }
            correlated.capability == CapabilityId::PROCESS_NETWORK_ESCALATION
                && self.network_escalation.complete(correlated.request_id)
        });
        batch
            .shell_events
            .retain(|correlated| match &correlated.event {
                ShellEvent::NotificationDelivered => {
                    correlated.capability == CapabilityId::DESKTOP_NOTIFY
                }
                event => self.shell_ui_action.complete(
                    correlated.request_id,
                    &correlated.capability,
                    event,
                ),
            });
    }

    /// Resolve fold-produced terminals and operation failures once, before
    /// renderer-specific history, feedback or invalidation sees the output.
    pub(crate) fn accept_fold_terminals(&mut self, output: &mut BatchFoldOutput) {
        output.process_affinity_results.retain(|result| {
            self.affinity.complete(
                result.request_id,
                result.target.clone(),
                result.cpus.clone(),
            )
        });
        if !output.process_affinity_results.is_empty() {
            output.changes.process_affinity = true;
        }

        output
            .batch_results
            .retain(|(request_id, result)| self.batch.complete(*request_id, result.clone()));
        if !output.batch_results.is_empty() {
            output.changes.process_batch = true;
        }

        output.smart_self_test_results.retain(|result| {
            self.smart_self_test
                .complete(result.request_id, &result.target)
        });
        if !output.smart_self_test_results.is_empty() {
            output.changes.smart_self_test = true;
        }

        for failure in &output.failures {
            if failure.capability == CapabilityId::PROCESS_AFFINITY
                && self.affinity.fail(failure.request_id, failure.kind)
            {
                output.changes.process_affinity = true;
            }
            if failure.capability == CapabilityId::PROCESS_CONTROL
                && self.batch.fail(failure.request_id, failure.kind)
            {
                output.changes.process_batch = true;
            }
            if failure.capability == CapabilityId::SMART_CONTROL
                && self.smart_self_test.fail(failure.request_id, failure.kind)
            {
                output.changes.smart_self_test = true;
            }
            if failure.capability == CapabilityId::TELEMETRY_GPU_ENGINES {
                let _ = self.gpu_engine_rows.fail(failure.request_id, failure.kind);
            }
            if failure.capability == CapabilityId::PROCESS_NETWORK_ESCALATION {
                let _ = self
                    .network_escalation
                    .fail(failure.request_id, failure.kind);
            }
            let _ =
                self.shell_ui_action
                    .fail(failure.request_id, &failure.capability, failure.kind);
        }
    }
}
