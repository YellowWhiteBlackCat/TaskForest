//! Platform-neutral directory-usage analysis models.
//!
//! These types carry no OS paths, commands, `RawFd`, or provider selection.
//! Scans are bounded (depth + counted entries + reported Top-N), publish
//! bounded progress, and can be cancelled between provider chunks. Every
//! measured number distinguishes current / measured-zero / unavailable
//! through [`ScalarObservation`]; an unreadable subtree is never fabricated
//! into a zero.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::FailureKind;
use crate::core::metrics::ScalarObservation;

/// Cancellation token handed to a running scan. Providers poll
/// [`Self::is_cancelled`] between bounded units of work (directories, not
/// entries) and return a `Cancelled` snapshot promptly once set. Pure std
/// (an atomic flag behind an `Arc`), so it stays in the zero-dependency core
/// and is reachable from both the runtime lane and the native provider SPI.
pub struct DirectoryScanControl {
    scan_id: DirectoryScanId,
    cancelled: Arc<AtomicBool>,
}

impl DirectoryScanControl {
    #[must_use]
    pub fn new(scan_id: DirectoryScanId, cancelled: Arc<AtomicBool>) -> Self {
        Self { scan_id, cancelled }
    }

    #[must_use]
    pub fn scan_id(&self) -> DirectoryScanId {
        self.scan_id
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Monotonic identity of one scan session. Zero is the pre-scan sentinel.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct DirectoryScanId(u64);

impl DirectoryScanId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }
}

/// Bounded scan policy. Providers clamp hostile values instead of trusting
/// them, and every consumer renders the `capped` flag honestly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryScanBounds {
    /// Maximum directory nesting below the scan root.
    pub max_depth: u32,
    /// Maximum files + directories counted across the whole scan.
    pub max_entries: u64,
    /// Maximum entries published per progress/terminal snapshot.
    pub max_reported: usize,
}

pub const DEFAULT_DIRECTORY_SCAN_DEPTH: u32 = 6;
pub const DEFAULT_DIRECTORY_SCAN_ENTRIES: u64 = 100_000;
pub const DEFAULT_DIRECTORY_SCAN_REPORTED: usize = 50;

impl DirectoryScanBounds {
    /// Harden a requested policy: depth in `1..=MAX_DIRECTORY_SCAN_DEPTH`,
    /// counted entries in `1..=MAX_DIRECTORY_SCAN_ENTRIES`, reported entries
    /// in `1..=MAX_DIRECTORY_SCAN_REPORTED`.
    #[must_use]
    pub fn hardened(self) -> Self {
        Self {
            max_depth: self.max_depth.clamp(1, MAX_DIRECTORY_SCAN_DEPTH),
            max_entries: self.max_entries.clamp(1, MAX_DIRECTORY_SCAN_ENTRIES),
            max_reported: self.max_reported.clamp(1, MAX_DIRECTORY_SCAN_REPORTED),
        }
    }
}

impl Default for DirectoryScanBounds {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_DIRECTORY_SCAN_DEPTH,
            max_entries: DEFAULT_DIRECTORY_SCAN_ENTRIES,
            max_reported: DEFAULT_DIRECTORY_SCAN_REPORTED,
        }
    }
}

/// Absolute policy ceilings. The provider and the report projection both clamp
/// to these so a hostile or hand-edited request can never grow an unbounded
/// scan or an unbounded UI list.
pub const MAX_DIRECTORY_SCAN_DEPTH: u32 = 32;
pub const MAX_DIRECTORY_SCAN_ENTRIES: u64 = 2_000_000;
pub const MAX_DIRECTORY_SCAN_REPORTED: usize = 200;

/// One scan request: an opaque platform-neutral root display path plus the
/// bounded policy. Starting a scan for the same root again is the resume
/// path (a fresh scan supersedes the previous one).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryScanSpec {
    pub root: String,
    pub bounds: DirectoryScanBounds,
}

/// One measured directory subtree in the report.
///
/// `size_bytes`/`file_count` use typed availability: an empty-but-readable
/// directory publishes `Available(0)` (measured zero), an unreadable one
/// publishes `Unavailable(PermissionDenied)` — the two are never confused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryUsageEntry {
    /// Display path relative to the scan root, `/`-separated. The root itself
    /// uses the empty string.
    pub path: String,
    /// Nesting level below the scan root (root = 0).
    pub depth: u32,
    /// Bytes counted in this subtree (files only, symlinks never followed).
    pub size_bytes: ScalarObservation<u64>,
    /// Files counted in this subtree.
    pub file_count: ScalarObservation<u64>,
    /// Whether this exact directory could not be read (typed, directory-level).
    pub unreadable: Option<FailureKind>,
}

impl DirectoryUsageEntry {
    #[must_use]
    pub const fn root(observed_at_ms: u64) -> Self {
        Self {
            path: String::new(),
            depth: 0,
            size_bytes: ScalarObservation::available(0_u64, observed_at_ms),
            file_count: ScalarObservation::available(0_u64, observed_at_ms),
            unreadable: None,
        }
    }
}

