//! macOS process-domain providers built exclusively on safe wrapper crates.
//!
//! List/resources use `sysinfo`; process control uses `sysinfo::Process::kill_with`
//! (signals: terminate/kill/stop/continue) plus `renice` through the bounded
//! command runner for priority. Per-process network/GPU/isolation and CPU
//! affinity have NO safe source on macOS — those providers complete with
//! typed unsupported outcomes and the gaps are recorded in
//! `adr/019-macos-telemetry-safety.md`. The registry-facing group structs
//! live in `process/composition.rs`.

mod bundle_identity;
mod composition;
mod pending;

pub use composition::{
    MacProcessControlProviders, MacProcessObservationProviders, MacProcessProviders,
};
pub use pending::{
    PendingProcessAffinityControlProvider, PendingProcessAffinityProvider,
    PendingProcessGpuProvider, PendingProcessIsolationProvider,
    PendingProcessNetworkEscalationProvider, PendingProcessNetworkProvider,
    PendingProcessOpenFilesProvider, PendingProcessResourceControlProvider,
    PendingProcessThreadsProvider,
};

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{
    CumulativeCounter, FailureKind, FrozenProcessIdentity, ProcessBatchIntent, ProcessBatchResult,
    ProcessBatchTargetResult, ProcessInsightSnapshot, ProcessItem, ProcessMetadataFailure,
    ProcessMetadataObservation, ProcessMetadataObservations, ProcessOwner, ProcessOwnerIdentity,
    ProcessResourceSnapshot, ProcessScalarObservations, ProcessSignal, ProviderId,
    ScalarObservation,
};
use taskmanager_platform_contract::{PartialSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    ProcessControlProvider, ProcessListProvider, ProcessResourcesProvider,
};

use crate::provider::process_facts::ProcessFactsCache;

const PROCESS_LIST_PROVIDER: ProviderId = ProviderId::borrowed("macos.process.list.sysinfo");

fn source(provider: ProviderId, item_count: usize) -> SourceStatus {
    SourceStatus {
        provider,
        outcome: SourceOutcome::Available,
        item_count,
    }
}

/// Process list from `sysinfo`: PID, parent, name, command line, CPU/memory,
/// start time, per-process disk totals and executable path are all real.
/// Per-process open-file count is real via `sysinfo::Process::open_files`
/// (which calls `proc_pidinfo(PROC_PIDLISTFDS)` inside sysinfo's audited
/// boundary). Per-process owner is real: the POSIX uid comes from
/// `sysinfo::Process::user_id` and the friendly username is resolved through
/// the `sysinfo::Users` table, both degraded honestly to Unsupported/Absent
/// when sysinfo returns None. Per-process thread count and POSIX nice value
/// have no safe sysinfo accessor on macOS; both are filled by ONE bounded
/// the `ps -Ao pid,nice,thcount` shell-out cached ~5 s (see `ProcessFactsCache`),
/// and degrade honestly to typed Unsupported on a cache miss (ADR-019).
/// Application identity is derived from the executable path alone (no
/// AppKit): a `.app/Contents/MacOS` bundle layout publishes an Available
/// identity, a confirmed non-bundle executable path publishes Absent, and a
/// missing executable path stays Unknown (see `process/bundle_identity.rs`).
pub struct MacProcessListProvider {
    system: sysinfo::System,
    /// PID-keyed disk baselines carry the process start time so PID reuse
    /// cannot turn an older process's cumulative counter into a fabricated
    /// rate for the replacement process.
    disk_rates: HashMap<u32, MacProcessDiskRateState>,
    /// PID -> (nice, threads) from the last `ps` snapshot, refreshed at most
    /// once per ~5 s. A PID absent from the cache keeps both scalars honestly
    /// `Unsupported` rather than fabricating 0.
    process_facts: ProcessFactsCache,
}

impl MacProcessListProvider {
    pub fn new() -> Self {
        Self {
            system: sysinfo::System::new(),
            disk_rates: HashMap::new(),
            process_facts: ProcessFactsCache::new(),
        }
    }
}

impl ProcessListProvider for MacProcessListProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<PartialSourceSnapshot<ProcessItem>, ProviderFailure> {
        self.system
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        // Resolve uid -> username labels once per refresh (sysinfo::Users is
        // independent of the process table and owns its own SMC/passwd read).
        let users = sysinfo::Users::new_with_refreshed_list();

        // Clone the cached PID -> (nice, threads) map out of the &self borrow
        // before the loop, exactly as the Windows adapter clones the priority
        // map — `fresh` borrows self mutably on a cache miss and the loop body
        // also borrows self.disk_rates / self.system.
        let facts = self.process_facts.fresh(Instant::now()).clone();

