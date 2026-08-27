//! WSL distribution rollup sampling over the bounded fixed-program channel.
//!
//! The registry inventory (`taskmanager-windows-api::query_wsl_distributions`)
//! lists registered distributions but carries no runtime state, and WSL2 guest
//! processes are invisible to host-side enumeration: every Linux process lives
//! inside the shared utility VM behind `vmmem`. The supported observation
//! channel is `wsl.exe` itself — a fixed management binary, never a command
//! interpreter — running two fixed Linux programs inside an already-running
//! distribution: `ls -1 /proc` enumerates the distribution's pid namespace,
//! then `cat` reads each member's `/proc/<pid>/stat` and `/proc/<pid>/status`.
//!
//! Honesty rules specific to this channel:
//! - A stopped distribution is never sampled: executing anything against it
//!   cold-boots the utility VM, a multi-second user-visible side effect. Its
//!   row keeps typed-unavailable metrics and an empty member list.
//! - Per-distribution values are aggregates over thread leaders only (the
//!   `/proc` top level). CPU of non-leader threads and page-cache memory are
//!   known undercounts, recorded in the telemetry contract.
//! - Decoding and parsing below are pure and cross-platform, so the parser
//!   surface is testable on any host; only the two spawn helpers touch
//!   `wsl.exe`, and they ride the audited bounded-command lifecycle.

use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::time::Duration;

use taskmanager_core::{ContainerSummary, FailureKind, IsolationKind, ScalarObservation};

use crate::command::run_with_timeout;

const WSL_EXE: &str = "wsl.exe";
const WSL_LIST_TIMEOUT: Duration = Duration::from_secs(3);
const WSL_SAMPLE_TIMEOUT: Duration = Duration::from_secs(4);
/// Pids per `cat` invocation; keeps the wsl.exe command line far below the
/// 32 KiB CreateProcess ceiling even for busy distributions.
const WSL_CAT_CHUNK_PIDS: usize = 256;
/// Hard per-refresh sampling ceiling per distribution; beyond it the rollup
/// reports a typed incomplete (Stale) state instead of silently truncating.
const WSL_SAMPLE_PID_CAP: usize = 2048;
/// Linux `USER_HZ` for WSL kernels; converts `/proc` jiffies to wall time.
const WSL_USER_HZ: f64 = 100.0;

/// One per-pid reading merged from `/proc/<pid>/stat` (cumulative CPU) and
/// `/proc/<pid>/status` (`VmRSS`). `rss_bytes` is zero when the status block
/// was unreadable — the memory aggregate is then a sum over known members.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WslProcSample {
    pid: u32,
    cpu_jiffies: u64,
    rss_bytes: u64,
}

/// Result of one rollup pass: the rows plus whether every running
/// distribution was sampled to completion (false downgrades the rollup to a
/// typed Stale state, mirroring the Linux cgroup collector's capped-scan rule).
#[derive(Debug)]
pub(super) struct WslRollupOutcome {
    pub containers: Vec<ContainerSummary>,
    pub complete: bool,
}

