//! Linux process observation and control providers.

use std::path::PathBuf;

use taskmanager_core::{
    FrozenProcessIdentity, ProcessBatchIntent, ProcessBatchResult, ProcessEnvironment,
    ProcessGpuSnapshot, ProcessInsightSnapshot, ProcessIsolation, ProcessItem,
    ProcessNetworkSnapshot, ProcessOpenFiles, ProcessResourceSnapshot, ProcessSignal,
    ProcessThreads, ResourceGroupLimitRequest,
};
use taskmanager_escalation::polkit::NetLauncherProcess;
use taskmanager_platform_contract::{FailureKind, PartialSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    ProcessAffinityControlProvider, ProcessAffinityProvider, ProcessControlProvider,
    ProcessEnvironmentProvider, ProcessGpuProvider, ProcessIsolationProvider, ProcessListProvider,
    ProcessNetworkEscalationProvider, ProcessNetworkProvider, ProcessOpenFilesProvider,
    ProcessResourceControlProvider, ProcessResourcesProvider, ProcessThreadsProvider,
};

use crate::engine::process::batch::execute_process_batch;
use crate::engine::process::telemetry::{
    CgroupCpuLimit, CgroupLimitRequest, ProcessEnvironmentCollector, ProcessGpuCollector,
    ProcessIsolationCollector, ProcessNetworkCollector, ProcessOpenFilesCollector,
    ProcessResourcesCollector, ProcessThreadsCollector, SharedAccountingBackend,
};
// Cgroup-v2 write helpers are plain cgroupfs I/O and ship in release builds;
// they are only reachable on Linux.
#[cfg(target_os = "linux")]
use crate::engine::process::telemetry::{
    CgroupIoError, CgroupLimitApplyError, CgroupLimitConfirmation, CgroupLimitPlanError,
    apply_cgroup_limit_plan, authorize_cgroup_limit_plan, parse_proc_cgroup, plan_cgroup_limits,
};
use crate::engine::process::{
    ProcessManager, affinity_operation, finish_with_escalation, signal_operation,
    validate_exact_start_token,
};
#[cfg(target_os = "linux")]
use taskmanager_core::ProcessIdentity;

use super::process_target::validate_process_identity;

pub(super) struct ProcfsProcessListProvider {
    pub(super) process_manager: ProcessManager,
}

impl ProcessListProvider for ProcfsProcessListProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<PartialSourceSnapshot<ProcessItem>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            Ok(self.process_manager.refresh_at(observed_at_ms))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = observed_at_ms;
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

pub(super) struct NativeProcessNetworkProvider {
    pub(super) collector: ProcessNetworkCollector,
    pub(super) process_manager: ProcessManager,
}

/// System-level per-feature escalation for per-process byte accounting: offers
/// the OS-native prompt through the injected launcher seam, consumes the
/// granted capture fd, and swaps the shared accounting backend so the very
/// next observation reports real rates (ADR-023/024/025). Constructed with the
/// same shared backend handle as [`NativeProcessNetworkProvider`]'s collector.
pub(super) struct NativeProcessNetworkEscalationProvider {
    accounting: SharedAccountingBackend,
    proc_root: PathBuf,
    iface_index: u32,
    launcher: Box<dyn NetLauncherProcess + Send>,
}

impl NativeProcessNetworkEscalationProvider {
    #[must_use]
    pub(super) fn new(
        accounting: SharedAccountingBackend,
        proc_root: PathBuf,
        iface_index: u32,
        launcher: Box<dyn NetLauncherProcess + Send>,
    ) -> Self {
        Self {
            accounting,
            proc_root,
            iface_index,
            launcher,
        }
    }
}

impl ProcessNetworkEscalationProvider for NativeProcessNetworkEscalationProvider {
    fn request_capture_escalation(&mut self) -> Result<(), ProviderFailure> {
        // The pkexec + AF_PACKET + SCM_RIGHTS chain (ADR-023/024/025) exists
        // only on Linux; the non-Linux compile of this adapter answers with the
        // permanent typed Unsupported outcome instead of a fabricated grant.
        #[cfg(target_os = "linux")]
        {
            use crate::engine::process::telemetry::net_accounting::AfPacketAccountingBackend;
            // The launcher seam is object-safe only at the trait level, so drive
            // it directly instead of the Sized-bound `invoke_net_launcher_with`
            // helper; the outcome mapping below mirrors that helper exactly.
            let fd = match self.launcher.obtain_fd(self.iface_index) {
                Ok(fd) => fd,
                Err(error) => {
                    return Err(ProviderFailure::from_kind(
                        if error.kind() == std::io::ErrorKind::PermissionDenied {
                            FailureKind::PermissionDenied
                        } else {
                            FailureKind::TemporarilyUnavailable
                        },
                    ));
                }
            };
            // Swap in a backend started from the escalated fd: the capture worker
            // restarts over the SCM_RIGHTS fd and the next observation reports real
            // rates. The mutex is held only across this swap, never the worker.
            let mut accounting = self
                .accounting
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *accounting = Box::new(AfPacketAccountingBackend::start_from_source(
                taskmanager_afpacket::PacketSource::from_owned_fd(fd),
                &self.proc_root,
            ));
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(ProviderFailure::from_kind(FailureKind::Unsupported))
        }
    }
}

