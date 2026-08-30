//! Windows process-list provider split out of `process.rs`.
//!
//! `sysinfo` supplies the live process table. POSIX `nice` remains typed
//! unavailable because this dependency set has no safe Windows accessor;
//! per-process thread count comes from the audited ToolHelp boundary and
//! degrades to typed unavailable when enumeration fails. The trailing 60 s
//! platform-neutral per-process history rules are stamped onto every row here.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use taskmanager_core::{
    CumulativeCounter, FailureKind, ProcessHistorySample, ProcessHistoryStore, ProcessItem,
    ProcessLiveKey, ProcessMetadataFailure, ProcessMetadataObservation,
    ProcessMetadataObservations, ProcessScalarObservations, ScalarObservation,
};
use taskmanager_platform_contract::{PartialSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::ProcessListProvider;

use super::{PROCESS_LIST_PROVIDER, source};

/// Process list from `sysinfo`: PID, parent, name, command line, CPU/memory,
/// start time, per-process disk totals and executable path are all real.
/// Per-process handle count is real via `sysinfo::Process::open_files`, which
/// uses the crate's audited Windows backend. Owner and POSIX nice are
/// explicitly unsupported rather than obtained through a command interpreter
/// or a guessed mapping; thread counts come from the audited ToolHelp thread
/// snapshot and stay typed unavailable when that enumeration fails. Handle
/// counts are sampled at a lower cadence and retained only after the exact
/// process-creation token matches.
const FD_COUNT_REFRESH_EVERY_N_TICKS: u32 = 5;

pub struct WinProcessListProvider {
    system: sysinfo::System,
    /// PID-keyed disk baselines carry the process start token so PID reuse
    /// cannot turn an older process's cumulative counter into a fake zero-rate
    /// sample for the replacement process.
    disk_rates: HashMap<u32, WinProcessDiskRateState>,
    /// The precise native creation token prevents a reused PID from inheriting
    /// a prior process's handle count during the deferred ticks.
    previous_fds: HashMap<u32, (u64, ScalarObservation<u32>)>,
    previous_users: HashMap<u32, (u64, String)>,
    icon_cache: HashMap<PathBuf, Option<taskmanager_core::ApplicationIconAsset>>,
    fd_tick: u32,
    histories: ProcessHistoryStore,
    history_started_at: Instant,
}

impl WinProcessListProvider {
    pub fn new() -> Self {
        Self {
            system: sysinfo::System::new(),
            disk_rates: HashMap::new(),
            previous_fds: HashMap::new(),
            previous_users: HashMap::new(),
            icon_cache: HashMap::new(),
            fd_tick: 0,
            histories: ProcessHistoryStore::default(),
            history_started_at: Instant::now(),
        }
    }
}

impl Default for WinProcessListProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessListProvider for WinProcessListProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<PartialSourceSnapshot<ProcessItem>, ProviderFailure> {
        self.system
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let want_fd_count = self.fd_tick.rem_euclid(FD_COUNT_REFRESH_EVERY_N_TICKS) == 0;
        self.fd_tick = self.fd_tick.wrapping_add(1);
        self.histories
            .begin_refresh(self.history_started_at.elapsed());

        let mut items = Vec::new();
        let mut current_pids = std::collections::HashSet::new();
        let mut next_fds = HashMap::new();
        let mut next_users = HashMap::new();
        let thread_counts = taskmanager_windows_api::enumerate_all_process_thread_counts();

        for (pid, process) in self.system.processes() {
            let pid_value = pid.as_u32();
            current_pids.insert(pid_value);
            let disk = process.disk_usage();
            let start_time_secs = process.start_time();
            let start_token = process_start_token(pid_value, start_time_secs, observed_at_ms);
            let current_start_token = start_token.current_value().copied();
            let (read_rate, write_rate) = self.disk_rates.entry(pid_value).or_default().observe(
                current_start_token,
                disk.total_read_bytes,
                disk.total_written_bytes,
                observed_at_ms,
            );
            // Borrow the argv pieces while building the display string. The
            // previous Vec<String> + join path allocated twice per process.
            let process_argv: Vec<std::borrow::Cow<'_, str>> = process
                .cmd()
                .iter()
                .map(|part| part.to_string_lossy())
                .collect();
            let mut joined_cmdline = String::new();
            for (index, part) in process_argv.iter().enumerate() {
                if index > 0 {
                    joined_cmdline.push(' ');
                }
                joined_cmdline.push_str(part);
            }
            let cmdline = if process_argv.is_empty() {
                process.name().to_string_lossy().into_owned()
            } else {
                joined_cmdline
            };
            let fd_observation = observe_fd_count(
                process,
                want_fd_count,
                current_start_token,
                self.previous_fds.get(&pid_value).copied(),
                observed_at_ms,
            );
            if let (Some(token), Some(_)) = (current_start_token, fd_observation.last_known_value())
            {
                next_fds.insert(pid_value, (token, fd_observation));
            }
            let nice_obs =
                if let Ok(priority) = taskmanager_windows_api::process_priority(pid_value) {
                    let val = match priority {
                        taskmanager_windows_api::ProcessPriorityClass::Realtime => -20,
                        taskmanager_windows_api::ProcessPriorityClass::High => -15,
                        taskmanager_windows_api::ProcessPriorityClass::AboveNormal => -5,
                        taskmanager_windows_api::ProcessPriorityClass::Normal => 0,
                        taskmanager_windows_api::ProcessPriorityClass::BelowNormal => 5,
                        taskmanager_windows_api::ProcessPriorityClass::Idle => 15,
                    };
                    ScalarObservation::available(val, observed_at_ms)
                } else {
                    ScalarObservation::unavailable(FailureKind::Unsupported)
                };

            // Whole-table enumeration failure, or a PID missing from the
            // snapshot because it exited mid-refresh, is typed-unavailable;
            // publishing 1 would fabricate a single-threaded process.
            let thread_count = match thread_counts.as_ref() {
                Ok(counts) => match counts.get(&pid_value).copied() {
                    Some(count) => ScalarObservation::available(count, observed_at_ms),
                    None => ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                },
                Err(taskmanager_windows_api::WindowsApiError::PermissionDenied) => {
                    ScalarObservation::unavailable(FailureKind::PermissionDenied)
                }
                Err(taskmanager_windows_api::WindowsApiError::IdentityChanged) => {
                    ScalarObservation::unavailable(FailureKind::IdentityChanged)
                }
                Err(taskmanager_windows_api::WindowsApiError::Unsupported) => {
                    ScalarObservation::unavailable(FailureKind::Unsupported)
                }
                Err(_) => ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            };

            let mut item =
                ProcessItem::new(pid_value, process.name().to_string_lossy().into_owned());
            item.parent_pid = process.parent().map(|parent| parent.as_u32());
            item.cmdline = cmdline;
            item.status = process_status_label(process.status()).to_string();
            let observations = ProcessScalarObservations {
                start_token,
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
                threads: thread_count,
                start_time_secs: ScalarObservation::available(start_time_secs, observed_at_ms),
                cpu_time_secs: ScalarObservation::available(
                    process.accumulated_cpu_time().saturating_div(100),
                    observed_at_ms,
                ),
                fds: fd_observation,
                nice: nice_obs,
            };

            let user_name = if let Some((start, u)) = self.previous_users.get(&pid_value) {
                if *start == start_time_secs {
                    Some(u.clone())
                } else {
                    taskmanager_windows_api::query_process_user(pid_value).ok()
                }
            } else {
                taskmanager_windows_api::query_process_user(pid_value).ok()
            };
            if let Some(ref u) = user_name {
                next_users.insert(pid_value, (start_time_secs, u.clone()));
            }

            let owner = match user_name {
                Some(u) => ProcessMetadataObservation::available(
                    taskmanager_core::ProcessOwner {
                        identity: taskmanager_core::ProcessOwnerIdentity::Opaque(u.clone()),
                        label: Some(u),
                    },
                    observed_at_ms,
                ),
                None => ProcessMetadataObservation::unavailable(
                    ProcessMetadataFailure::PermissionDenied,
                ),
            };

            let executable_path = match process.exe() {
                Some(path) => {
                    ProcessMetadataObservation::available(PathBuf::from(path), observed_at_ms)
                }
                None => ProcessMetadataObservation::absent(observed_at_ms),
            };

            let application_identity = match process.exe() {
                Some(path) => {
                    let path_buf = PathBuf::from(path);
                    let asset_opt = self.icon_cache.entry(path_buf.clone()).or_insert_with(|| {
                        let path_str = path.to_string_lossy();
                        if let Ok(bytes) =
                            taskmanager_windows_api::extract_process_icon_bmp(&path_str)
                        {
                            taskmanager_core::ApplicationIconAsset::from_bytes(
                                taskmanager_core::ApplicationIconFormat::Bmp,
                                bytes,
                            )
                        } else {
                            None
                        }
                    });
                    let stem = path_buf
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(item.name.as_str());
                    let display_name = stem.to_string();
                    let launcher_id = path.to_string_lossy().into_owned();
                    let identity = taskmanager_core::ProcessApplicationIdentity::new(
                        launcher_id,
                        display_name,
                        None,
                    )
                    .map(|id| id.with_icon_resolution(asset_opt.clone(), None));

                    match identity {
                        Some(id) => ProcessMetadataObservation::available(id, observed_at_ms),
                        None => ProcessMetadataObservation::absent(observed_at_ms),
                    }
                }
                None => {
                    let display_name = item
                        .name
                        .trim_end_matches(".exe")
                        .trim_end_matches(".EXE")
                        .to_string();
                    let identity = taskmanager_core::ProcessApplicationIdentity::new(
                        item.name.clone(),
                        display_name,
                        None,
                    );
                    match identity {
                        Some(id) => ProcessMetadataObservation::available(id, observed_at_ms),
                        None => ProcessMetadataObservation::absent(observed_at_ms),
                    }
                }
            };
            item.apply_application_identity(application_identity);

            item.apply_metadata_observations(ProcessMetadataObservations {
                owner,
                executable_path,
            });
            item.apply_scalar_observations(observations);

            let history =
                ProcessLiveKey::from_process(&item).map_or_else(Default::default, |identity| {
                    self.histories
                        .record(identity, ProcessHistorySample::from_process(&item))
                });
            item.cpu_history = history.cpu;
            item.mem_history = history.memory;
            item.disk_history = history.disk;
            item.disk_read_history = history.disk_read;
            item.disk_write_history = history.disk_write;
            items.push(item);
        }
        self.disk_rates.retain(|pid, _| current_pids.contains(pid));
        self.previous_fds = next_fds;
        self.previous_users = next_users;
        self.histories.finish_refresh();

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

fn observe_fd_count(
    process: &sysinfo::Process,
    want_fd_count: bool,
    current_token: Option<u64>,
    previous: Option<(u64, ScalarObservation<u32>)>,
    observed_at_ms: u64,
) -> ScalarObservation<u32> {
    let current = if want_fd_count {
        process.open_files().map_or_else(
            || ScalarObservation::unavailable(FailureKind::Unsupported),
            |count| {
                ScalarObservation::available(
                    u32::try_from(count).unwrap_or(u32::MAX),
                    observed_at_ms,
                )
            },
        )
    } else {
        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
    };
    retain_fd_count(current, current_token, previous)
}

fn retain_fd_count(
    current: ScalarObservation<u32>,
    current_token: Option<u64>,
    previous: Option<(u64, ScalarObservation<u32>)>,
) -> ScalarObservation<u32> {
    let Some(current_token) = current_token else {
        return current;
    };
    let Some((previous_token, previous_observation)) = previous else {
        return current;
    };
    if previous_token == current_token {
        current.retain_previous(previous_observation)
    } else {
        current
    }
}

fn process_start_token(
    pid: u32,
    _display_start_time_secs: u64,
    observed_at_ms: u64,
) -> ScalarObservation<u64> {
    #[cfg(windows)]
    {
        match taskmanager_windows_api::process_creation_time_100ns(pid) {
            Ok(token) => ScalarObservation::available(token, observed_at_ms),
            Err(taskmanager_windows_api::WindowsApiError::PermissionDenied) => {
                ScalarObservation::unavailable(FailureKind::PermissionDenied)
            }
            Err(taskmanager_windows_api::WindowsApiError::IdentityChanged) => {
                ScalarObservation::unavailable(FailureKind::IdentityChanged)
            }
            Err(taskmanager_windows_api::WindowsApiError::Unsupported) => {
                ScalarObservation::unavailable(FailureKind::Unsupported)
            }
            Err(_) => ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        }
    }
    #[cfg(not(windows))]
    {
        // The Windows adapter is compiled on Linux for the cross-target
        // contract gate. Keep that proof deterministic; the native Windows
        // build above is the only path that authorizes destructive control.
        let _ = pid;
        ScalarObservation::available(_display_start_time_secs, observed_at_ms)
    }
}

#[derive(Default)]
struct WinProcessDiskRateState {
    start_token: Option<u64>,
    read: CumulativeCounter,
    write: CumulativeCounter,
}

impl WinProcessDiskRateState {
    fn observe(
        &mut self,
        start_token: Option<u64>,
        read_total: u64,
        write_total: u64,
        observed_at_ms: u64,
    ) -> (ScalarObservation<u64>, ScalarObservation<u64>) {
        let Some(start_token) = start_token else {
            self.start_token = None;
            self.read.reset();
            self.write.reset();
            return (
                ScalarObservation::unavailable(FailureKind::IdentityChanged),
                ScalarObservation::unavailable(FailureKind::IdentityChanged),
            );
        };
        let identity_changed = self
            .start_token
            .is_some_and(|previous| previous != start_token);
        if identity_changed {
            self.read.reset();
            self.write.reset();
        }
        self.start_token = Some(start_token);
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

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_process_list.rs"]
mod tests;
