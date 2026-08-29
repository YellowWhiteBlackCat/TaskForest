//! Bounded Linux process-memory enrichment.
//!
//! Mission Center's Apps memory value is a hybrid PSS estimate: anonymous and
//! shared-memory RSS stay private, while file-backed RSS is divided by a
//! periodically sampled sharing factor. This module owns only Linux I/O and
//! the approximation. The typed scalar contract remains in `taskmanager-core`.

use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hash;
use std::io::Read;
use std::sync::Arc;

use taskmanager_core::{FailureKind, SourceOutcome};

use super::procfs::{
    ProcMemoryObservations, ProcStatusMemoryFields, read_proc_memory_observations, read_proc_stat,
};
use super::tree::io_failure;

const MAPS_REFRESH_INTERVAL_MS: u64 = 5_000;
const MAX_MAP_BYTES: u64 = 8 * 1024 * 1024;
/// A process list can be very large. A deterministic cap keeps one refresh
/// bounded; processes outside the cap remain typed-unavailable until a later
/// bounded sample rather than receiving a guessed memory value.
const MAX_MAP_PROCESSES: usize = 1_024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileMapping {
    /// Paths are interned across all sampled processes. `/proc/<pid>/maps`
    /// repeats the same shared libraries many times; an `Arc<str>` keeps one
    /// allocation for the path while retaining one size entry per process.
    path: Arc<str>,
    size_bytes: u64,
}