/// Decode `wsl.exe`'s own console listing. The tool renders UTF-16LE (with or
/// without a BOM); a `cat`/`ls` passthrough or a UTF-8 build decodes as UTF-8.
/// A NUL byte anywhere is the UTF-16LE marker; both paths are lossy so no
/// byte sequence can panic or fail the refresh.
fn decode_wsl_console_text(bytes: &[u8]) -> String {
    if bytes.contains(&0) {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        String::from_utf16_lossy(&units)
            .trim_start_matches('\u{feff}')
            .to_owned()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Keep only the lines of a `--list --running` rendering that name one of the
/// registered distributions. Localized headers are dropped structurally (the
/// intersection with registry names never relies on message text); a leading
/// `*` default marker and CRLF endings are trimmed first.
fn running_names_from_list_text(text: &str, registered: &HashSet<&str>) -> Vec<String> {
    text.lines()
        .map(|line| line.trim().trim_start_matches('*').trim())
        .filter(|line| !line.is_empty() && !line.contains(':'))
        .filter(|line| registered.contains(line))
        .map(ToOwned::to_owned)
        .collect()
}

/// Extract the numeric top-level entries of an `ls -1 /proc` listing. Non-numeric
/// entries (`cpuinfo`, `self`, ...) fail the `u32` parse and drop out; output is
/// sorted and deduplicated so the member list is a stable identity set.
fn numeric_proc_ids(bytes: &[u8]) -> Vec<u32> {
    let mut pids: Vec<u32> = String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect();
    pids.sort_unstable();
    pids.dedup();
    pids
}

/// Parse one `/proc/<pid>/stat` line into `(pid, utime + stime)`. The comm field
/// may contain spaces and parentheses, so fields are addressed relative to the
/// last `)`; a malformed line yields `None` rather than a partial guess.
fn parse_proc_stat_line(line: &str) -> Option<(u32, u64)> {
    let close = line.rfind(')')?;
    let pid = line[..close].split('(').next()?.trim().parse().ok()?;
    let fields: Vec<&str> = line[close + 1..].split_whitespace().collect();
    // fields[0] is the state, fields[1] the ppid; utime/stime are stat fields
    // 14/15, which land at indexes 11/12 after the parenthesized comm.
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some((pid, utime.saturating_add(stime)))
}

/// Merge one concatenated `cat` payload of interleaved `/proc/<pid>/stat` and
/// `/proc/<pid>/status` blocks into per-pid samples. Every line is
/// self-describing — stat lines embed the pid, status blocks carry a `Pid:`
/// line ahead of `VmRSS:` — so vanished members reorder or drop harmlessly.
fn merge_proc_samples(text: &str) -> Vec<WslProcSample> {
    let mut cpu_by_pid: HashMap<u32, u64> = HashMap::new();
    let mut rss_by_pid: HashMap<u32, u64> = HashMap::new();
    let mut status_pid: Option<u32> = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Pid:") {
            status_pid = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("VmRSS:") {
            let rss_kib = value
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .ok();
            if let (Some(pid), Some(rss_kib)) = (status_pid, rss_kib) {
                rss_by_pid.insert(pid, rss_kib.saturating_mul(1024));
            }
        } else if let Some((pid, cpu_jiffies)) = parse_proc_stat_line(line) {
            cpu_by_pid.insert(pid, cpu_jiffies);
        }
    }
    let mut samples: Vec<WslProcSample> = cpu_by_pid
        .into_iter()
        .map(|(pid, cpu_jiffies)| WslProcSample {
            pid,
            cpu_jiffies,
            rss_bytes: rss_by_pid.get(&pid).copied().unwrap_or(0),
        })
        .collect();
    samples.sort_unstable_by_key(|sample| sample.pid);
    samples
}

/// Run `wsl.exe --list --running` and return its stdout bytes. `None` on any
/// bounded-runner failure or a non-success exit (WSL absent, service down): the
/// caller then reports an unknown running set rather than guessing "stopped".
fn wsl_list_running_stdout() -> Option<Vec<u8>> {
    let mut command = Command::new(WSL_EXE);
    command.args(["--list", "--running"]);
    let output = run_with_timeout(&mut command, WSL_LIST_TIMEOUT).ok()?;
    if !output.status.success() {
        return None;
    }
    Some(output.stdout)
}

/// Run one fixed program inside a distribution through `wsl.exe --exec` (no
/// shell, no interpreter). The exit status is deliberately not surfaced:
/// `cat` exits non-zero when a member vanished mid-refresh while its already-
/// emitted lines remain valid, so callers judge by decoded content instead.
fn wsl_exec_stdout(distro: &str, program: &str, args: &[String]) -> Option<Vec<u8>> {
    let mut command = Command::new(WSL_EXE);
    command
        .arg("--distribution")
        .arg(distro)
        .arg("--exec")
        .arg(program)
        .args(args);
    let output = run_with_timeout(&mut command, WSL_SAMPLE_TIMEOUT).ok()?;
    Some(output.stdout)
}

/// Sample one running distribution's thread-leader processes. Returns `None`
/// when the channel failed outright (missing `ls`/`cat` in a minimal distro,
/// timeout, spawn failure); `truncated` flags the pid-cap binding.
fn sample_running_distro(distro: &str) -> Option<(Vec<WslProcSample>, bool)> {
    // Fixed-argv contract: a name that wsl.exe could parse as a flag is not
    // sampleable through this channel and is rejected up front.
    if distro.is_empty() || distro.starts_with('-') {
        return None;
    }
    let listing = wsl_exec_stdout(distro, "ls", &["-1".to_owned(), "/proc".to_owned()])?;
    let pids = numeric_proc_ids(&listing);
    // A running distribution always exposes at least its init process; an
    // empty listing means the channel broke, not a genuinely empty namespace.
    if pids.is_empty() {
        return None;
    }
    let truncated = pids.len() > WSL_SAMPLE_PID_CAP;
    let mut samples = Vec::new();
    for chunk in pids
        .iter()
        .take(WSL_SAMPLE_PID_CAP)
        .collect::<Vec<_>>()
        .chunks(WSL_CAT_CHUNK_PIDS)
    {
        let mut args: Vec<String> = Vec::with_capacity(chunk.len() * 2);
        for pid in chunk {
            args.push(format!("/proc/{pid}/stat"));
            args.push(format!("/proc/{pid}/status"));
        }
        let payload = wsl_exec_stdout(distro, "cat", &args)?;
        samples.extend(merge_proc_samples(&String::from_utf8_lossy(&payload)));
    }
    Some((samples, truncated))
}

/// Per-distribution CPU baselines. The aggregate jiffies sum is not monotonic
/// (exited members drop their counters), so a decrease saturates to zero
/// instead of the Linux cgroup tracker's identity-change reset.
#[derive(Debug, Default)]
struct WslCpuRateTracker {
    baselines: HashMap<String, (u64, u64)>,
}

impl WslCpuRateTracker {
    /// Derive a single-core-equivalent CPU% from summed thread-leader
    /// jiffies; the formula mirrors the Linux cgroup tracker's
    /// `delta / elapsed` shape in `USER_HZ` units.
    fn percentage(
        &mut self,
        distro: &str,
        cpu_jiffies: u64,
        now_ms: u64,
    ) -> ScalarObservation<f32> {
        let Some(&(prev_jiffies, prev_ms)) = self.baselines.get(distro) else {
            self.baselines
                .insert(distro.to_owned(), (cpu_jiffies, now_ms));
            return ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable);
        };
        let Some(elapsed_ms) = now_ms.checked_sub(prev_ms).filter(|elapsed| *elapsed > 0) else {
            self.baselines
                .insert(distro.to_owned(), (cpu_jiffies, now_ms));
            return ScalarObservation::unavailable(FailureKind::IdentityChanged);
        };
        self.baselines
            .insert(distro.to_owned(), (cpu_jiffies, now_ms));
        let delta = cpu_jiffies.saturating_sub(prev_jiffies);
        // pct = (delta / USER_HZ seconds) / (elapsed_ms / 1000 seconds) * 100
        let percentage = (delta as f64 * 100_000.0) / (WSL_USER_HZ * elapsed_ms as f64);
        if percentage.is_finite() && percentage >= 0.0 && percentage <= f32::MAX as f64 {
            ScalarObservation::available(percentage as f32, now_ms)
        } else {
            ScalarObservation::unavailable(FailureKind::ProviderFault)
        }
    }

    /// Drop baselines for distributions absent from the running set so a
    /// stopped-and-restarted distro re-establishes its baseline.
    fn retain(&mut self, running: &HashSet<String>) {
        self.baselines.retain(|name, _| running.contains(name));
    }

    fn clear(&mut self) {
        self.baselines.clear();
    }
}

