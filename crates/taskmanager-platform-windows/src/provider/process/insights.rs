//! Per-process insight facets backed by the audited Windows boundaries:
//! threads (ToolHelp32 snapshot + `GetThreadTimes`), open files (the
//! system-handle-table walk), and the environment block (PEB reads). Every
//! observe brackets its native query with the creation-time identity check,
//! so PID reuse can never publish a replacement process's facts.

use std::collections::HashMap;

use taskmanager_core::{
    CounterDelta, CumulativeCounter, DeviceState, FailureKind, FrozenProcessIdentity,
    ProcessEnvironment, ProcessInsightSnapshot, ProcessOpenFiles, ProcessThreadInfo,
    ProcessThreads, ThreadState,
};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::{
    ProcessEnvironmentProvider, ProcessOpenFilesProvider, ProcessThreadsProvider,
};

#[cfg(windows)]
use super::map_windows_api_failure;
use super::{snapshot_identity, validate_process_target, validate_process_target_after};

/// Per-tid CPU rate baselines for the threads facet. `WinProcessThreadsProvider`
/// is constructed as a unit value in `provider.rs` (outside this module's
/// ownership), so the identity-guarded baselines live in a bounded
/// process-wide table instead of a struct field; the creation token keeps a
/// reused pid from inheriting another process's counters, and a poisoned lock
/// still holds identity-guarded baselines, so recovery keeps rates honest.
static THREAD_CPU_BASELINES: std::sync::LazyLock<
    std::sync::Mutex<HashMap<u32, ThreadCpuBaselines>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// The threads facet observes focused targets, not the whole table; the bound
/// keeps a long-lived process from growing the baseline table without end.
const MAX_TRACKED_THREAD_TARGETS: usize = 128;

struct ThreadCpuBaselines {
    start_token: u64,
    per_tid: HashMap<u32, CumulativeCounter>,
}

/// Assemble the contract's thread rows from the boundary details, computing
/// `cpu_percent` from identity-bound cumulative-CPU deltas. The first sample
/// of a tid (and any zero-width or rolled-back window) is `None` — a missing
/// rate is never a believable zero.
fn thread_rows_with_rates(
    target: &FrozenProcessIdentity,
    details: Vec<taskmanager_windows_api::WindowsThreadDetail>,
    observed_at_ms: u64,
) -> Vec<ProcessThreadInfo> {
    let mut details = details;
    details.sort_by_key(|detail| detail.tid);

    let token = target.authoritative_start_token().unwrap_or(0);
    let mut baselines = match THREAD_CPU_BASELINES.lock() {
        Ok(baselines) => baselines,
        Err(poisoned) => poisoned.into_inner(),
    };
    if baselines.len() >= MAX_TRACKED_THREAD_TARGETS && !baselines.contains_key(&target.pid) {
        baselines.clear();
    }
    let entry = baselines
        .entry(target.pid)
        .or_insert_with(|| ThreadCpuBaselines {
            start_token: token,
            per_tid: HashMap::new(),
        });
    if entry.start_token != token {
        entry.start_token = token;
        entry.per_tid.clear();
    }

    let mut next_per_tid: HashMap<u32, CumulativeCounter> = HashMap::with_capacity(details.len());
    let mut threads = Vec::with_capacity(details.len());
    for detail in details {
        let mut cpu_percent = None;
        if let Some(secs) = detail.cpu_time_secs
            && secs.is_finite()
            && secs >= 0.0
        {
            let counter = next_per_tid.entry(detail.tid).or_default();
            let cpu_100ns = (secs * 1e7).round() as u64;
            if let CounterDelta::Available { value, elapsed_ms } = counter.observe(
                Ok(cpu_100ns),
                observed_at_ms,
                FailureKind::TemporarilyUnavailable,
            ) {
                let wall_secs = elapsed_ms as f64 / 1000.0;
                if wall_secs > 0.0 {
                    let cpu_secs = value as f64 / 1e7;
                    // One thread runs on at most one logical CPU, so the
                    // single-core-equivalent rate clamps to 100%.
                    cpu_percent = Some(((cpu_secs / wall_secs * 100.0).clamp(0.0, 100.0)) as f32);
                }
            }
        }
        threads.push(ProcessThreadInfo {
            tid: detail.tid,
            // GetThreadDescription is absent for unnamed threads; an empty
            // comm is the contract's honest "no name" shape.
            comm: detail.name.unwrap_or_default(),
            // ToolHelp32 exposes no scheduler state on Windows; `Other` is
            // the honest mapping, not a gap to close later.
            state: ThreadState::Other,
            cpu_time_secs: detail.cpu_time_secs,
            cpu_percent,
        });
    }
    // Dead tids leave with the old map; live ones carried their baselines over.
    entry.per_tid = next_per_tid;
    threads
}

pub struct WinProcessThreadsProvider;

impl ProcessThreadsProvider for WinProcessThreadsProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessThreads>, ProviderFailure> {
        let expected = validate_process_target(target)?;
        let details =
            taskmanager_windows_api::query_process_thread_details(target.pid).map_err(|err| {
                match err {
                    taskmanager_windows_api::WindowsApiError::PermissionDenied => {
                        ProviderFailure::PermissionDenied
                    }
                    taskmanager_windows_api::WindowsApiError::IdentityChanged => {
                        ProviderFailure::IdentityChanged
                    }
                    taskmanager_windows_api::WindowsApiError::Unsupported => {
                        ProviderFailure::Unsupported
                    }
                    _ => ProviderFailure::ProviderFault,
                }
            })?;
        validate_process_target_after(target, expected)?;

        let threads = thread_rows_with_rates(target, details, observed_at_ms);