impl ProcessNetworkProvider for NativeProcessNetworkProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessNetworkSnapshot>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            validate_process_identity(&mut self.process_manager, target)?;
            validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
            // The validation refresh just rebuilt the authoritative live pid
            // set: drop per-identity state for pids that have exited so a
            // long-running session cannot accumulate stale baselines. Every
            // live pid (including other open insight targets) is retained.
            self.collector
                .retain_live_pids(&self.process_manager.live_pids());
            let snapshot = self.collector.collect(target.pid, observed_at_ms)?;
            validate_process_identity(&mut self.process_manager, target)?;
            Ok(snapshot)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, observed_at_ms);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

pub(super) struct NativeProcessGpuProvider {
    pub(super) collector: ProcessGpuCollector,
    pub(super) process_manager: ProcessManager,
}

impl ProcessGpuProvider for NativeProcessGpuProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessGpuSnapshot>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            validate_process_identity(&mut self.process_manager, target)?;
            validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
            // Same live-pid prune contract as the network provider above: the
            // validation refresh rebuilt the authoritative set, so exited
            // pids' rate baselines are dropped while live targets are kept.
            self.collector
                .retain_live_pids(&self.process_manager.live_pids());
            let snapshot = self.collector.collect(target.pid, observed_at_ms)?;
            validate_process_identity(&mut self.process_manager, target)?;
            Ok(snapshot)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, observed_at_ms);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

pub(super) struct NativeProcessResourcesProvider {
    pub(super) collector: ProcessResourcesCollector,
    pub(super) process_manager: ProcessManager,
}

impl ProcessResourcesProvider for NativeProcessResourcesProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessResourceSnapshot>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            validate_process_identity(&mut self.process_manager, target)?;
            validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
            let snapshot = self.collector.collect(target.pid, observed_at_ms)?;
            validate_process_identity(&mut self.process_manager, target)?;
            Ok(snapshot)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, observed_at_ms);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

pub(super) struct NativeProcessIsolationProvider {
    pub(super) collector: ProcessIsolationCollector,
    pub(super) process_manager: ProcessManager,
}

pub(super) struct NativeProcessThreadsProvider {
    pub(super) collector: ProcessThreadsCollector,
    pub(super) process_manager: ProcessManager,
}

impl ProcessThreadsProvider for NativeProcessThreadsProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessThreads>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            validate_process_identity(&mut self.process_manager, target)?;
            validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
            let snapshot = self.collector.collect(target.pid, observed_at_ms)?;
            validate_process_identity(&mut self.process_manager, target)?;
            Ok(snapshot)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, observed_at_ms);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

pub(super) struct NativeProcessOpenFilesProvider {
    pub(super) collector: ProcessOpenFilesCollector,
    pub(super) process_manager: ProcessManager,
}

impl ProcessOpenFilesProvider for NativeProcessOpenFilesProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessOpenFiles>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            validate_process_identity(&mut self.process_manager, target)?;
            validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
            let snapshot = self.collector.collect(target.pid, observed_at_ms)?;
            validate_process_identity(&mut self.process_manager, target)?;
            Ok(snapshot)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, observed_at_ms);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

pub(super) struct NativeProcessEnvironmentProvider {
    pub(super) collector: ProcessEnvironmentCollector,
    pub(super) process_manager: ProcessManager,
}

impl ProcessEnvironmentProvider for NativeProcessEnvironmentProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessEnvironment>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            validate_process_identity(&mut self.process_manager, target)?;
            validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
            let snapshot = self.collector.collect(target.pid, observed_at_ms)?;
            validate_process_identity(&mut self.process_manager, target)?;
            Ok(snapshot)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, observed_at_ms);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

