//! Request-session lifecycle forwarding for direct-track state.

use super::DirectTrackState;
use taskmanager_application::{
    GpuEngineRowsState, MsrReadoutState, ProcessAffinityState, ProcessBatchState, RaplPowerState,
    RequestAttemptId, SmartSelfTestState, SmbiosMemoryState,
};
use taskmanager_core::DeviceId;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessBatchIntent};
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_platform_contract::RequestId;

impl DirectTrackState {
    #[must_use]
    pub fn begin_process_affinity_read(
        &mut self,
        target: FrozenProcessIdentity,
    ) -> RequestAttemptId {
        self.request_sessions.begin_affinity(target)
    }

    pub fn accept_process_affinity_read(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions.accept_affinity(attempt, request_id)
    }

    pub fn reject_process_affinity_read(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions.reject_affinity(attempt, failure)
    }

    pub fn close_process_affinity(&mut self) {
        self.request_sessions.close_affinity();
    }

    #[must_use]
    pub const fn process_affinity_state(&self) -> &ProcessAffinityState {
        self.request_sessions.affinity()
    }

    #[must_use]
    pub fn begin_process_batch(&mut self, intent: ProcessBatchIntent) -> RequestAttemptId {
        self.request_sessions.begin_batch(intent)
    }

    pub fn accept_process_batch(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions.accept_batch(attempt, request_id)
    }

    pub fn reject_process_batch(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions.reject_batch(attempt, failure)
    }

    pub fn close_process_batch(&mut self) {
        self.request_sessions.close_batch();
    }

    #[must_use]
    pub const fn process_batch_state(&self) -> &ProcessBatchState {
        self.request_sessions.batch()
    }

    #[must_use]
    pub fn begin_smart_self_test(&mut self, intent: SmartSelfTestIntent) -> RequestAttemptId {
        self.request_sessions.begin_smart_self_test(intent)
    }

    pub fn accept_smart_self_test(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_smart_self_test(attempt, request_id)
    }

    pub fn reject_smart_self_test(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions
            .reject_smart_self_test(attempt, failure)
    }

    pub fn close_smart_self_test(&mut self) {
        self.request_sessions.close_smart_self_test();
    }

    #[must_use]
    pub const fn smart_self_test_state(&self) -> &SmartSelfTestState {
        self.request_sessions.smart_self_test()
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
    pub const fn gpu_engine_rows_state(&self) -> &GpuEngineRowsState {
        self.request_sessions.gpu_engine_rows()
    }

    #[must_use]
    pub fn begin_smbios_memory_request(&mut self) -> RequestAttemptId {
        self.request_sessions.begin_smbios_memory()
    }

    pub fn accept_smbios_memory_request(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_smbios_memory(attempt, request_id)
    }

    pub fn reject_smbios_memory_request(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions.reject_smbios_memory(attempt, failure)
    }

    pub fn close_smbios_memory_request(&mut self) {
        self.request_sessions.close_smbios_memory();
    }

    #[must_use]
    pub const fn smbios_memory_state(&self) -> &SmbiosMemoryState {
        self.request_sessions.smbios_memory()
    }

    #[must_use]
    pub fn begin_rapl_power_request(&mut self) -> RequestAttemptId {
        self.request_sessions.begin_rapl_power()
    }

    pub fn accept_rapl_power_request(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions.accept_rapl_power(attempt, request_id)
    }

    pub fn reject_rapl_power_request(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions.reject_rapl_power(attempt, failure)
    }

    pub fn close_rapl_power_request(&mut self) {
        self.request_sessions.close_rapl_power();
    }

    #[must_use]
    pub const fn rapl_power_state(&self) -> &RaplPowerState {
        self.request_sessions.rapl_power()
    }

    #[must_use]
    pub fn begin_msr_readout_request(&mut self) -> RequestAttemptId {
        self.request_sessions.begin_msr_readout()
    }

    pub fn accept_msr_readout_request(
        &mut self,
        attempt: RequestAttemptId,
        request_id: RequestId,
    ) -> bool {
        self.request_sessions
            .accept_msr_readout(attempt, request_id)
    }

    pub fn reject_msr_readout_request(
        &mut self,
        attempt: RequestAttemptId,
        failure: FailureKind,
    ) -> bool {
        self.request_sessions.reject_msr_readout(attempt, failure)
    }

    pub fn close_msr_readout_request(&mut self) {
        self.request_sessions.close_msr_readout();
    }

    #[must_use]
    pub const fn msr_readout_state(&self) -> &MsrReadoutState {
        self.request_sessions.msr_readout()
    }
}
