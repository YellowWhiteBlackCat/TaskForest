//! Pure application effects emitted by the reducer.
//!
//! Native request/event ports live in `taskmanager-platform-contract` and are
//! composed through [`crate::PlatformHandle`]. This module intentionally
//! contains no adapter trait and no all-capabilities supertrait.

use crate::platform::{
    CommandLaunchRequest, DesktopNotificationRequest, DirectoryUsageRequest, GpuEngineRowsRequest,
    NpuInventoryRequest, ProcessAffinityControlRequest, ProcessAffinityRequest,
    ProcessResourceControlRequest, ResourceRevealRequest, ServiceDependenciesRequest,
    ServiceLogSnapshotRequest, ServiceLogStreamRequest, SetupScriptRequest, SmartControlRequest,
    UrlOpenRequest,
};
use crate::{
    ControlRequestId, FrozenProcessIdentity, ProcessBatchIntent, ProcessSignal, RefreshRequest,
    ServiceAction, ServiceId, SessionControlAction, SessionId, StartupControlRequest,
};

/// Immutable service-control target captured when a destructive service action
/// (Stop / Restart / Disable) is requested. The provider-issued id and action
/// are frozen at request time so a later list refresh cannot silently change
/// what the confirmation represents or what the reducer submits — the
/// service-control analogue of [`FrozenProcessIdentity`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceControlTarget {
    pub service_id: ServiceId,
    pub action: ServiceAction,
}

/// Immutable login-session control target captured at the renderer boundary.
/// The provider-issued session identity, action and correlation id travel
/// together so a later refresh cannot silently retarget a native action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionControlTarget {
    pub request_id: ControlRequestId,
    pub session_id: SessionId,
    pub action: SessionControlAction,
}

/// Platform work requested by the pure reducer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformEffect {
    Refresh(RefreshRequest),
    EndTask(FrozenProcessIdentity),
    /// Submit one semantic process signal through the shared process-control
    /// port. Frontends own the menu shape; the shell/application owns the
    /// frozen identity and native dispatch.
    ProcessSignal {
        target: FrozenProcessIdentity,
        signal: ProcessSignal,
    },
    /// Submit a batch process-control action (Kill / Suspend / Resume /
    /// SetPriority) over a frozen target set, mirroring GPUI's batch path. The
    /// explicit shared `InteractionState` confirmation transition gates destructive
    /// actions; Cancel/Escape emit nothing.
    ExecuteBatch(ProcessBatchIntent),
    /// Submit a gated service-control action. Only emitted by the explicit
    /// confirmation transition — Cancel/Escape clear the overlay with no effect.
    ServiceControl(ServiceControlTarget),
    /// Submit a direct login-session action through the session-control port.
    SessionControl(SessionControlTarget),
    /// Submit a gated startup-entry enable/disable action through the startup-
    /// control port. Only emitted by the explicit confirmation transition —
    /// Cancel/Escape clear the overlay with no effect (mirrors SessionControl).
    StartupControl(StartupControlRequest),
    /// Reveal a process's executable location in the platform file manager.
    /// Routed through the integration resource-reveal port so the frontend
    /// never spawns the opener itself (the dependency firewall owns native
    /// command execution in the platform adapter).
    RevealResource(ResourceRevealRequest),
    /// Open a URL (e.g. a web search) in the platform's default handler, routed
    /// through the integration url-open port for the same firewall reason.
    OpenUrl(UrlOpenRequest),
    /// Submit the five per-process insight domains (network / GPU / resources /
    /// isolation / threads) for the frozen target under one application-owned
    /// revision. The correlated outcomes arrive as `ProjectedProcessInsights`
    /// in the next `PlatformEventBatch`.
    ProcessInsights(FrozenProcessIdentity),
    /// Submit the system-wide per-process network capture escalation request
    /// (capability `process.network.escalation`). Emitted when the Insights
    /// network facet reports `RequiresEscalation` and the user accepts the
    /// escalation pill (GPUI's "Enable per-process network" path). One-shot,
    /// no target, no confirmation gate.
    ProcessNetworkEscalation,
    /// Submit (or follow up) an incremental service log-stream request. The
    /// shell owns the 1-second follow throttle; the provider echoes
    /// `ServiceUpdate::Logs` / `LogStream` back into the batch.
    ServiceLogStream(ServiceLogStreamRequest),
    /// Deliver a desktop notification for a fired alert that passed the pure
    /// [`taskmanager_core::alerts::NotificationGate`] (BN-07). The gate lives in the command/reducer
    /// path so every frontend gets the same cooldown/quiet-hours policy.
    DesktopNotification(DesktopNotificationRequest),
    /// Submit a directory-usage scan lifecycle request (start / resume /
    /// cancel). The scan runs on its own bounded lane; progress and terminal
    /// results arrive as `DirectoryUsageEvent` publications in the next batch.
    /// Previously GPUI-only (`submit_directory_usage`).
    DirectoryUsage(DirectoryUsageRequest),
    /// Submit a gated SMART control action (start self-test / stop local
    /// tracking). Previously GPUI-only (`submit_smart_control`).
    SmartControl(SmartControlRequest),
    /// Submit the dependency-graph query for one service; the provider echoes
    /// a request-correlated `ServiceUpdate::Dependencies` back into the batch.
    ServiceDependencies(ServiceDependenciesRequest),
    /// Submit a one-shot service log snapshot (a log panel's initial fill
    /// before any [`PlatformEffect::ServiceLogStream`] follow-up).
    ServiceLogSnapshot(ServiceLogSnapshotRequest),
    /// Submit the per-process CPU-affinity READ for the frozen target; the
    /// correlated answer arrives as `PlatformEventBatch::process_affinity_events`.
    /// Previously GPUI-only (`submit_process_affinity`).
    ProcessAffinity(ProcessAffinityRequest),
    /// Submit a per-process CPU-affinity WRITE over the frozen target; the
    /// completion arrives as `ProcessEvent::AffinityApplied`. Previously
    /// GPUI-only (`submit_process_affinity_control`).
    ProcessAffinityControl(ProcessAffinityControlRequest),
    /// Launch a command through the integration port so the dependency
    /// firewall keeps native process spawning in the platform adapter.
    /// Previously GPUI-only (`submit_command_launch`).
    CommandLaunch(CommandLaunchRequest),
    /// Submit a first-run setup-script action (the install/revert exit-code
    /// receipt contract). Previously GPUI-only (`submit_setup_script`).
    SetupScript(SetupScriptRequest),
    /// Submit per-process resource-group limit writes. The completion arrives
    /// as `ProcessEvent::ResourceLimitsApplied`. Reserved UI lane (G-19): the
    /// typed vocabulary exists so a consumer can adopt it without another
    /// shell change; no frontend renders it yet.
    ResourceGroupControl(ProcessResourceControlRequest),
    /// Submit a per-engine GPU utilization read for one device (capability
    /// `telemetry.gpu.engines`). The privileged PMU helper runs once per
    /// request on its own bounded lane; rows or a typed failure arrive as
    /// `PlatformEventBatch::gpu_engine_rows_events`. Previously GPUI-only
    /// (the `invoke_perf_helper` poll loop).
    GpuEngineRows(GpuEngineRowsRequest),
    /// Submit an NPU accelerator inventory read (capability
    /// `accelerator.npu`). The provider enumerates once per request on its
    /// own bounded lane; a sorted device list (empty on a no-NPU host) or a
    /// typed failure arrives as `PlatformEventBatch::npu_inventory_events`.
    NpuInventory(NpuInventoryRequest),
}
