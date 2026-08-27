//! Best-effort hygiene for temporaries abandoned by dead writers.
//!
//! A killed writer leaves its `.tmp<pid>-<seq>` sibling behind forever; the
//! writer itself only removes it on a failed write or rename. Unchecked,
//! that debris accumulates against [`MAX_DIRECTORY_ENTRIES_PER_SCAN`] until
//! every flush fails typed and persistence stalls. Each flush sweeps before
//! its retention scan and deletes a temporary only when BOTH proofs hold:
//! the embedded writer pid is gone per the injected liveness probe (a live
//! writer may still rename the file into place), and the mtime is older
//! than [`STALE_TEMPORARY_AGE_MS`]. The sweep is fail-open — removal
//! failures are counted, never fatal — and structurally cannot touch the
//! lock, claim or data files because they do not carry the marker.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::MAX_DIRECTORY_ENTRIES_PER_SCAN;
use super::temporary_writer_pid;

/// Age beyond which a temporary whose writer pid is provably gone counts as
/// abandoned. A live writer's temporary spans one atomic write — create to
/// rename, milliseconds — so 24 hours is orders of magnitude beyond any
/// legitimate in-flight write; paired with the dead-pid proof it only
/// breaks ties for wedged systems and repeated crashes, never for a writer
/// that is still running.
pub const STALE_TEMPORARY_AGE_MS: u64 = 24 * 60 * 60 * 1000;

/// What one sweep pass removed and what it failed to remove. Both counters
/// saturate; failures never abort the pass or the flush around it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TemporarySweepReport {
    pub stale_removed: usize,
    pub removal_failures: usize,
}

/// One bounded, fail-open pass over `root`. At most
/// [`MAX_DIRECTORY_ENTRIES_PER_SCAN`] entries are examined, so extreme
/// debris recovers across consecutive flushes instead of monopolizing this
/// one.
pub(super) fn sweep_stale_temporaries(
    root: &Path,
    holder_is_gone: fn(u32) -> bool,
) -> TemporarySweepReport {
    let mut report = TemporarySweepReport::default();
    let Ok(entries) = fs::read_dir(root) else {
        // An unreadable root is the retention scan's typed failure to
        // report, not the sweep's.
        return report;
    };
    let mut entries_seen = 0usize;
    for entry in entries.flatten() {
        entries_seen = entries_seen.saturating_add(1);
        if entries_seen > MAX_DIRECTORY_ENTRIES_PER_SCAN {
            break;
        }
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let Some(pid) = entry.file_name().to_str().and_then(temporary_writer_pid) else {
            continue;
        };
        if !holder_is_gone(pid) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !modified_before_stale_threshold(&metadata) {
            continue;
        }
        match fs::remove_file(entry.path()) {
            Ok(()) => report.stale_removed = report.stale_removed.saturating_add(1),
            Err(_) => report.removal_failures = report.removal_failures.saturating_add(1),
        }
    }
    report
}

/// `false` for future-dated mtimes (a clock that stepped back) and for
/// unsupported mtime clocks: an unknown age is treated as fresh, never as
/// proof of abandonment.
fn modified_before_stale_threshold(metadata: &fs::Metadata) -> bool {
    metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= Duration::from_millis(STALE_TEMPORARY_AGE_MS))
}
