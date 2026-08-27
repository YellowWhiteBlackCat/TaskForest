//! Physically independent Linux collectors for process-insight domains.
//!
//! Each collector owns only its domain's slow state and reads the same procfs
//! start-time token immediately before and after collection. A PID reuse that
//! occurs while vendor GPU APIs, cgroupfs, or procfs are blocked therefore
//! rejects that facet instead of publishing mixed-generation data.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use taskmanager_core::{
    DeviceState, ProcessEnvironment, ProcessGpuSnapshot, ProcessIdentity, ProcessInsightSnapshot,
    ProcessIsolation, ProcessNetworkSnapshot, ProcessOpenFiles, ProcessResourceSnapshot,
    ProcessThreads,
};
use taskmanager_platform_contract::ProviderFailure;

use super::super::procfs::clock_ticks_per_second;
use super::{
    ProcessGpuRateTracker, ProcessNetworkAccountingBackend, ProcessNetworkRateTracker, environment,
    gpu, gpu_engines, isolation, network, open_files, parse_start_time_ticks, resources, threads,
};

/// Byte-accounting backend shared between the network observation provider
/// and the escalation provider: the escalation provider swaps in a backend
/// started from an escalated capture fd; the observer reads through the same
/// handle, so the upgrade is visible to the very next observation. The mutex
/// is held only for one `read_counters` call per tick (insight refresh
/// cadence), never across the worker loop.
pub(crate) type SharedAccountingBackend = Arc<Mutex<Box<dyn ProcessNetworkAccountingBackend>>>;

pub(crate) struct ProcessNetworkCollector {
    states: HashMap<ProcessIdentity, DeviceState>,
    rates: ProcessNetworkRateTracker,
    accounting: SharedAccountingBackend,
}

impl Default for ProcessNetworkCollector {
    fn default() -> Self {
        Self {
            states: HashMap::new(),
            rates: ProcessNetworkRateTracker::default(),
            accounting: Arc::new(Mutex::new(Box::new(
                network::UnsupportedNetworkAccountingBackend,
            ))),
        }
    }
}