#[derive(Debug, Clone)]
struct CachedProcessMaps {
    start_token: Result<u64, FailureKind>,
    mappings: Result<Vec<FileMapping>, FailureKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcessMemoryObservation {
    pub(super) pss: Result<u64, FailureKind>,
    pub(super) swap: Result<u64, FailureKind>,
    pub(super) pss_outcome: SourceOutcome,
    pub(super) swap_outcome: SourceOutcome,
}

impl ProcessMemoryObservation {
    pub(super) const fn unavailable(failure: FailureKind) -> Self {
        Self {
            pss: Err(failure),
            swap: Err(failure),
            pss_outcome: SourceOutcome::Unavailable(failure),
            swap_outcome: SourceOutcome::Unavailable(failure),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct MemoryMaps {
    refreshed_at_ms: Option<u64>,
    processes: HashMap<u32, CachedProcessMaps>,
    share_count: HashMap<Arc<str>, u32>,
}

impl MemoryMaps {
    /// Refresh the bounded mapping/share sample at most once per five seconds.
    /// The caller supplies the current process inventory so this module never
    /// scans `/proc` recursively or spawns an unbounded helper.
    pub(super) fn refresh(&mut self, pids: &[u32], observed_at_ms: u64) {
        if !self
            .refreshed_at_ms
            .is_none_or(|last| observed_at_ms.saturating_sub(last) >= MAPS_REFRESH_INTERVAL_MS)
        {
            return;
        }

        let mut bounded_pids = pids.to_vec();
        bounded_pids.sort_unstable();
        bounded_pids.dedup();
        bounded_pids.truncate(MAX_MAP_PROCESSES);

        self.processes.clear();
        self.share_count.clear();
        for pid in bounded_pids {
            let start_token = read_proc_stat(pid).map(|stat| stat.start_ticks);
            let mappings = match start_token {
                Ok(_) => read_proc_maps(pid),
                Err(failure) => Err(failure),
            };
            self.processes.insert(
                pid,
                CachedProcessMaps {
                    start_token,
                    mappings,
                },
            );
        }

        for process in self.processes.values() {
            let Ok(mappings) = &process.mappings else {
                continue;
            };
            let mut unique_paths = HashSet::new();
            for mapping in mappings {
                if unique_paths.insert(&*mapping.path) {
                    let count = self
                        .share_count
                        .entry(Arc::clone(&mapping.path))
                        .or_default();
                    *count = count.saturating_add(1);
                }
            }
        }
        self.refreshed_at_ms = Some(observed_at_ms);
    }

    /// Read current status memory and project PSS/swap only after the cached
    /// mapping sample proves the same provider-native start token. Status and
    /// stat are read again here to narrow the PID-reuse window around the
    /// enrichment; a race is reported as `IdentityChanged`, never as zero.
    pub(super) fn observe(&self, pid: u32, expected_start_token: u64) -> ProcessMemoryObservation {
        let (pss, swap) = match read_proc_memory_observations(pid) {
            Ok(ProcMemoryObservations {
                pss_fields,
                swap_bytes,
            }) => {
                let pss =
                    pss_fields.and_then(|status| self.pss_for(pid, expected_start_token, status));
                (pss, swap_bytes)
            }
            Err(failure) => (Err(failure), Err(failure)),
        };
        ProcessMemoryObservation {
            pss_outcome: result_outcome(&pss),
            swap_outcome: result_outcome(&swap),
            pss,
            swap,
        }
    }

    fn pss_for(
        &self,
        pid: u32,
        expected_start_token: u64,
        status: ProcStatusMemoryFields,
    ) -> Result<u64, FailureKind> {
        let cached = self
            .processes
            .get(&pid)
            .ok_or(FailureKind::TemporarilyUnavailable)?;
        let cached_start_token = cached.start_token?;
        if cached_start_token != expected_start_token {
            return Err(FailureKind::IdentityChanged);
        }
        if read_proc_stat(pid)?.start_ticks != expected_start_token {
            return Err(FailureKind::IdentityChanged);
        }
        let mappings = cached.mappings.as_ref().map_err(|failure| *failure)?;
        hybrid_pss(status, mappings, &self.share_count)
    }
}

fn read_proc_maps(pid: u32) -> Result<Vec<FileMapping>, FailureKind> {
    let file = fs::File::open(format!("/proc/{pid}/maps")).map_err(|error| io_failure(&error))?;
    let mut limited = file.take(MAX_MAP_BYTES.saturating_add(1));
    let mut text = String::new();
    limited
        .read_to_string(&mut text)
        .map_err(|_| FailureKind::ProviderFault)?;
    let text_len = u64::try_from(text.len()).map_err(|_| FailureKind::ProviderFault)?;
    if text_len > MAX_MAP_BYTES {
        return Err(FailureKind::TemporarilyUnavailable);
    }
    parse_proc_maps(&text)
}

fn parse_proc_maps(text: &str) -> Result<Vec<FileMapping>, FailureKind> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_maps_line)
        .collect::<Result<Vec<_>, _>>()
        .map(|mappings| mappings.into_iter().flatten().collect())
}

fn parse_maps_line(line: &str) -> Result<Option<FileMapping>, FailureKind> {
    let mut fields = line.split_whitespace();
    let range = fields.next().ok_or(FailureKind::ProviderFault)?;
    let _permissions = fields.next().ok_or(FailureKind::ProviderFault)?;
    let _offset = fields.next().ok_or(FailureKind::ProviderFault)?;
    let _device = fields.next().ok_or(FailureKind::ProviderFault)?;
    let inode = fields.next().ok_or(FailureKind::ProviderFault)?;
    let path = fields.collect::<Vec<_>>().join(" ");

    let (start, end) = range.split_once('-').ok_or(FailureKind::ProviderFault)?;
    let start = u64::from_str_radix(start, 16).map_err(|_| FailureKind::ProviderFault)?;
    let end = u64::from_str_radix(end, 16).map_err(|_| FailureKind::ProviderFault)?;
    let size_bytes = end.checked_sub(start).ok_or(FailureKind::ProviderFault)?;
    let inode = inode
        .parse::<u64>()
        .map_err(|_| FailureKind::ProviderFault)?;

    // Anonymous/special pseudo mappings do not contribute to the file-backed
    // sharing estimate. `(deleted)` files are skipped because their path no
    // longer identifies a shareable backing object.
    if inode == 0 || path.is_empty() || path.starts_with('[') || path.ends_with(" (deleted)") {
        return Ok(None);
    }
    Ok(Some(FileMapping {
        path: Arc::from(path),
        size_bytes,
    }))
}

fn hybrid_pss<K>(
    status: ProcStatusMemoryFields,
    mappings: &[FileMapping],
    share_count: &HashMap<K, u32>,
) -> Result<u64, FailureKind>
where
    K: Borrow<str> + Eq + Hash,
{
    let private_bytes = status
        .rss_anon_bytes
        .checked_add(status.rss_shmem_bytes)
        .ok_or(FailureKind::ProviderFault)?;
    let reported_components = private_bytes
        .checked_add(status.rss_file_bytes)
        .ok_or(FailureKind::ProviderFault)?;
    if reported_components > status.rss_bytes {
        // `/proc/<pid>/status` was internally inconsistent, usually because
        // the process changed while the kernel rendered the file. Do not turn
        // that race into a believable PSS number.
        return Err(FailureKind::TemporarilyUnavailable);
    }
    if status.rss_file_bytes == 0 {
        return Ok(private_bytes);
    }
    if mappings.is_empty() {
        return Err(FailureKind::TemporarilyUnavailable);
    }

    let mut total_mapping_bytes = 0.0_f64;
    let mut weighted_inverse_share = 0.0_f64;
    for mapping in mappings {
        let share = share_count
            .get(mapping.path.as_ref())
            .copied()
            .filter(|share| *share > 0)
            .ok_or(FailureKind::ProviderFault)?;
        let size = mapping.size_bytes as f64;
        total_mapping_bytes += size;
        weighted_inverse_share += size / f64::from(share);
    }
    if !total_mapping_bytes.is_finite()
        || !weighted_inverse_share.is_finite()
        || total_mapping_bytes <= 0.0
        || weighted_inverse_share <= 0.0
    {
        return Err(FailureKind::ProviderFault);
    }

    let file_pss = status.rss_file_bytes as f64 * weighted_inverse_share / total_mapping_bytes;
    let total_pss = private_bytes as f64 + file_pss;
    if !total_pss.is_finite() || total_pss < 0.0 || total_pss > u64::MAX as f64 {
        return Err(FailureKind::ProviderFault);
    }
    // The range check above makes this conversion bounded and loss is limited
    // to the final sub-byte floating-point rounding of the approximation.
    Ok(total_pss.round() as u64)
}

fn result_outcome(result: &Result<u64, FailureKind>) -> SourceOutcome {
    match result {
        Ok(_) => SourceOutcome::Available,
        Err(failure) => SourceOutcome::Unavailable(*failure),
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_memory_maps_tests.rs"]
mod tests;
