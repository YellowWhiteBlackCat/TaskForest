//! macOS process providers that have no safe on-box source yet.
//!
//! Each capability here completes with a typed `Unsupported` outcome rather
//! than fabricating an observation (ADR-019): per-process network (nettop
//! needs privileges; libpcap/Netiquette are not safely wrapped), per-process
//! GPU memory (Metal/IOKit unsafe), sandbox/entitlement facts, thread counts,
//! CPU affinity (no macOS API), resource-group limits, the off-Linux
//! per-process byte-accounting escalation path, and the per-fd open-files
//! insight (sysinfo surfaces only the fd COUNT; the fd -> target listing has
//! no safe accessor, so the optional facet publishes honest Unsupported).

use taskmanager_core::{
    FrozenProcessIdentity, ProcessGpuSnapshot, ProcessInsightSnapshot, ProcessOpenFiles,
    ProcessThreads,
};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::{
    ProcessAffinityControlProvider, ProcessAffinityProvider, ProcessGpuProvider,
    ProcessIsolationProvider, ProcessNetworkEscalationProvider, ProcessNetworkProvider,
    ProcessOpenFilesProvider, ProcessResourceControlProvider, ProcessThreadsProvider,
};

/// No safe macOS source for per-process network accounting yet (nettop needs
/// privileges; libpcap/Netiquette are not safely wrapped).
pub struct PendingProcessNetworkProvider;

impl ProcessNetworkProvider for PendingProcessNetworkProvider {
    fn observe(
        &mut self,
        _target: &FrozenProcessIdentity,
        _observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<taskmanager_core::ProcessNetworkSnapshot>, ProviderFailure>
    {
        Err(ProviderFailure::Unsupported)
    }
}

/// No safe macOS source for per-process GPU memory (Metal/IOKit unsafe).
pub struct PendingProcessGpuProvider;

impl ProcessGpuProvider for PendingProcessGpuProvider {
    fn observe(
        &mut self,
        _target: &FrozenProcessIdentity,
        _observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessGpuSnapshot>, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

/// No safe macOS source for sandbox/entitlement facts yet.
pub struct PendingProcessIsolationProvider;

impl ProcessIsolationProvider for PendingProcessIsolationProvider {
    fn observe(
        &mut self,
        _target: &FrozenProcessIdentity,
        _observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<taskmanager_core::ProcessIsolation>, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

pub struct PendingProcessThreadsProvider;

impl ProcessThreadsProvider for PendingProcessThreadsProvider {
    fn observe(
        &mut self,
        _target: &FrozenProcessIdentity,
        _observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessThreads>, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

/// No safe macOS source for the per-fd open-files insight: sysinfo exposes
/// only the fd COUNT (`Process::open_files`, used by the process list); the
/// fd -> target listing behind `proc_pidinfo(PROC_PIDLISTFDS)` has no safe
/// wrapper, so the optional facet completes honestly (ADR-019).
pub struct PendingProcessOpenFilesProvider;

impl ProcessOpenFilesProvider for PendingProcessOpenFilesProvider {
    fn observe(
        &mut self,
        _target: &FrozenProcessIdentity,
        _observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessOpenFiles>, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

/// macOS has no CPU-affinity API; the capability is honestly unsupported.
pub struct PendingProcessAffinityProvider;

impl ProcessAffinityProvider for PendingProcessAffinityProvider {
    fn affinity(&mut self, _target: &FrozenProcessIdentity) -> Result<Vec<u32>, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

/// macOS has no CPU-affinity API; the capability is honestly unsupported.
pub struct PendingProcessAffinityControlProvider;

impl ProcessAffinityControlProvider for PendingProcessAffinityControlProvider {
    fn set_affinity(
        &mut self,
        _target: &FrozenProcessIdentity,
        _cpus: &[u32],
    ) -> Result<(), ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

/// No safe macOS source for resource-group limits (launchd rlimits are not
/// per-process controllable through a safe API).
pub struct PendingProcessResourceControlProvider;

/// macOS has no per-process byte-accounting escalation path (no AF_PACKET /
/// SCM_RIGHTS chain off-Linux): the honest `Unsupported` answer.
pub struct PendingProcessNetworkEscalationProvider;

impl ProcessResourceControlProvider for PendingProcessResourceControlProvider {
    fn apply_limits(
        &mut self,
        _target: &FrozenProcessIdentity,
        _limits: &taskmanager_core::ResourceGroupLimitRequest,
    ) -> Result<(), ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

impl ProcessNetworkEscalationProvider for PendingProcessNetworkEscalationProvider {
    fn request_capture_escalation(&mut self) -> Result<(), ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}