        let mut items = Vec::new();
        let mut current_pids = std::collections::HashSet::new();
        for (pid, process) in self.system.processes() {
            let pid_value = pid.as_u32();
            current_pids.insert(pid_value);
            let disk = process.disk_usage();
            let start_time_secs = process.start_time();
            let (read_rate, write_rate) = self.disk_rates.entry(pid_value).or_default().observe(
                start_time_secs,
                disk.total_read_bytes,
                disk.total_written_bytes,
                observed_at_ms,
            );
            let cmdline = if process.cmd().is_empty() {
                process.name().to_string_lossy().into_owned()
            } else {
                process
                    .cmd()
                    .iter()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let mut item =
                ProcessItem::new(pid_value, process.name().to_string_lossy().into_owned());
            item.parent_pid = process.parent().map(|parent| parent.as_u32());
            item.cmdline = cmdline;
            item.status = process_status_label(process.status()).to_string();
            // Per-process thread count and POSIX nice come from the cached
            // `ps -Ao pid,nice,thcount` snapshot (sysinfo 0.39 has no safe
            // accessor for either on macOS). A cache hit promotes the scalar
            // to Available; a miss / empty cache keeps it honestly
            // `Unsupported` — never a fabricated 0. Either field can be
            // independently present when `ps` surfaced only one column.
            let (nice_obs, threads_obs) = match facts.get(&pid_value) {
                Some((nice, threads)) => (
                    nice.map(|value| ScalarObservation::available(value, observed_at_ms))
                        .unwrap_or_else(|| {
                            ScalarObservation::unavailable(FailureKind::Unsupported)
                        }),
                    threads
                        .map(|value| ScalarObservation::available(value, observed_at_ms))
                        .unwrap_or_else(|| {
                            ScalarObservation::unavailable(FailureKind::Unsupported)
                        }),
                ),
                None => (
                    ScalarObservation::unavailable(FailureKind::Unsupported),
                    ScalarObservation::unavailable(FailureKind::Unsupported),
                ),
            };
            let observations = ProcessScalarObservations {
                // sysinfo exposes only a second-resolution wall-clock start
                // value on macOS. Keep it as display/history metadata below,
                // but do not promote it to the exact token that authorizes
                // target reads or destructive control.
                start_token: ScalarObservation::unavailable(FailureKind::Unsupported),
                cpu_percentage: ScalarObservation::available(process.cpu_usage(), observed_at_ms),
                memory_bytes: ScalarObservation::available(process.memory(), observed_at_ms),
                memory_pss_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
                swap_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
                disk_read_bytes_total: ScalarObservation::available(
                    disk.total_read_bytes,
                    observed_at_ms,
                ),
                disk_write_bytes_total: ScalarObservation::available(
                    disk.total_written_bytes,
                    observed_at_ms,
                ),
                disk_read_bytes_per_sec: read_rate,
                disk_write_bytes_per_sec: write_rate,
                threads: threads_obs,
                start_time_secs: ScalarObservation::available(start_time_secs, observed_at_ms),
                cpu_time_secs: ScalarObservation::available(
                    process.accumulated_cpu_time().saturating_div(100),
                    observed_at_ms,
                ),
                // sysinfo::Process::open_files() reads `proc_pidinfo(
                // PROC_PIDLISTFDS)` inside sysinfo's audited boundary — the
                // real per-process fd count; None degrades to Unsupported.
                fds: process.open_files().map_or_else(
                    || ScalarObservation::unavailable(FailureKind::Unsupported),
                    |count| ScalarObservation::available(count as u32, observed_at_ms),
                ),
                nice: nice_obs,
            };
            // Typed owner + executable-path truth. sysinfo resolves the real
            // POSIX uid (proc_pidinfo) and the username label via the users
            // table; both degrade honestly to Unsupported/Absent when sysinfo
            // returns None rather than fabricating an empty owner.
            let owner = match process.user_id() {
                Some(uid) => {
                    let label = users
                        .get_user_by_id(uid)
                        .map(|user| user.name().to_string());
                    #[cfg(unix)]
                    let identity = ProcessOwnerIdentity::Numeric(u64::from(**uid));
                    #[cfg(not(unix))]
                    let identity = ProcessOwnerIdentity::Opaque(uid.to_string());
                    ProcessMetadataObservation::available(
                        ProcessOwner { identity, label },
                        observed_at_ms,
                    )
                }
                None => {
                    ProcessMetadataObservation::unavailable(ProcessMetadataFailure::Unsupported)
                }
            };
            let executable_path = match process.exe() {
                Some(path) => {
                    ProcessMetadataObservation::available(PathBuf::from(path), observed_at_ms)
                }
                None => ProcessMetadataObservation::absent(observed_at_ms),
            };
            // Bundle-layout classification is a pure path function, so the
            // whole three-state rule (available / absent / unknown) is shared
            // with the cross-platform unit tests in `bundle_identity`.
            item.apply_application_identity(bundle_identity::application_identity_observation(
                process.exe(),
                observed_at_ms,
            ));
            item.apply_metadata_observations(ProcessMetadataObservations {
                owner,
                executable_path,
            });
            item.apply_scalar_observations(observations);
            items.push(item);
        }
        self.disk_rates.retain(|pid, _| current_pids.contains(pid));

        if items.is_empty() {
            return Err(ProviderFailure::TemporarilyUnavailable);
        }
        let item_count = items.len();
        Ok(PartialSourceSnapshot::new(
            items,
            vec![source(PROCESS_LIST_PROVIDER, item_count)],
        ))
    }
}