/// Owns the per-distribution CPU baselines across rollup refreshes.
#[derive(Debug, Default)]
pub(super) struct WslRollupCollector {
    rates: WslCpuRateTracker,
}

impl WslRollupCollector {
    /// Build one [`ContainerSummary`] per registered distribution. Rows for
    /// stopped distributions (or an unknown running set) keep typed-unavailable
    /// metrics; running ones are sampled through the fixed-program channel.
    pub(super) fn rollup(&mut self, now_ms: u64) -> WslRollupOutcome {
        let distros = match taskmanager_windows_api::query_wsl_distributions() {
            Ok(distros) => distros,
            Err(_) => {
                self.rates.clear();
                return WslRollupOutcome {
                    containers: Vec::new(),
                    complete: true,
                };
            }
        };
        if distros.is_empty() {
            self.rates.clear();
            return WslRollupOutcome {
                containers: Vec::new(),
                complete: true,
            };
        }
        let registered: HashSet<&str> = distros.iter().map(|distro| distro.name.as_str()).collect();
        let running: Option<HashSet<String>> = wsl_list_running_stdout().map(|bytes| {
            running_names_from_list_text(&decode_wsl_console_text(&bytes), &registered)
                .into_iter()
                .collect()
        });
        if let Some(running) = running.as_ref() {
            self.rates.retain(running);
        }
        let mut complete = running.is_some();
        let mut containers = Vec::with_capacity(distros.len());
        for distro in distros {
            let is_running = running
                .as_ref()
                .is_some_and(|set| set.contains(&distro.name));
            let mut row_incomplete = false;
            let (cpu_percentage, memory_bytes, member_pids) = if is_running {
                match sample_running_distro(&distro.name) {
                    Some((samples, truncated)) => {
                        row_incomplete = truncated;
                        let cpu_jiffies = samples
                            .iter()
                            .fold(0u64, |sum, sample| sum.saturating_add(sample.cpu_jiffies));
                        let rss_bytes = samples
                            .iter()
                            .fold(0u64, |sum, sample| sum.saturating_add(sample.rss_bytes));
                        let cpu = self.rates.percentage(&distro.name, cpu_jiffies, now_ms);
                        (
                            cpu,
                            ScalarObservation::available(rss_bytes, now_ms),
                            sample_pids(&samples),
                        )
                    }
                    None => (
                        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                        Vec::new(),
                    ),
                }
            } else {
                (
                    ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                    ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                    Vec::new(),
                )
            };
            complete &= !row_incomplete;
            let runtime = if distro.name.to_lowercase().contains("docker") {
                IsolationKind::Docker
            } else {
                IsolationKind::Wsl
            };
            let id = format!("wsl:{}", distro.name);
            let cgroup_path = format!("wsl://{}", distro.name);
            containers.push(ContainerSummary {
                id,
                name: distro.name,
                runtime: Some(runtime),
                cgroup_path,
                cpu_percentage,
                memory_bytes,
                member_pids,
            });
        }
        // Descending CPU%: a distro with a current reading ranks above one
        // whose CPU% is still a typed gap (None sorts as -1.0); ties break on
        // name for a stable order — the Linux rollup's established convention.
        containers.sort_by(|left, right| {
            cpu_sort_key(right)
                .total_cmp(&cpu_sort_key(left))
                .then_with(|| left.name.cmp(&right.name))
        });
        WslRollupOutcome {
            containers,
            complete,
        }
    }
}

/// Sort key for the descending-CPU% ordering; an unavailable reading maps to
/// -1.0 (CPU% is never negative) so real readings always outrank typed gaps.
fn cpu_sort_key(container: &ContainerSummary) -> f64 {
    container
        .cpu_percentage
        .current_value()
        .map(|value| f64::from(*value))
        .unwrap_or(-1.0)
}

fn sample_pids(samples: &[WslProcSample]) -> Vec<u32> {
    samples.iter().map(|sample| sample.pid).collect()
}

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_system_wsl.rs"]
mod tests;
