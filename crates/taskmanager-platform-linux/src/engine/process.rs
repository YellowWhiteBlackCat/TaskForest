//! Linux process enumeration and control provider.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::Instant;

use crate::config::FD_COUNT_REFRESH_EVERY_N_TICKS;
use sysinfo::{ProcessStatus, System};
pub use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::{
    FailureKind, ProcessHistorySample, ProcessHistoryStore, ProcessMetadataFailure, ProviderId,
    SourceOutcome, SourceStatus,
};
use taskmanager_platform_contract::PartialSourceSnapshot;

mod application;
pub mod batch;
mod control;
mod foreign_control;
mod memory_maps;
mod metadata;
mod observation;
pub mod open;
mod procfs;
mod rates;
pub mod signal;
pub mod telemetry;
mod tree;

#[cfg(feature = "test-support")]
pub use batch::{ProcessBatchSubmitError, ProcessBatchWorker};
#[cfg(feature = "test-support")]
pub use open::parent_dir;
pub use open::{open_file_location, read_exe_path};
#[cfg(feature = "test-support")]
pub use procfs::{
    ProcIoFields, ProcStatFields, parse_proc_io, parse_proc_stat, parse_proc_status_memory,
};
#[cfg(feature = "test-support")]
pub use signal::{kill_process, pause_process, resume_process, terminate_process};

use application::{ApplicationCatalog, retain_for_same_identity};
pub(crate) use foreign_control::{
    affinity_operation, batch_operation, finish_with_escalation, signal_operation,
};
use memory_maps::{MemoryMaps, ProcessMemoryObservation};
use metadata::{PasswdLabels, load_passwd_labels, observe_process_metadata};
use observation::{mark_retained_item_stale, observe_clock_ticks, observe_process_scalars};
pub(crate) use procfs::validate_exact_start_token;
use rates::ProcessRateState;
use tree::{io_failure, read_boot_time_secs};

/// Owner-label map from `/etc/passwd`, cached across ticks.
///
/// `loaded_at_ms` is `None` until the first refresh loads it. A failed load
/// expires on a short retry interval instead of being cached as success, so a
/// transient error never becomes a permanent `Ok` and a permanent error keeps
/// retrying at bounded cost.
struct PasswdCache {
    loaded_at_ms: Option<u64>,
    labels: PasswdLabels,
}

impl Default for PasswdCache {
    fn default() -> Self {
        Self {
            loaded_at_ms: None,
            labels: Err(ProcessMetadataFailure::ProviderFault),
        }
    }
}

impl PasswdCache {
    fn labels(&mut self, observed_at_ms: u64) -> &PasswdLabels {
        self.labels_or_refresh(observed_at_ms, &mut load_passwd_labels)
    }

    fn labels_or_refresh(
        &mut self,
        observed_at_ms: u64,
        load: &mut impl FnMut() -> PasswdLabels,
    ) -> &PasswdLabels {
        if self.loaded_at_ms.is_none_or(|loaded_at| {
            observed_at_ms.saturating_sub(loaded_at) >= passwd_cache_interval(&self.labels)
        }) {
            self.labels = load();
            self.loaded_at_ms = Some(observed_at_ms);
        }
        &self.labels
    }
}

/// Boot epoch from `/proc/stat` `btime`, cached because it cannot change while
/// the system runs. Failures retry on a short interval like [`PasswdCache`].
struct BootTimeCache {
    loaded_at_ms: Option<u64>,
    value: Result<u64, FailureKind>,
}

impl Default for BootTimeCache {
    fn default() -> Self {
        Self {
            loaded_at_ms: None,
            value: Err(FailureKind::ProviderFault),
        }
    }
}

impl BootTimeCache {
    fn value(&mut self, observed_at_ms: u64) -> Result<u64, FailureKind> {
        self.value_or_refresh(observed_at_ms, &mut read_boot_time_secs)
    }

    fn value_or_refresh(
        &mut self,
        observed_at_ms: u64,
        load: &mut impl FnMut() -> Result<u64, FailureKind>,
    ) -> Result<u64, FailureKind> {
        if self.loaded_at_ms.is_none_or(|loaded_at| {
            observed_at_ms.saturating_sub(loaded_at) >= boot_time_cache_interval(&self.value)
        }) {
            self.value = load();
            self.loaded_at_ms = Some(observed_at_ms);
        }
        self.value
    }
}