        let value = ProcessThreads {
            state: DeviceState::healthy(observed_at_ms),
            threads,
        };

        Ok(ProcessInsightSnapshot {
            identity: snapshot_identity(target),
            value,
        })
    }
}

/// Per-process open files from the audited system-handle-table boundary
/// (route B, ADR-018). Only File-type kernel objects are open files: loaded
/// modules are a different fact, and sockets stay to the connections insight
/// that owns them on Windows. Owners that refuse `PROCESS_DUP_HANDLE`
/// (other users' processes) surface as the typed permission failure; the
/// creation-token bracket around the walk keeps PID reuse from publishing
/// replacement facts.
pub struct WinProcessOpenFilesProvider;

impl ProcessOpenFilesProvider for WinProcessOpenFilesProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessOpenFiles>, ProviderFailure> {
        #[cfg(windows)]
        {
            let expected = validate_process_target(target)?;
            let raw_entries = taskmanager_windows_api::query_process_open_files(target.pid)
                .map_err(map_windows_api_failure)?;
            validate_process_target_after(target, expected)?;

            let value = open_files_value_from_boundary(raw_entries, observed_at_ms);
            Ok(ProcessInsightSnapshot {
                identity: snapshot_identity(target),
                value,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (target, observed_at_ms);
            Err(ProviderFailure::Unsupported)
        }
    }
}

/// Project the boundary's File-object handles onto the contract facet:
/// handles that exceed `u32::MAX` are counted unreadable rather than
/// truncated, unresolved targets are counted unreadable while their entries
/// stay, and the listing is ordered by ascending fd so downstream diffing is
/// stable.
#[cfg(any(windows, test))]
pub(crate) fn open_files_value_from_boundary(
    raw_entries: Vec<taskmanager_windows_api::WindowsOpenHandleEntry>,
    observed_at_ms: u64,
) -> ProcessOpenFiles {
    use taskmanager_core::{OpenFileEntry, OpenFileKind};

    let mut unreadable_count = 0_u32;
    let mut entries = Vec::with_capacity(raw_entries.len());
    for raw in raw_entries {
        // Native handle values cannot honestly exceed `u32::MAX` today, but
        // the projection refuses to truncate if one ever does.
        let Ok(fd) = u32::try_from(raw.handle) else {
            unreadable_count = unreadable_count.saturating_add(1);
            continue;
        };
        if raw.target.is_none() {
            unreadable_count = unreadable_count.saturating_add(1);
        }
        let kind = match raw.kind {
            taskmanager_windows_api::WindowsOpenHandleKind::File => OpenFileKind::File,
            taskmanager_windows_api::WindowsOpenHandleKind::Pipe => OpenFileKind::Pipe,
            // Windows has no honest socket source in this walk (the
            // connections insight owns that fact), so Other stays Other.
            taskmanager_windows_api::WindowsOpenHandleKind::Other => OpenFileKind::Other,
        };
        entries.push(OpenFileEntry {
            fd,
            kind,
            target: raw.target,
        });
    }
    entries.sort_unstable_by_key(|entry| entry.fd);
    ProcessOpenFiles {
        state: DeviceState::healthy(observed_at_ms),
        entries,
        unreadable_count,
    }
}

/// Per-process environment variables and working directory from the audited
/// PEB/`ReadProcessMemory` boundary (the Windows counterpart of Linux's
/// `/proc/<pid>/{environ,cwd}`). Same-user processes are readable; owners that
/// refuse `PROCESS_VM_READ` surface as the typed permission failure, and a
/// WOW64 target seen from the other bitness is a typed `Unsupported` rather
/// than a guessed PEB layout. The creation-token bracket around the read
/// keeps PID reuse from publishing a replacement process's facts.
// Dead until the registration swap in `provider.rs` (integrator-owned)
// constructs `WinProcessEnvironmentProvider` via the runtime's environment
// facet (`with_environment`).
pub struct WinProcessEnvironmentProvider;

impl ProcessEnvironmentProvider for WinProcessEnvironmentProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessEnvironment>, ProviderFailure> {
        #[cfg(windows)]
        {
            let expected = validate_process_target(target)?;
            let raw = taskmanager_windows_api::query_process_environment(target.pid)
                .map_err(map_windows_api_failure)?;
            validate_process_target_after(target, expected)?;

            let value = environment_value_from_boundary(raw, observed_at_ms);
            Ok(ProcessInsightSnapshot {
                identity: snapshot_identity(target),
                value,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (target, observed_at_ms);
            Err(ProviderFailure::Unsupported)
        }
    }
}

/// Project the boundary's bounded block onto the contract facet: entries keep
/// their source order, the working directory stays an honest `None` when the
/// native source exposed none, and the boundary's truncation count passes
/// through unchanged (the boundary already enforces the contract's
/// byte/entry budgets).
// Dead until the registration swap in `provider.rs` (integrator-owned)
// constructs `WinProcessEnvironmentProvider` via the runtime's environment
// facet (`with_environment`).
#[cfg(any(windows, test))]
pub(crate) fn environment_value_from_boundary(
    raw: taskmanager_windows_api::WindowsProcessEnvironmentBlock,
    observed_at_ms: u64,
) -> ProcessEnvironment {
    use std::path::PathBuf;

    use taskmanager_core::ProcessEnvironmentEntry;

    ProcessEnvironment {
        state: DeviceState::healthy(observed_at_ms),
        working_directory: raw.working_directory.map(PathBuf::from),
        entries: raw
            .entries
            .into_iter()
            .map(|(key, value)| ProcessEnvironmentEntry { key, value })
            .collect(),
        truncated_count: raw.truncated_count,
    }
}