/// Whether the scan is still running or reached a terminal state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryScanStatus {
    /// More chunks will follow.
    Scanning,
    /// The scan finished (possibly with the bounds capped).
    Completed,
    /// The user cancelled the scan; totals/entries are the partial prefix.
    Cancelled,
    /// The scan could not start or aborted on a typed failure.
    Failed(FailureKind),
}

/// Cumulative counters for one scan session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryScanTotals {
    /// Directories visited (successfully listed).
    pub directories_visited: u64,
    /// Files counted.
    pub files_counted: u64,
    /// Directories skipped because they could not be listed.
    pub unreadable_directories: u64,
    /// Bytes counted, typed: `Partial(failure)` once any directory was
    /// unreadable (the sum is real but incomplete), `Available` otherwise.
    /// Never fabricated: an unreadable subtree contributes nothing.
    pub bytes_counted: ScalarObservation<u64>,
    /// Deepest nesting reached (never exceeds `max_depth`).
    pub depth_reached: u32,
    /// True when the entry/depth bounds stopped the scan before exhaustion.
    pub capped: bool,
}

impl Default for DirectoryScanTotals {
    fn default() -> Self {
        Self {
            directories_visited: 0,
            files_counted: 0,
            unreadable_directories: 0,
            bytes_counted: ScalarObservation::available(0_u64, 0),
            depth_reached: 0,
            capped: false,
        }
    }
}

impl DirectoryScanTotals {
    #[must_use]
    pub fn fresh(observed_at_ms: u64) -> Self {
        Self {
            bytes_counted: ScalarObservation::available(0_u64, observed_at_ms),
            ..Self::default()
        }
    }

    /// Record one counted file. Fails (returns the entry) when the entry cap
    /// is already reached so the provider stops counting honestly.
    pub fn record_file(&mut self, size_bytes: u64, cap: u64) -> bool {
        if self.files_counted + self.directories_visited + 1 > cap {
            return false;
        }
        let next = match self.bytes_counted.current_value() {
            Some(sum) => sum.saturating_add(size_bytes),
            None => return false,
        };
        self.files_counted += 1;
        self.bytes_counted = ScalarObservation::available(next, self.observed_at());
        true
    }

    /// Record one visited directory. Fails when the entry cap is reached.
    pub fn record_directory(&mut self, cap: u64) -> bool {
        if self.files_counted + self.directories_visited + 1 > cap {
            return false;
        }
        self.directories_visited += 1;
        true
    }

    /// Record an unreadable directory. The byte sum stays real but is now
    /// typed `Partial(strongest_failure)` — never silently "complete".
    pub fn record_unreadable(&mut self, failure: FailureKind) {
        self.unreadable_directories += 1;
        self.record_partial(failure);
    }

    /// Mark the byte sum partial without claiming a whole directory was
    /// unreadable (e.g. one file inside a readable directory raced away).
    pub fn record_partial(&mut self, failure: FailureKind) {
        self.bytes_counted = match self.bytes_counted.current_value() {
            Some(sum) => ScalarObservation::partial(
                *sum,
                self.observed_at(),
                stronger_failure(self.bytes_counted.availability().failure(), failure),
            ),
            None => self.bytes_counted,
        };
    }

    #[must_use]
    pub fn observed_at(&self) -> u64 {
        self.bytes_counted.last_success_ms().unwrap_or(0)
    }
}

/// Strongest of two failures (mirrors the runtime health merge priorities).
#[must_use]
pub fn stronger_failure(left: Option<FailureKind>, right: FailureKind) -> FailureKind {
    match left {
        Some(left) => stronger_of(left, right),
        None => right,
    }
}

#[must_use]
pub const fn stronger_of(left: FailureKind, right: FailureKind) -> FailureKind {
    if failure_priority(right) > failure_priority(left) {
        right
    } else {
        left
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

/// One bounded publication for a scan: progress while `Scanning`, the final
/// result otherwise. `entries` is capped at `max_reported` (largest first).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryUsageSnapshot {
    pub scan_id: DirectoryScanId,
    /// Display path of the scan root (the partition mount point chosen by
    /// the UI; core treats it as an opaque platform-neutral string).
    pub root: String,
    pub status: DirectoryScanStatus,
    pub entries: Vec<DirectoryUsageEntry>,
    pub totals: DirectoryScanTotals,
}

impl DirectoryUsageSnapshot {
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        !matches!(self.status, DirectoryScanStatus::Scanning)
    }
}

/// Sort report entries largest-first with a stable path tiebreak, then cap at
/// `max_reported`. Pure and deterministic: the provider calls this once per
/// published chunk, the UI projection never re-sorts.
#[must_use]
pub fn report_entries(
    mut entries: Vec<DirectoryUsageEntry>,
    max_reported: usize,
) -> Vec<DirectoryUsageEntry> {
    entries.sort_by(|left, right| {
        right
            .size_bytes
            .current_value()
            .copied()
            .unwrap_or(0)
            .cmp(&left.size_bytes.current_value().copied().unwrap_or(0))
            .then_with(|| left.path.cmp(&right.path))
    });
    entries.truncate(max_reported);
    entries
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_directory_usage_tests.rs"]
mod tests;