/// Last tick's rows plus a pid → index lookup, rebuilt together by
/// [`Self::sync_from`] so the previous-tick lookup is O(1) while preserving
/// the `Vec::iter().find()` semantics (first occurrence wins) it replaced.
#[derive(Default)]
struct PreviousItems {
    items: Vec<ProcessItem>,
    by_pid: HashMap<u32, usize>,
}

impl PreviousItems {
    fn find(&self, pid: u32) -> Option<&ProcessItem> {
        self.by_pid
            .get(&pid)
            .and_then(|&index| self.items.get(index))
    }

    fn sync_from(&mut self, items: &[ProcessItem]) {
        self.items.clear();
        self.items.extend_from_slice(items);
        self.by_pid.clear();
        self.by_pid.reserve(self.items.len());
        for (index, item) in self.items.iter().enumerate() {
            self.by_pid.entry(item.pid).or_insert(index);
        }
    }
}

pub struct ProcessManager {
    system: System,
    histories: ProcessHistoryStore,
    history_started_at: Instant,
    rates: ProcessRateState,
    previous_items: PreviousItems,
    passwd_cache: PasswdCache,
    boot_time_cache: BootTimeCache,
    memory_maps: MemoryMaps,
    applications: ApplicationCatalog,
    /// Monotonic refresh counter used to defer the per-process `/proc/<pid>/fd`
    /// scan to every Nth tick (see `FD_COUNT_REFRESH_EVERY_N_TICKS`). fd count
    /// is a low-frequency-drift column; sampling it ~1×/s bounds syscall cost
    /// while intermediate ticks reuse the retained previous value.
    fd_tick: u32,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManager {
    /// The authoritative live pid set from the most recent successful
    /// enumeration. Insight collectors prune their per-identity baseline maps
    /// against this set (the per-target equivalent of the `retain_pids` pass
    /// below), so entries for exited processes cannot accumulate across an
    /// arbitrarily long session. Only refreshed by a successful tick: a failed
    /// enumeration keeps the previous set, which can retain dead entries one
    /// extra tick but never drops a live pid.
    pub(crate) fn live_pids(&self) -> HashSet<u32> {
        self.previous_items.by_pid.keys().copied().collect()
    }

    #[must_use]
    pub fn new() -> Self {
        Self {
            system: System::new(),
            histories: ProcessHistoryStore::default(),
            history_started_at: Instant::now(),
            rates: ProcessRateState::default(),
            previous_items: PreviousItems::default(),
            passwd_cache: PasswdCache::default(),
            boot_time_cache: BootTimeCache::default(),
            memory_maps: MemoryMaps::default(),
            applications: ApplicationCatalog::default(),
            fd_tick: 0,
        }
    }

    pub fn refresh(&mut self) -> PartialSourceSnapshot<ProcessItem> {
        self.refresh_at(taskmanager_core::core::time::unix_millis(
            std::time::SystemTime::now(),
        ))
    }

    pub fn refresh_at(&mut self, observed_at_ms: u64) -> PartialSourceSnapshot<ProcessItem> {
        // The per-process fd scan is deferred to every Nth tick. The first tick
        // (fd_tick == 0) is always a full read so a freshly-seen pid establishes
        // a value before any deferral. wrapping_add/rem_euclid keep this
        // panic-free across arbitrarily long uptimes.
        let want_fd_count = self.fd_tick.rem_euclid(FD_COUNT_REFRESH_EVERY_N_TICKS) == 0;
        self.fd_tick = self.fd_tick.wrapping_add(1);
        let procfs_probe = match probe_procfs() {
            Err(failure) => {
                self.rates.clear();
                let mut items = self.previous_items.items.clone();
                for item in &mut items {
                    mark_retained_item_stale(item, failure);
                }
                let item_count = items.len();
                return PartialSourceSnapshot::new(
                    items,
                    vec![source_status(
                        PROCESS_INVENTORY_PROVIDER,
                        SourceOutcome::Unavailable(failure),
                        item_count,
                    )],
                );
            }
            Ok(probe) => probe,
        };

        self.histories
            .begin_refresh(self.history_started_at.elapsed());
        self.system
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let mut process_pids: Vec<u32> = self
            .system
            .processes()
            .iter()
            .filter(|(_, process)| process.thread_kind().is_none())
            .map(|(pid, _)| pid.as_u32())
            .collect();
        process_pids.sort_unstable();
        self.memory_maps.refresh(&process_pids, observed_at_ms);
        let passwd = self.passwd_cache.labels(observed_at_ms);
        let boot_time = self.boot_time_cache.value(observed_at_ms);
        let clock_ticks = observe_clock_ticks();
        let mut process_stat = FieldSourceSummary::default();
        let mut process_fds = FieldSourceSummary::default();
        let mut process_memory = FieldSourceSummary::default();
        let mut process_memory_pss = FieldSourceSummary::default();
        let mut process_swap = FieldSourceSummary::default();
        let mut process_io = FieldSourceSummary::default();
        let mut process_rates = FieldSourceSummary::default();
        let mut process_owner_identity = FieldSourceSummary::default();
        let mut process_owner_label = FieldSourceSummary::default();
        let mut process_executable = FieldSourceSummary::default();
        let mut process_application = FieldSourceSummary::default();
        let mut items = Vec::new();
        for (pid, process) in self.system.processes() {
            // sysinfo 0.31 populates the process map with individual THREADS
            // (`thread_kind = Some(Userland/Kernel)`) alongside thread-group
            // leaders (`thread_kind = None`). A process list shows PROCESSES, not
            // threads — skip thread entries so Chrome's per-thread names
            // (`Chrome_ChildIOT`, `ThreadPoolForeg`, …) and the parent's memory
            // (duplicated onto every thread) don't flood the list. The per-process
            // thread COUNT column is unaffected (it comes from `/proc/<pid>/stat`,
            // not from enumerating threads here).
            if process.thread_kind().is_some() {
                continue;
            }
            let status = match process.status() {
                ProcessStatus::Run => "Running",
                ProcessStatus::Sleep => "Sleeping",
                ProcessStatus::Stop => "Stopped",
                ProcessStatus::Zombie => "Zombie",
                _ => "Other",
            }
            .to_owned();
            // Borrowed argv pieces (no per-arg String) with the display
            // cmdline built by pushing — the previous Vec<String> + join
            // allocated twice per process per tick.
            let process_argv: Vec<std::borrow::Cow<'_, str>> = process
                .cmd()
                .iter()
                .map(|part| part.to_string_lossy())
                .collect();
            let mut cmdline = String::new();
            for (index, part) in process_argv.iter().enumerate() {
                if index > 0 {
                    cmdline.push(' ');
                }
                cmdline.push_str(part);
            }
            let cmdline = if process_argv.is_empty() {
                process.name().to_string_lossy().into_owned()
            } else {
                cmdline
            };
            let pid = pid.as_u32();
            let name = process.name().to_string_lossy().into_owned();
            let previous = self.previous_items.find(pid);
            let (scalar_observations, evidence) = observe_process_scalars(
                pid,
                &boot_time,
                &clock_ticks,
                observed_at_ms,
                previous,
                &mut self.rates,
                want_fd_count,
            );
            let current_start_token = scalar_observations
                .start_token
                .current_value()
                .copied()
                .ok_or_else(|| {
                    scalar_observations
                        .start_token
                        .availability()
                        .failure()
                        .unwrap_or(FailureKind::IdentityChanged)
                });
            let memory_observation = current_start_token
                .map(|start_token| self.memory_maps.observe(pid, start_token))
                .unwrap_or_else(ProcessMemoryObservation::unavailable);
            let mut scalar_observations = scalar_observations;
            scalar_observations.memory_pss_bytes = match memory_observation.pss {
                Ok(value) => taskmanager_core::ScalarObservation::available(value, observed_at_ms),
                Err(failure) => taskmanager_core::ScalarObservation::unavailable(failure),
            };
            scalar_observations.swap_bytes = match memory_observation.swap {
                Ok(value) => taskmanager_core::ScalarObservation::available(value, observed_at_ms),
                Err(failure) => taskmanager_core::ScalarObservation::unavailable(failure),
            };
            if let (Ok(current_token), Some(previous)) = (current_start_token, previous)
                && previous.current_start_token() == Some(current_token)
            {
                scalar_observations =
                    scalar_observations.retain_previous(*previous.scalar_observations());
            }
            let (metadata_observations, metadata_evidence) = observe_process_metadata(
                pid,
                passwd,
                observed_at_ms,
                current_start_token,
                previous,
            );
            let (application_identity, application_outcome) =
                self.applications.observe_for_process(
                    pid,
                    current_start_token,
                    &metadata_observations.executable_path,
                    &process_argv,
                    observed_at_ms,
                );
            let application_identity =
                retain_for_same_identity(application_identity, current_start_token.ok(), previous);
            process_stat.record(evidence.stat);
            process_fds.record(evidence.fds);
            process_memory.record(evidence.memory);
            process_memory_pss.record(memory_observation.pss_outcome);
            process_swap.record(memory_observation.swap_outcome);
            process_io.record(evidence.io);
            process_rates.record(evidence.rates);
            process_owner_identity.record(metadata_evidence.owner_identity);
            process_owner_label.record(metadata_evidence.owner_label);
            process_executable.record(metadata_evidence.executable_path);
            process_application.record(application_outcome);
            let mut item = ProcessItem::new(pid, name);
            item.parent_pid = process.parent().map(|parent| parent.as_u32());
            item.cmdline = cmdline;
            item.status = status;
            item.apply_metadata_observations(metadata_observations);
            item.apply_application_identity(application_identity);
            item.apply_scalar_observations(scalar_observations);
            let history = self.histories.record(
                pid,
                item.current_start_token(),
                ProcessHistorySample::from_process(&item),
            );
            item.cpu_history = history.cpu;
            item.mem_history = history.memory;
            item.disk_history = history.disk;
            item.disk_read_history = history.disk_read;
            item.disk_write_history = history.disk_write;
            items.push(item);
        }
        self.histories.finish_refresh();
        let current_pids = items.iter().map(|item| item.pid).collect();
        self.rates.retain_pids(&current_pids);
        let item_count = items.len();
        let inventory_outcome = match procfs_probe {
            ProcfsProbe::Complete if items.is_empty() => SourceOutcome::Empty,
            ProcfsProbe::Complete => SourceOutcome::Available,
            ProcfsProbe::Partial(failure) => SourceOutcome::Partial(failure),
        };
        let boot_outcome = boot_time_outcome(&boot_time, item_count);
        let sources = vec![
            source_status(PROCESS_INVENTORY_PROVIDER, inventory_outcome, item_count),
            source_status(
                PROCESS_BOOT_TIME_PROVIDER,
                boot_outcome,
                usize::from(boot_time.is_ok()),
            ),
            source_status(
                PROCESS_CLOCK_TICKS_PROVIDER,
                scalar_result_outcome(&clock_ticks),
                usize::from(clock_ticks.is_ok()),
            ),
            source_status(
                PROCESS_STAT_PROVIDER,
                process_stat.outcome(item_count),
                process_stat.successes,
            ),
            source_status(
                PROCESS_FD_PROVIDER,
                process_fds.outcome(item_count),
                process_fds.successes,
            ),
            source_status(
                PROCESS_MEMORY_PROVIDER,
                process_memory.outcome(item_count),
                process_memory.successes,
            ),
            source_status(
                PROCESS_MEMORY_PSS_PROVIDER,
                process_memory_pss.outcome(item_count),
                process_memory_pss.successes,
            ),
            source_status(
                PROCESS_SWAP_PROVIDER,
                process_swap.outcome(item_count),
                process_swap.successes,
            ),
            source_status(
                PROCESS_IO_PROVIDER,
                process_io.outcome(item_count),
                process_io.successes,
            ),
            source_status(
                PROCESS_RATE_PROVIDER,
                process_rates.outcome(item_count),
                process_rates.successes,
            ),
            source_status(
                PROCESS_OWNER_IDENTITY_PROVIDER,
                process_owner_identity.outcome(item_count),
                process_owner_identity.populated,
            ),
            source_status(
                PROCESS_OWNER_LABEL_PROVIDER,
                process_owner_label.outcome(item_count),
                process_owner_label.populated,
            ),
            source_status(
                PROCESS_EXECUTABLE_PROVIDER,
                process_executable.outcome(item_count),
                process_executable.populated,
            ),
            source_status(
                PROCESS_APPLICATION_PROVIDER,
                process_application.outcome(item_count),
                process_application.populated,
            ),
        ];
        self.previous_items.sync_from(&items);
        PartialSourceSnapshot::new(items, sources)
    }
}

const PASSWD_CACHE_TTL_MS: u64 = 30_000;
const PASSWD_CACHE_RETRY_MS: u64 = 5_000;
const BOOT_TIME_CACHE_TTL_MS: u64 = 60_000;
const BOOT_TIME_CACHE_RETRY_MS: u64 = 5_000;

fn passwd_cache_interval(labels: &PasswdLabels) -> u64 {
    if labels.is_ok() {
        PASSWD_CACHE_TTL_MS
    } else {
        PASSWD_CACHE_RETRY_MS
    }
}

fn boot_time_cache_interval(value: &Result<u64, FailureKind>) -> u64 {
    if value.is_ok() {
        BOOT_TIME_CACHE_TTL_MS
    } else {
        BOOT_TIME_CACHE_RETRY_MS
    }
}

const PROCESS_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.procfs.inventory");
const PROCESS_BOOT_TIME_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.procfs.boot-time");
const PROCESS_CLOCK_TICKS_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.posix.clock-ticks");
const PROCESS_STAT_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.procfs.stat");
const PROCESS_FD_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.procfs.fd");
const PROCESS_MEMORY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.procfs.status-memory");
const PROCESS_MEMORY_PSS_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.procfs.memory-pss");
const PROCESS_SWAP_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.procfs.swap");
const PROCESS_IO_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.procfs.io");
const PROCESS_RATE_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.procfs.rates");
const PROCESS_OWNER_IDENTITY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.procfs.owner-identity");
const PROCESS_OWNER_LABEL_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.passwd.owner-label");
const PROCESS_EXECUTABLE_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.procfs.executable");
const PROCESS_APPLICATION_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.process.desktop.application");

/// Procfs availability once a filesystem-level failure has been excluded.
/// The unavailable state is represented by the `Err` of [`probe_procfs`], so
/// the refresh path handles it before any item work and the later outcome
/// match is exhaustive without a panic arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcfsProbe {
    Complete,
    Partial(FailureKind),
}

