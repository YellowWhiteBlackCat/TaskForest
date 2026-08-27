//! Per-thread breakdown of a process, read from `/proc/<pid>/task`.
//!
//! Each `<tid>` directory under `task/` owns a `stat` file with the same layout
//! as `/proc/<pid>/stat`. Field 2 (comm) may contain spaces and is wrapped in
//! parentheses, so it is parsed by locating the outermost parentheses rather
//! than by splitting on whitespace.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::{
    FailureKind, ProcessIdentity, ProcessThreadInfo, ProcessThreads, ThreadState,
};

use super::{state_for_status, status_from_io_error};

/// Compatibility divisor used only by the public fixture-oriented collector;
/// the identity-bound provider path uses the live `sysconf(_SC_CLK_TCK)` value.
pub const PROC_CLK_TCK: u64 = 100;

#[derive(Debug, Clone, Copy)]
struct ThreadCounterPoint {
    ticks: u64,
    observed_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ThreadRateKey {
    process: ProcessIdentity,
    tid: u32,
}

/// Identity-bound per-thread CPU rate state. It is deliberately private to
/// the Linux collector: the shared model receives only the measured option,
/// never procfs paths or a provider-specific baseline map.
#[derive(Debug, Default)]
pub(super) struct ThreadCpuRateTracker {
    baselines: HashMap<ThreadRateKey, ThreadCounterPoint>,
}

impl ThreadCpuRateTracker {
    fn activate_process(&mut self, process: ProcessIdentity) {
        self.baselines.retain(|key, _| key.process == process);
    }

    fn observe(
        &mut self,
        process: ProcessIdentity,
        tid: u32,
        ticks: u64,
        observed_at_ms: u64,
        clock_ticks: &Result<u64, FailureKind>,
    ) -> Option<f32> {
        let Some(ticks_per_second) = clock_ticks
            .as_ref()
            .ok()
            .copied()
            .filter(|value| *value > 0)
        else {
            self.reset_process(process);
            return None;
        };
        let key = ThreadRateKey { process, tid };
        let previous = self.baselines.insert(
            key,
            ThreadCounterPoint {
                ticks,
                observed_at_ms,
            },
        );
        let previous = previous?;
        let elapsed_ms = observed_at_ms.checked_sub(previous.observed_at_ms)?;
        let delta_ticks = ticks.checked_sub(previous.ticks)?;
        let percentage =
            (delta_ticks as f64 * 100_000.0) / (ticks_per_second as f64 * elapsed_ms as f64);
        (percentage.is_finite() && percentage >= 0.0).then_some(percentage as f32)
    }

    fn retain(&mut self, process: ProcessIdentity, tids: &HashSet<u32>) {
        self.baselines
            .retain(|key, _| key.process == process && tids.contains(&key.tid));
    }

    fn reset_process(&mut self, process: ProcessIdentity) {
        self.baselines.retain(|key, _| key.process != process);
    }
}

/// Parsed fields from one `/proc/<pid>/task/<tid>/stat` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadStatFields {
    /// Field 2: the executable name, with surrounding parentheses removed.
    pub comm: String,
    /// Field 3: the first character of the scheduler state (e.g. `R`, `S`).
    pub state_char: char,
    /// Field 14: user-mode CPU time in clock ticks.
    pub utime: u64,
    /// Field 15: kernel-mode CPU time in clock ticks.
    pub stime: u64,
}

impl ThreadStatFields {
    #[must_use]
    pub fn cpu_ticks_total(&self) -> Option<u64> {
        self.utime.checked_add(self.stime)
    }
}

/// Parse `/proc/<pid>/task/<tid>/stat` (same layout as `/proc/<pid>/stat`).
///
/// Returns `None` for any malformed line (missing parentheses, too few
/// fields, non-numeric CPU counters) so callers never fabricate a thread.
#[must_use]
pub fn parse_thread_stat(text: &str) -> Option<ThreadStatFields> {
    let left_paren = text.find('(')?;
    let right_paren = text.rfind(')')?;
    if right_paren <= left_paren {
        return None;
    }
    let comm = text.get(left_paren + 1..right_paren)?.to_string();
    // After the closing paren, fields are 1-indexed starting at field 3
    // (state). utime/stime are fields 14/15, i.e. tail indices 11/12.
    let tail: Vec<&str> = text.get(right_paren + 1..)?.split_whitespace().collect();
    let state_char = tail.first()?.chars().next()?;
    let utime: u64 = tail.get(11)?.parse().ok()?;
    let stime: u64 = tail.get(12)?.parse().ok()?;
    Some(ThreadStatFields {
        comm,
        state_char,
        utime,
        stime,
    })
}