#[derive(Default)]
struct MacProcessDiskRateState {
    start_time_secs: Option<u64>,
    read: CumulativeCounter,
    write: CumulativeCounter,
}

impl MacProcessDiskRateState {
    fn observe(
        &mut self,
        start_time_secs: u64,
        read_total: u64,
        write_total: u64,
        observed_at_ms: u64,
    ) -> (ScalarObservation<u64>, ScalarObservation<u64>) {
        let identity_changed = self
            .start_time_secs
            .is_some_and(|previous| previous != start_time_secs);
        if identity_changed {
            self.read.reset();
            self.write.reset();
        }
        self.start_time_secs = Some(start_time_secs);
        let initial_gap = if identity_changed {
            FailureKind::IdentityChanged
        } else {
            FailureKind::TemporarilyUnavailable
        };
        (
            self.read
                .observe(Ok(read_total), observed_at_ms, initial_gap)
                .per_second(observed_at_ms),
            self.write
                .observe(Ok(write_total), observed_at_ms, initial_gap)
                .per_second(observed_at_ms),
        )
    }
}

fn process_status_label(status: sysinfo::ProcessStatus) -> &'static str {
    match status {
        sysinfo::ProcessStatus::Run => "Running",
        sysinfo::ProcessStatus::Sleep => "Sleeping",
        sysinfo::ProcessStatus::Stop => "Stopped",
        sysinfo::ProcessStatus::Zombie => "Zombie",
        _ => "Other",
    }
}

/// macOS process-list start times are second-resolution compatibility facts,
/// not an exact provider-issued creation token. Until a safe precise
/// `proc_pidinfo` boundary exists, target-scoped resources fail closed rather
/// than reading whichever process currently owns the PID.
pub struct MacProcessResourcesProvider;

impl MacProcessResourcesProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ProcessResourcesProvider for MacProcessResourcesProvider {
    fn observe(
        &mut self,
        _target: &FrozenProcessIdentity,
        _observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessResourceSnapshot>, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

/// Destructive control is intentionally unavailable until macOS has a safe
/// native boundary that validates a precise creation token on the same owned
/// process handle. A second-resolution sysinfo timestamp cannot authorize a
/// signal or `renice` operation.
pub struct MacProcessControlProvider;

impl MacProcessControlProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ProcessControlProvider for MacProcessControlProvider {
    fn end_task(&mut self, target: FrozenProcessIdentity) -> Result<(), ProviderFailure> {
        let _ = target;
        Err(ProviderFailure::Unsupported)
    }

    fn execute_batch(
        &mut self,
        intent: ProcessBatchIntent,
    ) -> Result<ProcessBatchResult, ProviderFailure> {
        let targets = intent.targets.clone();
        let results = targets
            .into_iter()
            .map(|target| {
                (
                    target,
                    ProcessBatchTargetResult::Failed(FailureKind::Unsupported),
                )
            })
            .collect();
        let batch = ProcessBatchResult {
            intent,
            targets: results,
        };
        Ok(batch)
    }

    fn send_signal(
        &mut self,
        target: &FrozenProcessIdentity,
        signal: ProcessSignal,
    ) -> Result<(), ProviderFailure> {
        let _ = (target, signal);
        Err(ProviderFailure::Unsupported)
    }
}

#[cfg(test)]
#[path = "../../tests/headless/macos_provider_process.rs"]
mod tests;
