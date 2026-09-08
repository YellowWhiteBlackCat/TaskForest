//! Bounded macOS per-process network-rate facts.
//!
//! macOS ships `nettop` as the user-facing process network accounting source.
//! The cache runs one `-P -L 1 -d` sample at most every five seconds, parses
//! the selected CSV columns by header name, and leaves a process scalar
//! unavailable when the command, column, or row is missing. No per-process
//! command loop and no `unsafe` libproc shim are introduced.

use std::collections::HashMap;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use taskmanager_platform_portable::run_with_timeout;

const FACTS_TTL: Duration = Duration::from_secs(5);
#[cfg(target_os = "macos")]
const NETTOP_TIMEOUT: Duration = Duration::from_secs(2);

/// Cached `(receive bytes/s, send bytes/s)` values keyed by PID.
pub(crate) struct ProcessNetworkFactsCache {
    map: HashMap<u32, (Option<u64>, Option<u64>)>,
    refreshed_at: Option<Instant>,
}

impl ProcessNetworkFactsCache {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
            refreshed_at: None,
        }
    }

    pub(crate) fn fresh(&mut self, now: Instant) -> &HashMap<u32, (Option<u64>, Option<u64>)> {
        let stale = self
            .refreshed_at
            .is_none_or(|at| now.duration_since(at) >= FACTS_TTL);
        if stale {
            self.map = collect_nettop_process_rates();
            self.refreshed_at = Some(now);
        }
        &self.map
    }

    /// Test/support seam: seed a known-good cache without running a host
    /// command. The production provider still owns the only native collector.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", allow(dead_code))]
    pub(crate) fn with_map(map: HashMap<u32, (Option<u64>, Option<u64>)>, at: Instant) -> Self {
        Self {
            map,
            refreshed_at: Some(at),
        }
    }
}

impl Default for ProcessNetworkFactsCache {
    fn default() -> Self {
        Self::new()
    }
}

fn collect_nettop_process_rates() -> HashMap<u32, (Option<u64>, Option<u64>)> {
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("nettop");
        command.args([
            "-n",
            "-P",
            "-L",
            "1",
            "-d",
            "-x",
            "-J",
            "pid,bytes_in,bytes_out",
        ]);
        return match run_with_timeout(&mut command, NETTOP_TIMEOUT) {
            Ok(output) if output.status.success() => {
                parse_nettop_csv(&String::from_utf8_lossy(&output.stdout))
            }
            _ => HashMap::new(),
        };
    }
    #[cfg(not(target_os = "macos"))]
    {
        HashMap::new()
    }
}

/// Parse `nettop -L` CSV output using its header instead of relying on a
/// release-specific column order. Quoted fields are accepted; malformed rows
/// are skipped and never become a zero-rate process.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_nettop_csv(output: &str) -> HashMap<u32, (Option<u64>, Option<u64>)> {
    let mut lines = output.lines().filter(|line| !line.trim().is_empty());
    let Some(header_line) = lines.next() else {
        return HashMap::new();
    };
    let header = csv_fields(header_line);
    let Some(pid_index) = header_index(&header, "pid") else {
        return HashMap::new();
    };
    let rx_index = header_index(&header, "bytes_in");
    let tx_index = header_index(&header, "bytes_out");
    if rx_index.is_none() && tx_index.is_none() {
        return HashMap::new();
    }

    let mut result = HashMap::new();
    for line in lines {
        let fields = csv_fields(line);
        let Some(pid) = fields.get(pid_index).and_then(|value| value.parse().ok()) else {
            continue;
        };
        let rx = rx_index
            .and_then(|index| fields.get(index))
            .and_then(|value| parse_byte_count(value));
        let tx = tx_index
            .and_then(|index| fields.get(index))
            .and_then(|value| parse_byte_count(value));
        if rx.is_some() || tx.is_some() {
            result.insert(pid, (rx, tx));
        }
    }
    result
}

#[cfg(any(target_os = "macos", test))]
fn header_index(header: &[String], wanted: &str) -> Option<usize> {
    header
        .iter()
        .position(|value| value.trim().eq_ignore_ascii_case(wanted))
}

#[cfg(any(target_os = "macos", test))]
fn parse_byte_count(value: &str) -> Option<u64> {
    let value = value.trim().trim_matches('"');
    (!value.is_empty() && value != "-")
        .then(|| value.parse::<u64>().ok())
        .flatten()
}

#[cfg(any(target_os = "macos", test))]
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(std::mem::take(&mut current));
            }
            _ => current.push(character),
        }
    }
    fields.push(current);
    fields
}

#[cfg(test)]
#[path = "../../../tests/headless/macos_provider_process_network_facts.rs"]
mod tests;
