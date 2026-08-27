//! Process-domain provider SPI: inventory, per-process telemetry insights, and
//! affinity, resource, and signal/control mutation lanes.

use taskmanager_core::{
    FrozenProcessIdentity, ProcessBatchIntent, ProcessBatchResult, ProcessEnvironment,
    ProcessGpuSnapshot, ProcessInsightSnapshot, ProcessIsolation, ProcessItem,
    ProcessNetworkSnapshot, ProcessOpenFiles, ProcessResourceSnapshot, ProcessSignal,
    ProcessThreads, ResourceGroupLimitRequest,
};
use taskmanager_platform_contract::{PartialSourceSnapshot, ProviderFailure};

pub trait ProcessListProvider: Send + 'static {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<PartialSourceSnapshot<ProcessItem>, ProviderFailure>;
}

pub trait ProcessNetworkProvider: Send + 'static {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessNetworkSnapshot>, ProviderFailure>;
}

pub trait ProcessGpuProvider: Send + 'static {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessGpuSnapshot>, ProviderFailure>;
}

pub trait ProcessResourcesProvider: Send + 'static {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessResourceSnapshot>, ProviderFailure>;
}

pub trait ProcessIsolationProvider: Send + 'static {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessIsolation>, ProviderFailure>;
}

pub trait ProcessThreadsProvider: Send + 'static {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessThreads>, ProviderFailure>;
}

pub trait ProcessOpenFilesProvider: Send + 'static {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessOpenFiles>, ProviderFailure>;
}

pub trait ProcessEnvironmentProvider: Send + 'static {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessEnvironment>, ProviderFailure>;
}

pub trait ProcessAffinityProvider: Send + 'static {
    fn affinity(&mut self, target: &FrozenProcessIdentity) -> Result<Vec<u32>, ProviderFailure>;
}

/// Affinity mutation is independent from signal/batch control: it uses a
/// different kernel operation, privilege policy, failure surface, and runtime
/// lane. Native adapters may support affinity observation without supporting
/// mutation.
pub trait ProcessAffinityControlProvider: Send + 'static {
    fn set_affinity(
        &mut self,
        target: &FrozenProcessIdentity,
        cpus: &[u32],
    ) -> Result<(), ProviderFailure>;
}

/// Resource-group mutation is independent from signal/batch and affinity
/// control: it has its own target object, authorization policy, and rollback
/// surface. Native adapters translate the shared request to their job/cgroup
/// primitive at this boundary.
pub trait ProcessResourceControlProvider: Send + 'static {
    fn apply_limits(
        &mut self,
        target: &FrozenProcessIdentity,
        limits: &ResourceGroupLimitRequest,
    ) -> Result<(), ProviderFailure>;
}

/// System-level (no target) per-feature escalation for per-process byte
/// accounting: the OS-native prompt is offered (pkexec/polkit on Linux), the
/// granted capture fd is consumed, and the accounting backend restarts with
/// real `CAP_NET_RAW` capture. Adapters without an escalation path return a
/// typed failure (e.g. `Unsupported` off-Linux) — never a fabricated capture.
pub trait ProcessNetworkEscalationProvider: Send + 'static {
    fn request_capture_escalation(&mut self) -> Result<(), ProviderFailure>;
}

/// Mutations stay cohesive because they share frozen-identity validation,
/// signal semantics, and one serialized side-effect budget.
pub trait ProcessControlProvider: Send + 'static {
    fn end_task(&mut self, target: FrozenProcessIdentity) -> Result<(), ProviderFailure>;
    fn execute_batch(
        &mut self,
        intent: ProcessBatchIntent,
    ) -> Result<ProcessBatchResult, ProviderFailure>;
    fn send_signal(
        &mut self,
        target: &FrozenProcessIdentity,
        signal: ProcessSignal,
    ) -> Result<(), ProviderFailure>;
}