impl ProcessIsolationProvider for NativeProcessIsolationProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessIsolation>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            validate_process_identity(&mut self.process_manager, target)?;
            validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
            let snapshot = self.collector.collect(target.pid, observed_at_ms)?;
            validate_process_identity(&mut self.process_manager, target)?;
            Ok(snapshot)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, observed_at_ms);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

pub(super) struct NativeProcessAffinityProvider;

impl ProcessAffinityProvider for NativeProcessAffinityProvider {
    fn affinity(&mut self, target: &FrozenProcessIdentity) -> Result<Vec<u32>, ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            let mut manager = ProcessManager::new();
            validate_process_identity(&mut manager, target)?;
            validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
            let cpus = ProcessManager::get_process_affinity(target.pid)
                .map_err(ProviderFailure::from_kind)?;
            if cpus.is_empty() {
                Err(ProviderFailure::TemporarilyUnavailable)
            } else {
                Ok(cpus)
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = target;
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

pub(super) struct NativeProcessControlProvider {
    pub(super) process_manager: ProcessManager,
}

pub(super) struct NativeProcessAffinityControlProvider {
    pub(super) process_manager: ProcessManager,
}

impl ProcessAffinityControlProvider for NativeProcessAffinityControlProvider {
    fn set_affinity(
        &mut self,
        target: &FrozenProcessIdentity,
        cpus: &[u32],
    ) -> Result<(), ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            validate_process_identity(&mut self.process_manager, target)?;
            validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
            let direct = ProcessManager::set_process_affinity(target.pid, cpus);
            finish_with_escalation(target, affinity_operation(cpus), direct)
                .map_err(ProviderFailure::from_kind)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, cpus);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

/// Native cgroup-v2 resource-limit control.
///
/// Independent from [`NativeProcessControlProvider`] (signal/batch) and
/// [`NativeProcessAffinityControlProvider`] (scheduler affinity): cgroup writes
/// target a different kernel object (the unified cgroup), need the target's
/// cgroup membership, and run their own identity-revalidated transaction with
/// rollback. Mirroring affinity, the frozen target is revalidated immediately
/// before the write.
///
/// The plan/authorize/apply pipeline is plain cgroupfs I/O with no test-only
/// types, so it is compiled in release builds (previously it was gated behind
/// `cfg(test/test-support)` and the capability vanished from shipped artifacts).
pub(super) struct NativeProcessCgroupControlProvider {
    process_manager: ProcessManager,
}

impl Default for NativeProcessCgroupControlProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeProcessCgroupControlProvider {
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            process_manager: ProcessManager::new(),
        }
    }

    /// Apply cgroup-v2 resource limits to the target's unified cgroup.
    ///
    /// Authorization mirrors [`NativeProcessAffinityControlProvider::set_affinity`]
    /// (frozen-identity revalidation immediately before the kernel write) and
    /// then runs plan -> authorize -> apply with `allow_write: true` and a
    /// confirmation identity matched to the plan, so the cgroup pipeline's own
    /// double identity-revalidation and rollback logic is reused unchanged.
    fn apply_cgroup_limits(
        &mut self,
        target: &FrozenProcessIdentity,
        request: CgroupLimitRequest,
    ) -> Result<(), ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            validate_process_identity(&mut self.process_manager, target)?;
            validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
            let start_token = target
                .authoritative_start_token()
                .ok_or(ProviderFailure::IdentityChanged)?;
            let identity = ProcessIdentity {
                pid: target.pid,
                start_token,
            };
            // The plan is a pure function over the target's current cgroup
            // membership, read fresh here so a migrated process cannot inherit
            // authority from a stale membership.
            let membership_text = std::fs::read_to_string(format!("/proc/{}/cgroup", target.pid))
                .map_err(cgroup_membership_read_failure)?;
            let memberships = parse_proc_cgroup(&membership_text);
            let plan =
                plan_cgroup_limits(identity, &memberships, request).map_err(cgroup_plan_failure)?;
            let authorized = authorize_cgroup_limit_plan(
                plan,
                CgroupLimitConfirmation {
                    identity,
                    allow_write: true,
                },
            )
            .map_err(cgroup_plan_failure)?;
            apply_cgroup_limit_plan(&authorized).map_err(cgroup_apply_failure)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, request);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

impl ProcessResourceControlProvider for NativeProcessCgroupControlProvider {
    fn apply_limits(
        &mut self,
        target: &FrozenProcessIdentity,
        limits: &ResourceGroupLimitRequest,
    ) -> Result<(), ProviderFailure> {
        let request = CgroupLimitRequest {
            memory_max: limits.memory,
            cpu_max: limits.cpu.map(|cpu| CgroupCpuLimit {
                quota_us: cpu.quota,
                period_us: cpu.period_micros,
            }),
            pids_max: limits.processes,
        };
        self.apply_cgroup_limits(target, request)
    }
}

