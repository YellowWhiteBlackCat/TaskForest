//! Renderer-neutral request helpers for the typed on-demand effect lanes
//! (G-03/G-19, ADR-027).
//!
//! Before these helpers, nine `PlatformClient` submit methods were reachable
//! only from GPUI (which bypasses the shell); TUI/Iced had no typed effect to
//! queue. Each helper freezes the caller-supplied request into the matching
//! [`taskmanager_application::PlatformEffect`] variant so a frontend never
//! constructs the payload itself — mirroring
//! [`super::ShellApp::request_process_network_escalation`]. Submissibility
//! checks (frozen identity authority) stay on the client, exactly like the
//! existing lanes.
use super::ShellApp;
use taskmanager_application::{
    DirectoryUsageRequest, GpuEngineRowsRequest, MsrReadoutRequest, NpuInventoryRequest,
    PlatformEffect, ProcessAffinityControlRequest, ProcessAffinityRequest, RaplPowerRequest,
    ServiceDependenciesRequest, ServiceLogSnapshotRequest, SmartControlRequest,
    SmbiosMemoryRequest,
};
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_core::core::target::ServiceId;

impl ShellApp {
    /// Queue a directory-usage scan lifecycle request (start / resume /
    /// cancel). Progress and terminal results arrive as
    /// `PlatformEventBatch::directory_usage_events`.
    #[must_use]
    pub fn request_directory_usage(request: DirectoryUsageRequest) -> PlatformEffect {
        PlatformEffect::DirectoryUsage(request)
    }

    /// Queue a per-engine GPU utilization read for `device_id` (capability
    /// `telemetry.gpu.engines`). The privileged PMU helper runs once per
    /// request on its own bounded lane; the answer arrives as
    /// `PlatformEventBatch::gpu_engine_rows_events`; the shared request session
    /// admits only the active matching request/device terminal and owns the
    /// accepted payload. Frontends pace their
    /// own requests (the escalation discipline forbids auto-triggering the
    /// OS-native prompt).
    #[must_use]
    pub fn request_gpu_engine_rows(device_id: DeviceId) -> PlatformEffect {
        PlatformEffect::GpuEngineRows(GpuEngineRowsRequest { device_id })
    }

    /// Queue an NPU accelerator inventory read (capability
    /// `accelerator.npu`). The provider enumerates once per request on its
    /// own bounded lane; the answer arrives as
    /// `PlatformEventBatch::npu_inventory_events` and lands in
    /// [`super::SystemProjectionStore::npu_inventory`].
    #[must_use]
    pub fn request_npu_inventory() -> PlatformEffect {
        PlatformEffect::NpuInventory(NpuInventoryRequest {})
    }

    /// Queue a SMBIOS memory-inventory read (capability
    /// `telemetry.memory.smbios`). The privileged helper runs once per request
    /// on its own bounded lane; the answer arrives as
    /// `PlatformEventBatch::smbios_memory_events` and the shared request
    /// session admits only the active matching request terminal. Frontends
    /// pace their own requests (the escalation discipline forbids
    /// auto-triggering the OS-native prompt).
    #[must_use]
    pub fn request_smbios_memory() -> PlatformEffect {
        PlatformEffect::SmbiosMemory(SmbiosMemoryRequest::Refresh)
    }

    /// Queue a CPU package-power read (capability
    /// `telemetry.cpu.package_power`). The privileged RAPL helper samples once
    /// per request on its own bounded lane; the answer arrives as
    /// `PlatformEventBatch::rapl_power_events` and the shared request session
    /// admits only the active matching request terminal. Frontends pace their
    /// own requests (the escalation discipline forbids auto-triggering the
    /// OS-native prompt).
    #[must_use]
    pub fn request_rapl_power() -> PlatformEffect {
        PlatformEffect::RaplPower(RaplPowerRequest::Refresh)
    }

    /// Queue a CPU MSR readout (capability `telemetry.cpu.msr`). The
    /// privileged MSR helper reads once per request on its own bounded lane;
    /// the answer arrives as `PlatformEventBatch::msr_readout_events` and the
    /// shared request session admits only the active matching request
    /// terminal. Frontends pace their own requests (the escalation discipline
    /// forbids auto-triggering the OS-native prompt).
    #[must_use]
    pub fn request_msr_readouts() -> PlatformEffect {
        PlatformEffect::MsrReadout(MsrReadoutRequest::Refresh)
    }

    /// Queue the dependency-graph query for one service; the provider echoes
    /// `ServiceUpdate::Dependencies` back into the batch.
    #[must_use]
    pub fn request_service_dependencies(service_id: ServiceId) -> PlatformEffect {
        PlatformEffect::ServiceDependencies(ServiceDependenciesRequest { service_id })
    }

    /// Queue a one-shot service log snapshot (a log panel's initial fill,
    /// before any `ServiceLogStream` follow-up).
    #[must_use]
    pub fn request_service_log_snapshot(service_id: ServiceId) -> PlatformEffect {
        PlatformEffect::ServiceLogSnapshot(ServiceLogSnapshotRequest { service_id })
    }

    /// Queue a gated SMART control action (start self-test / stop tracking).
    #[must_use]
    pub fn request_smart_control(request: SmartControlRequest) -> PlatformEffect {
        PlatformEffect::SmartControl(request)
    }

    /// Freeze the selected process identity and queue the per-process
    /// CPU-affinity READ; the correlated answer arrives as
    /// `PlatformEventBatch::process_affinity_events` and lands in
    /// [`ShellApp::process_affinity_state`]. `None` when the selection is
    /// not a trustworthy process, mirroring
    /// [`ShellApp::request_process_insights`].
    #[must_use]
    pub fn request_process_affinity(&mut self) -> Option<PlatformEffect> {
        Some(PlatformEffect::ProcessAffinity(ProcessAffinityRequest {
            target: self.selected_process_identity()?,
        }))
    }

    /// Freeze the selected process identity and queue a CPU-affinity WRITE
    /// over `cpus`; the completion arrives as `ProcessEvent::AffinityApplied`
    /// and is emitted once by the batch fold into the shared feedback reducer.
    #[must_use]
    pub fn request_process_affinity_control(&mut self, cpus: Vec<u32>) -> Option<PlatformEffect> {
        self.selected_process_identity()
            .and_then(|target| self.request_process_affinity_control_for(target, cpus))
    }

    /// Queue a CPU-affinity WRITE for an already frozen identity. Frontends
    /// that keep an editor open across process-list refreshes must use this
    /// exact-target variant; resolving the live selection again at Apply time
    /// could retarget a recycled PID.
    #[must_use]
    pub fn request_process_affinity_control_for(
        &mut self,
        target: FrozenProcessIdentity,
        cpus: Vec<u32>,
    ) -> Option<PlatformEffect> {
        Some(PlatformEffect::ProcessAffinityControl(
            ProcessAffinityControlRequest { target, cpus },
        ))
    }
}
