//! Per-process `nice` and thread-count facts from ONE bounded `ps` shell-out.
//!
//! sysinfo 0.39 exposes no per-process thread count and no POSIX `nice` on
//! macOS (`tasks()`/`thread_kind()` are Linux/Android-only; `Process` has no
//! `nice()` accessor on apple targets). Both scalars are instead read from a
//! single `ps -Ao pid,nice,thcount` invocation, run at most once per ~5 s
//! (priority and thread count are quasi-static; this keeps `ps` startup out of
//! the ~1 Hz refresh path — a per-process shell-out over 100–300 processes
//! would be far too slow). The cache mirrors the Windows
//! `WinProcessListProvider::fresh_priority_map` and the macOS
//! `MacNetworkTelemetryProvider::fresh_facts` patterns.
//!
//! Safety policy (ADR-019): route C only — a bounded `std::process::Command`
//! shell-out to `ps`. No hand-written FFI; no `unsafe`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use taskmanager_platform_portable::run_with_timeout;

/// Cache window: priority and thread count are quasi-static, so one `ps`
/// snapshot is reused across refreshes inside this window. Matches the Windows
/// priority-cache and macOS network-facts TTL.
const FACTS_TTL: Duration = Duration::from_secs(5);

/// Cached per-process facts keyed by PID. `nice` is the POSIX nice value
/// (-20..=20); `threads` is the Mach thread count. Each is `None` when `ps`
/// did not surface a usable value for that column (e.g. `thcount` absent, or a
/// value that did not parse), so the caller degrades that one scalar honestly
/// instead of fabricating a number. Mirrors the `WinProcessListProvider`
/// priority cache and the `MacNetworkTelemetryProvider` link/SSID cache.
pub(crate) struct ProcessFactsCache {
    map: HashMap<u32, (Option<i32>, Option<u32>)>,
    refreshed_at: Option<Instant>,
}

impl ProcessFactsCache {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
            refreshed_at: None,
        }
    }

    /// Return the cached PID -> (nice, threads) map, re-shelling `ps` only
    /// when the cache is older than ~5 s (or never populated). Mirrors
    /// `WinProcessListProvider::fresh_priority_map` and
    /// `MacNetworkTelemetryProvider::fresh_facts`. Returns the empty map when
    /// `ps` is absent (Linux CI), exited non-zero, or timed out, so every
    /// caller scalar degrades honestly to typed `Unsupported`.
    pub(crate) fn fresh(&mut self, now: Instant) -> &HashMap<u32, (Option<i32>, Option<u32>)> {
        let stale = self
            .refreshed_at
            .is_none_or(|at| now.duration_since(at) >= FACTS_TTL);
        if stale {
            self.map = ps_process_facts();
            self.refreshed_at = Some(now);
        }
        &self.map
    }

    /// Test-only constructor: seed the cache with a pre-built map and mark it
    /// fresh so `fresh()` never shells out. Lets the provider tests inject
    /// deterministic facts without depending on a real `ps`/host process
    /// table.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", allow(dead_code))]
    pub(crate) fn with_map(map: HashMap<u32, (Option<i32>, Option<u32>)>, at: Instant) -> Self {
        Self {
            map,
            refreshed_at: Some(at),
        }
    }
}

impl Default for ProcessFactsCache {
    fn default() -> Self {
        Self::new()
    }
}

/// `ps -Ao pid,nice,thcount` -> PID -> (nice, threads) map. Returns an empty
/// map when `ps` is absent (Linux CI), exited non-zero, or timed out, so every
/// caller scalar degrades honestly to typed `Unsupported`. Non-zero exit
/// (e.g. `ps` refused) and `Spawn(NotFound)` are all tolerated as "no facts".
fn ps_process_facts() -> HashMap<u32, (Option<i32>, Option<u32>)> {
    let mut command = std::process::Command::new("ps");
    command.args(["-Ao", "pid,nice,thcount"]);
    match run_with_timeout(&mut command, Duration::from_secs(2)) {
        Ok(output) if output.status.success() => {
            parse_ps_facts_excerpt(&String::from_utf8_lossy(&output.stdout))
        }
        // `ps` absent (Linux CI cross-build), non-zero exit, or timeout: no
        // facts are fabricated; the scalars stay typed Unsupported.
        _ => HashMap::new(),
    }
}

/// Header-driven parser for a `ps -Ao pid,nice,thcount` excerpt. Columns are
/// located by their header token (`PID` / `NICE` / `THCOUNT`, matched
/// case-insensitively) rather than by fixed character offsets, so column-width
/// drift between macOS releases cannot misalign a field. Pure & host-
/// independent.
///
/// Returns the empty map when:
///   - there is no header line (empty output), or
///   - the header carries no `PID` token (nothing to key on), or
///   - the header carries `PID` but neither `NICE` nor `THCOUNT` (no useful
///     facts to publish).
///
/// A data row whose PID does not parse, or whose nice/threads cell is absent
/// or non-numeric, contributes `None` for that field (and is skipped entirely
/// when both fields are `None`, so no useless PID-only rows are carried).
pub(crate) fn parse_ps_facts_excerpt(stdout: &str) -> HashMap<u32, (Option<i32>, Option<u32>)> {
    let mut out = HashMap::new();
    let mut lines = stdout.lines();
    let Some(header) = lines.next() else {
        return out;
    };
    let header = header.trim();
    if header.is_empty() {
        return out;
    }
    let columns: Vec<&str> = header.split_whitespace().collect();
    let Some(pid_idx) = column_index(&columns, "PID") else {
        return out;
    };
    let nice_idx = column_index(&columns, "NICE");
    let threads_idx = column_index(&columns, "THCOUNT");
    // Without NICE or THCOUNT there is nothing to publish; PID alone would
    // carry useless rows.
    if nice_idx.is_none() && threads_idx.is_none() {
        return out;
    }
    for line in lines {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(pid_str) = fields.get(pid_idx).copied() else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        let nice = nice_idx
            .and_then(|i| fields.get(i).copied())
            .and_then(|v| v.parse::<i32>().ok());
        let threads = threads_idx
            .and_then(|i| fields.get(i).copied())
            .and_then(|v| v.parse::<u32>().ok());
        if nice.is_none() && threads.is_none() {
            continue;
        }
        out.insert(pid, (nice, threads));
    }
    out
}

/// Case-insensitive header-token lookup (0-based column index).
fn column_index(columns: &[&str], name: &str) -> Option<usize> {
    columns
        .iter()
        .position(|token| token.eq_ignore_ascii_case(name))
}

#[cfg(test)]
#[path = "../../tests/headless/macos_provider_process_facts.rs"]
mod tests;