#[cfg(target_os = "linux")]
fn cgroup_plan_failure(error: CgroupLimitPlanError) -> ProviderFailure {
    match error {
        CgroupLimitPlanError::MissingIdentity => ProviderFailure::IdentityChanged,
        CgroupLimitPlanError::MissingUnifiedCgroup | CgroupLimitPlanError::EmptyRequest => {
            ProviderFailure::Unsupported
        }
        CgroupLimitPlanError::InvalidCpuPeriod
        | CgroupLimitPlanError::NotConfirmed
        | CgroupLimitPlanError::ConfirmationIdentityMismatch => ProviderFailure::Rejected,
    }
}

#[cfg(target_os = "linux")]
fn cgroup_apply_failure(error: CgroupLimitApplyError) -> ProviderFailure {
    match error {
        CgroupLimitApplyError::Unsupported => ProviderFailure::Unsupported,
        CgroupLimitApplyError::IdentityReadFailed(failure)
        | CgroupLimitApplyError::MembershipReadFailed(failure) => cgroup_io_failure(failure),
        CgroupLimitApplyError::IdentityChanged { .. }
        | CgroupLimitApplyError::TargetChanged { .. } => ProviderFailure::IdentityChanged,
        CgroupLimitApplyError::ReadFailed { failure, .. }
        | CgroupLimitApplyError::WriteFailed { failure, .. } => cgroup_io_failure(failure),
    }
}

#[cfg(target_os = "linux")]
fn cgroup_io_failure(failure: CgroupIoError) -> ProviderFailure {
    match failure {
        CgroupIoError::NotFound => ProviderFailure::IdentityChanged,
        CgroupIoError::PermissionDenied => ProviderFailure::PermissionDenied,
        CgroupIoError::Unavailable => ProviderFailure::TemporarilyUnavailable,
    }
}

#[cfg(target_os = "linux")]
fn cgroup_membership_read_failure(error: std::io::Error) -> ProviderFailure {
    match error.kind() {
        std::io::ErrorKind::NotFound => ProviderFailure::IdentityChanged,
        std::io::ErrorKind::PermissionDenied => ProviderFailure::PermissionDenied,
        _ => ProviderFailure::TemporarilyUnavailable,
    }
}

impl ProcessControlProvider for NativeProcessControlProvider {
    fn end_task(&mut self, target: FrozenProcessIdentity) -> Result<(), ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            validate_process_identity(&mut self.process_manager, &target)?;
            validate_exact_start_token(&target).map_err(ProviderFailure::from_kind)?;
            let direct = ProcessManager::terminate_process(target.pid);
            finish_with_escalation(
                &target,
                taskmanager_escalation::polkit::ForeignProcessControlOperation::End,
                direct,
            )
            .map_err(ProviderFailure::from_kind)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = target;
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }

    fn execute_batch(
        &mut self,
        intent: ProcessBatchIntent,
    ) -> Result<ProcessBatchResult, ProviderFailure> {
        Ok(execute_process_batch(intent))
    }

    fn send_signal(
        &mut self,
        target: &FrozenProcessIdentity,
        signal: ProcessSignal,
    ) -> Result<(), ProviderFailure> {
        #[cfg(target_os = "linux")]
        {
            validate_process_identity(&mut self.process_manager, target)?;
            validate_exact_start_token(target).map_err(ProviderFailure::from_kind)?;
            let direct = ProcessManager::send_signal(target.pid, native_signal(signal));
            finish_with_escalation(target, signal_operation(signal), direct)
                .map_err(ProviderFailure::from_kind)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, signal);
            Err(ProviderFailure::TemporarilyUnavailable)
        }
    }
}

#[cfg(target_os = "linux")]
const fn native_signal(signal: ProcessSignal) -> nix::sys::signal::Signal {
    use nix::sys::signal::Signal;
    match signal {
        ProcessSignal::Terminate => Signal::SIGTERM,
        ProcessSignal::Kill => Signal::SIGKILL,
        ProcessSignal::Stop => Signal::SIGSTOP,
        ProcessSignal::Continue => Signal::SIGCONT,
        ProcessSignal::Hangup => Signal::SIGHUP,
        ProcessSignal::Interrupt => Signal::SIGINT,
        ProcessSignal::User1 => Signal::SIGUSR1,
        ProcessSignal::User2 => Signal::SIGUSR2,
    }
}