/// Read every thread of `proc_dir` (`/proc/<pid>`) into a typed per-thread
/// facet. Designed to be called from a collector that has already pinned the
/// process start-time token.
pub fn collect_threads_from_proc_dir(proc_dir: &Path, now_ms: u64) -> ProcessThreads {
    collect_threads_from_proc_dir_inner(proc_dir, now_ms, None)
}

/// Collect a process's threads and derive identity-bound CPU percentages from
/// consecutive procfs samples. The first sample intentionally carries
/// `cpu_percent = None`, while cumulative CPU time remains available.
pub(super) fn collect_threads_with_cpu_rate(
    proc_dir: &Path,
    identity: ProcessIdentity,
    now_ms: u64,
    clock_ticks: &Result<u64, FailureKind>,
    rates: &mut ThreadCpuRateTracker,
) -> ProcessThreads {
    collect_threads_from_proc_dir_inner(proc_dir, now_ms, Some((identity, clock_ticks, rates)))
}

fn collect_threads_from_proc_dir_inner<'a>(
    proc_dir: &Path,
    now_ms: u64,
    rate_context: Option<(
        ProcessIdentity,
        &'a Result<u64, FailureKind>,
        &'a mut ThreadCpuRateTracker,
    )>,
) -> ProcessThreads {
    let task_dir = proc_dir.join("task");
    let mut seen_tids = HashSet::new();
    let (identity, clock_ticks, mut rates) = match rate_context {
        Some((identity, clock_ticks, rates)) => (Some(identity), Some(clock_ticks), Some(rates)),
        None => (None, None, None),
    };
    if let (Some(identity), Some(rates)) = (identity, rates.as_deref_mut()) {
        rates.activate_process(identity);
    }
    let entries = match std::fs::read_dir(&task_dir) {
        Ok(entries) => entries,
        Err(error) => {
            if let (Some(identity), Some(rates)) = (identity, rates.as_deref_mut()) {
                rates.reset_process(identity);
            }
            return ProcessThreads {
                state: state_for_status(status_from_io_error(&error), now_ms),
                ..ProcessThreads::default()
            };
        }
    };

    let mut threads = Vec::new();
    for entry in entries.flatten() {
        let Some(tid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let stat_path = task_dir.join(tid.to_string()).join("stat");
        let Ok(stat_text) = std::fs::read_to_string(&stat_path) else {
            // Threads can exit between enumeration and stat read; skip them
            // rather than failing the whole facet.
            continue;
        };
        let Some(fields) = parse_thread_stat(&stat_text) else {
            continue;
        };
        let Some(cpu_ticks) = fields.cpu_ticks_total() else {
            continue;
        };
        seen_tids.insert(tid);
        let cpu_percent = match (identity, clock_ticks, rates.as_deref_mut()) {
            (Some(identity), Some(clock_ticks), Some(rates)) => {
                rates.observe(identity, tid, cpu_ticks, now_ms, clock_ticks)
            }
            _ => None,
        };
        let cpu_time_secs = match clock_ticks {
            None => Some(cpu_ticks as f64 / PROC_CLK_TCK as f64),
            Some(Ok(ticks_per_second)) if *ticks_per_second > 0 => {
                Some(cpu_ticks as f64 / *ticks_per_second as f64)
            }
            Some(_) => None,
        };
        threads.push(ProcessThreadInfo {
            tid,
            comm: fields.comm,
            state: ThreadState::from_char(fields.state_char),
            cpu_time_secs,
            cpu_percent,
        });
    }

    if let (Some(identity), Some(rates)) = (identity, rates) {
        rates.retain(identity, &seen_tids);
    }

    threads.sort_by_key(|thread| thread.tid);

    ProcessThreads {
        // An empty thread list on a live process is still a healthy read: the
        // process genuinely has its single representative thread (or the list
        // raced and will be repopulated on the next sample).
        state: state_for_status(DeviceStatus::Healthy, now_ms),
        threads,
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_process_telemetry_threads_tests.rs"]
mod tests;