fn probe_procfs() -> Result<ProcfsProbe, FailureKind> {
    let entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(error) => return Err(io_failure(&error)),
    };
    let mut strongest = None;
    for entry in entries {
        if let Err(error) = entry {
            retain_strongest(&mut strongest, io_failure(&error));
        }
    }
    Ok(strongest.map_or(ProcfsProbe::Complete, ProcfsProbe::Partial))
}

#[derive(Default)]
struct FieldSourceSummary {
    successes: usize,
    populated: usize,
    strongest_failure: Option<FailureKind>,
}

impl FieldSourceSummary {
    fn record(&mut self, outcome: SourceOutcome) {
        match outcome {
            SourceOutcome::Available => {
                self.successes = self.successes.saturating_add(1);
                self.populated = self.populated.saturating_add(1);
            }
            SourceOutcome::Empty => {
                self.successes = self.successes.saturating_add(1);
            }
            SourceOutcome::Partial(failure) => {
                self.successes = self.successes.saturating_add(1);
                self.populated = self.populated.saturating_add(1);
                retain_strongest(&mut self.strongest_failure, failure);
            }
            SourceOutcome::Unavailable(failure) => {
                retain_strongest(&mut self.strongest_failure, failure);
            }
        }
    }

    fn outcome(&self, expected: usize) -> SourceOutcome {
        match (self.successes, self.strongest_failure, expected) {
            (0, None, 0) => SourceOutcome::Empty,
            (_, None, _) if self.populated == 0 => SourceOutcome::Empty,
            (_, None, _) => SourceOutcome::Available,
            (0, Some(failure), _) => SourceOutcome::Unavailable(failure),
            (_, Some(failure), _) => SourceOutcome::Partial(failure),
        }
    }
}