impl ProcessNetworkCollector {
    /// Build the collector over a pre-shared accounting handle (the live
    /// registry shares one backend between the observation and the escalation
    /// provider so a granted prompt upgrades capture in place).
    pub(crate) fn with_shared_accounting(accounting: SharedAccountingBackend) -> Self {
        Self {
            states: HashMap::new(),
            rates: ProcessNetworkRateTracker::default(),
            accounting,
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn collect(
        &mut self,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessNetworkSnapshot>, ProviderFailure> {
        self.collect_from_root(Path::new("/proc"), pid, now_ms)
    }

    /// Prune per-identity state for pids absent from the authoritative live
    /// pid set. The per-observe retains only reset a pid's own generation on
    /// reuse; exited pids the user once inspected would otherwise stay forever.
    /// Driven by the provider layer on the process-list tick that revalidates
    /// the target, so every live pid (including other open insights) stays.
    pub(crate) fn retain_live_pids(&mut self, live_pids: &HashSet<u32>) {
        self.states
            .retain(|known, _| live_pids.contains(&known.pid));
        self.rates.retain_live_pids(live_pids);
    }

    #[cfg(target_os = "linux")]
    fn collect_from_root(
        &mut self,
        proc_root: &Path,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessNetworkSnapshot>, ProviderFailure> {
        let identity = read_process_identity(proc_root, pid)?;
        let proc_dir = proc_root.join(pid.to_string());
        let mut value = network::collect_from_proc_dir(&proc_dir, now_ms);
        self.states
            .retain(|known, _| known.pid != identity.pid || *known == identity);
        value.state = self
            .states
            .get(&identity)
            .copied()
            .unwrap_or_default()
            .transition(value.state.status, now_ms);
        self.states.insert(identity, value.state);
        let mut accounting = self
            .accounting
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let provider = accounting.provider();
        let counters = accounting.read_counters(identity, now_ms);
        drop(accounting);
        self.rates
            .observe(identity, now_ms, provider, counters, &mut value);
        validate_post_collection_identity(proc_root, identity)?;
        Ok(ProcessInsightSnapshot { identity, value })
    }
}

pub(crate) struct ProcessGpuCollector {
    rates: ProcessGpuRateTracker,
    engine_rates: gpu_engines::ProcessGpuEngineRateTracker,
    enrichment: Box<dyn gpu::ProcessGpuEnrichmentProvider>,
}

impl Default for ProcessGpuCollector {
    fn default() -> Self {
        Self {
            rates: ProcessGpuRateTracker::default(),
            engine_rates: gpu_engines::ProcessGpuEngineRateTracker::default(),
            enrichment: gpu::standard_process_gpu_enrichment(),
        }
    }
}

impl ProcessGpuCollector {
    #[cfg(target_os = "linux")]
    pub(crate) fn collect(
        &mut self,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessGpuSnapshot>, ProviderFailure> {
        self.collect_from_root(Path::new("/proc"), pid, now_ms)
    }

    /// Prune per-identity baselines for pids absent from the authoritative
    /// live pid set — see [`ProcessNetworkCollector::retain_live_pids`].
    pub(crate) fn retain_live_pids(&mut self, live_pids: &HashSet<u32>) {
        self.rates.retain_live_pids(live_pids);
        self.engine_rates.retain_live_pids(live_pids);
    }

    #[cfg(target_os = "linux")]
    fn collect_from_root(
        &mut self,
        proc_root: &Path,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessGpuSnapshot>, ProviderFailure> {
        let identity = read_process_identity(proc_root, pid)?;
        let proc_dir = proc_root.join(pid.to_string());
        let raw = gpu::merge_gpu_enrichment(
            gpu::collect_counters_from_proc_dir(&proc_dir, now_ms),
            self.enrichment.collect(proc_root, identity, now_ms),
        );
        let mut value = self.rates.observe(identity, now_ms, raw);
        // Per-engine breakdown is collected through a separate procfs tree
        // (`fd/` readlink → `/dev/dri/` → `fdinfo/<fd>`) with its own baseline
        // state, so it carries an independent collection health.
        let engine_raw = gpu_engines::collect_gpu_engines_from_proc_dir(&proc_dir, now_ms);
        value.engines = self.engine_rates.observe(identity, now_ms, engine_raw);
        validate_post_collection_identity(proc_root, identity)?;
        Ok(ProcessInsightSnapshot { identity, value })
    }
}

#[derive(Debug, Default)]
pub(crate) struct ProcessResourcesCollector {
    tracker: resources::ProcessResourceTracker,
}

impl ProcessResourcesCollector {
    #[cfg(target_os = "linux")]
    pub(crate) fn collect(
        &mut self,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessResourceSnapshot>, ProviderFailure> {
        self.collect_from_roots(Path::new("/proc"), Path::new("/sys/fs/cgroup"), pid, now_ms)
    }

    #[cfg(target_os = "linux")]
    fn collect_from_roots(
        &mut self,
        proc_root: &Path,
        cgroup_root: &Path,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessResourceSnapshot>, ProviderFailure> {
        let identity = read_process_identity(proc_root, pid)?;
        let value = self.tracker.collect(
            &proc_root.join(pid.to_string()),
            cgroup_root,
            identity,
            now_ms,
        );
        validate_post_collection_identity(proc_root, identity)?;
        Ok(ProcessInsightSnapshot { identity, value })
    }
}

#[derive(Debug, Default)]
pub(crate) struct ProcessIsolationCollector;

impl ProcessIsolationCollector {
    #[cfg(target_os = "linux")]
    pub(crate) fn collect(
        &mut self,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessIsolation>, ProviderFailure> {
        self.collect_from_root(Path::new("/proc"), pid, now_ms)
    }

    #[cfg(target_os = "linux")]
    fn collect_from_root(
        &mut self,
        proc_root: &Path,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessIsolation>, ProviderFailure> {
        let identity = read_process_identity(proc_root, pid)?;
        let value =
            isolation::collect_independent_from_proc_dir(&proc_root.join(pid.to_string()), now_ms);
        validate_post_collection_identity(proc_root, identity)?;
        Ok(ProcessInsightSnapshot { identity, value })
    }
}

/// Stateless collector for the open-files facet. Mirrors the other collectors:
/// the process start-time token is read immediately before and after
/// collection so a PID reuse that occurs while `read_dir`/`readlink` are
/// blocked rejects the facet instead of publishing mixed-generation data.
#[derive(Debug, Default)]
pub struct ProcessOpenFilesCollector;

impl ProcessOpenFilesCollector {
    #[cfg(target_os = "linux")]
    pub fn collect(
        &mut self,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessOpenFiles>, ProviderFailure> {
        self.collect_from_root(Path::new("/proc"), pid, now_ms)
    }

    #[cfg(target_os = "linux")]
    pub fn collect_from_root(
        &mut self,
        proc_root: &Path,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessOpenFiles>, ProviderFailure> {
        let identity = read_process_identity(proc_root, pid)?;
        let value =
            open_files::collect_open_files_from_proc_dir(&proc_root.join(pid.to_string()), now_ms);
        validate_post_collection_identity(proc_root, identity)?;
        Ok(ProcessInsightSnapshot { identity, value })
    }
}

/// Stateless collector for the environment facet (working directory +
/// bounded environment table). Identity is pinned before and after the
/// bounded read, mirroring the open-files collector.
#[derive(Debug, Default)]
pub struct ProcessEnvironmentCollector;

impl ProcessEnvironmentCollector {
    #[cfg(target_os = "linux")]
    pub fn collect(
        &mut self,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessEnvironment>, ProviderFailure> {
        self.collect_from_root(Path::new("/proc"), pid, now_ms)
    }

    #[cfg(target_os = "linux")]
    pub fn collect_from_root(
        &mut self,
        proc_root: &Path,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessEnvironment>, ProviderFailure> {
        let identity = read_process_identity(proc_root, pid)?;
        let value = environment::collect_environment_from_proc_dir(
            &proc_root.join(pid.to_string()),
            now_ms,
        );
        validate_post_collection_identity(proc_root, identity)?;
        Ok(ProcessInsightSnapshot { identity, value })
    }
}

/// Stateful collector for the per-thread facet. It keeps only bounded,
/// identity-keyed counter baselines so consecutive observations can expose a
/// real CPU% without allowing a reused PID or TID to inherit old history.
#[derive(Debug, Default)]
pub struct ProcessThreadsCollector {
    rates: threads::ThreadCpuRateTracker,
}

impl ProcessThreadsCollector {
    #[cfg(target_os = "linux")]
    pub fn collect(
        &mut self,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessThreads>, ProviderFailure> {
        self.collect_from_root(Path::new("/proc"), pid, now_ms)
    }

    #[cfg(target_os = "linux")]
    pub fn collect_from_root(
        &mut self,
        proc_root: &Path,
        pid: u32,
        now_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessThreads>, ProviderFailure> {
        let identity = read_process_identity(proc_root, pid)?;
        let clock_ticks = clock_ticks_per_second();
        let value = threads::collect_threads_with_cpu_rate(
            &proc_root.join(pid.to_string()),
            identity,
            now_ms,
            &clock_ticks,
            &mut self.rates,
        );
        validate_post_collection_identity(proc_root, identity)?;
        Ok(ProcessInsightSnapshot { identity, value })
    }
}

#[cfg(target_os = "linux")]
fn read_process_identity(proc_root: &Path, pid: u32) -> Result<ProcessIdentity, ProviderFailure> {
    let path: PathBuf = proc_root.join(pid.to_string()).join("stat");
    let text = std::fs::read_to_string(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => ProviderFailure::IdentityChanged,
        std::io::ErrorKind::PermissionDenied => ProviderFailure::PermissionDenied,
        std::io::ErrorKind::TimedOut
        | std::io::ErrorKind::Interrupted
        | std::io::ErrorKind::WouldBlock => ProviderFailure::TemporarilyUnavailable,
        _ => ProviderFailure::ProviderFault,
    })?;
    let start_token = parse_start_time_ticks(&text).ok_or(ProviderFailure::ProviderFault)?;
    Ok(ProcessIdentity { pid, start_token })
}

#[cfg(target_os = "linux")]
fn validate_post_collection_identity(
    proc_root: &Path,
    expected: ProcessIdentity,
) -> Result<(), ProviderFailure> {
    let observed = read_process_identity(proc_root, expected.pid)?;
    if observed == expected {
        Ok(())
    } else {
        Err(ProviderFailure::IdentityChanged)
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_process_telemetry_facets_tests.rs"]
mod tests;