fn source_status(provider: ProviderId, outcome: SourceOutcome, item_count: usize) -> SourceStatus {
    SourceStatus {
        provider,
        outcome,
        item_count,
    }
}

fn boot_time_outcome(boot_time: &Result<u64, FailureKind>, process_count: usize) -> SourceOutcome {
    match boot_time {
        Ok(_) if process_count == 0 => SourceOutcome::Empty,
        Ok(_) => SourceOutcome::Available,
        Err(failure) => SourceOutcome::Unavailable(*failure),
    }
}

fn scalar_result_outcome<T>(result: &Result<T, FailureKind>) -> SourceOutcome {
    match result {
        Ok(_) => SourceOutcome::Available,
        Err(failure) => SourceOutcome::Unavailable(*failure),
    }
}

fn retain_strongest(current: &mut Option<FailureKind>, candidate: FailureKind) {
    if current.is_none_or(|failure| failure_priority(candidate) > failure_priority(failure)) {
        *current = Some(candidate);
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::MissingDependency => 7,
        FailureKind::TimedOut => 6,
        FailureKind::ProviderFault => 5,
        FailureKind::TemporarilyUnavailable => 4,
        FailureKind::Unsupported => 3,
        FailureKind::IdentityChanged => 2,
        FailureKind::Rejected => 1,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/engine/process.rs"]
mod tests;
