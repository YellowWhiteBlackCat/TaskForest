//! Canonical bounded directory-usage scanner shared by native adapters.
//!
//! Pure safe `std::fs` with no `du` shell-out (uncancellable, no per-directory
//! typed degradation), no `unsafe`, and no escalation. The scan is bounded
//! (depth plus counted entries), publishes bounded Top-N reports, never
//! follows symlinks (loop safety by construction: only `symlink_metadata` is
//! consulted, so a symlink is counted as an entry and its target subtree
//! never enters the size aggregate), maps unreadable directories to typed
//! per-directory `PermissionDenied`, and is cancellable between directories
//! via the control token. One `scan_chunk` call performs one bounded unit of
//! work and returns a `Scanning` snapshot until the session reaches a
//! terminal state (`Completed` / `Cancelled` / `Failed`).
//!
//! This is the single source of truth: native OS adapters (Linux ADR-019
//! route-C, macOS parity) delegate their [`DirectoryUsageProvider`] impl to
//! a [`DirectoryUsageScanner`] one-for-one instead of carrying their own
//! traversal. Behavior is byte-for-byte identical to the previous per-adapter
//! copies; the corrected test suite (source of truth: the macOS port) lives
//! here and exercises the scanner directly.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use taskmanager_core::{
    DirectoryScanBounds, DirectoryScanControl, DirectoryScanId, DirectoryScanSpec,
    DirectoryScanStatus, DirectoryScanTotals, DirectoryUsageEntry, DirectoryUsageSnapshot,
    FailureKind, ScalarObservation, report_entries,
};
use taskmanager_platform_contract::ProviderFailure;

use taskmanager_platform_provider::DirectoryUsageProvider;

/// Entries examined per chunk. Combined with the chunk time budget it keeps
/// one provider call bounded and the cancel latency below ~100 ms.
const CHUNK_ENTRY_BUDGET: u64 = 512;
/// Wall-clock budget per chunk; the entry budget always applies too.
const CHUNK_TIME_BUDGET: Duration = Duration::from_millis(100);

/// Canonical directory-usage scanner: a chunked, cancellable, bounded,
/// pure safe-`std::fs` traversal that native adapters reuse via their
/// [`DirectoryUsageProvider`] impl. The session is kept alive across
/// `scan_chunk` calls while scanning and dropped on any terminal state.
///
/// An adapter provider delegates with a one-line forward, e.g.
///
/// ```ignore
/// impl DirectoryUsageProvider for NativeDirectoryUsageProvider {
///     fn scan_chunk(
///         &mut self,
///         spec: &DirectoryScanSpec,
///         control: &DirectoryScanControl,
///         observed_at_ms: u64,
///     ) -> Result<DirectoryUsageSnapshot, ProviderFailure> {
///         self.0.scan_chunk(spec, control, observed_at_ms)
///     }
/// }
/// ```
pub struct DirectoryUsageScanner {
    session: Option<ScanSession>,
}

impl DirectoryUsageScanner {
    #[must_use]
    pub const fn new() -> Self {
        Self { session: None }
    }
}

impl Default for DirectoryUsageScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryUsageProvider for DirectoryUsageScanner {
    fn scan_chunk(
        &mut self,
        spec: &DirectoryScanSpec,
        control: &DirectoryScanControl,
        observed_at_ms: u64,
    ) -> Result<DirectoryUsageSnapshot, ProviderFailure> {
        let mut session = match self.session.take() {
            Some(session) if session.spec == *spec => session,
            _ => ScanSession::start(spec, control.scan_id(), observed_at_ms),
        };
        let terminal = session.chunk(spec.bounds, control);
        let snapshot = session.snapshot(terminal, observed_at_ms);
        // Keep the session alive while scanning; drop it on terminal states.
        if !snapshot.is_terminal() {
            self.session = Some(session);
        }
        Ok(snapshot)
    }
}

struct ScanSession {
    spec: DirectoryScanSpec,
    scan_id: DirectoryScanId,
    /// Directories still to visit (absolute path + display path + depth).
    pending: VecDeque<PendingDir>,
    /// The current directory iterator is retained across chunks so a single
    /// high-fanout directory cannot monopolize one provider call.
    active: Option<ActiveDir>,
    /// Aggregate per-directory entries keyed by display path ("" = root).
    entries: HashMap<String, DirectoryUsageEntry>,
    totals: DirectoryScanTotals,
    /// Root plus every successfully enumerated `DirEntry`, including links
    /// and special files that do not contribute bytes to typed totals.
    admitted_entries: u64,
    /// The bounds stopped the scan before exhausting the tree.
    capped: bool,
    /// The pending stack was exhausted (or a terminal state was reached).
    exhausted: bool,
    /// The scan root itself could not be listed (typed, terminal).
    root_failure: Option<FailureKind>,
}

struct PendingDir {
    absolute: PathBuf,
    relative: String,
    depth: u32,
}

struct ActiveDir {
    dir: PendingDir,
    entries: fs::ReadDir,
}

impl ScanSession {
    fn start(spec: &DirectoryScanSpec, scan_id: DirectoryScanId, observed_at_ms: u64) -> Self {
        let mut session = Self {
            spec: spec.clone(),
            scan_id,
            pending: VecDeque::new(),
            active: None,
            entries: HashMap::new(),
            totals: DirectoryScanTotals::fresh(observed_at_ms),
            admitted_entries: 1,
            capped: false,
            exhausted: false,
            root_failure: None,
        };
        session
            .entries
            .insert(String::new(), DirectoryUsageEntry::root(observed_at_ms));
        let _root_recorded = session.totals.record_directory(u64::MAX);
        session.pending.push_back(PendingDir {
            absolute: PathBuf::from(&spec.root),
            relative: String::new(),
            depth: 0,
        });
        session
    }

    /// Run one bounded unit of work. `None` = still scanning; `Some(status)`
    /// = the terminal state to publish.
    fn chunk(
        &mut self,
        bounds: DirectoryScanBounds,
        control: &DirectoryScanControl,
    ) -> Option<DirectoryScanStatus> {
        let bounds = bounds.hardened();
        let started_at = Instant::now();
        let mut examined = 0_u64;
        let mut cancelled = false;
        let mut capped_now = false;

        while !cancelled && !self.exhausted && !capped_now {
            if control.is_cancelled() {
                cancelled = true;
                break;
            }
            if examined >= CHUNK_ENTRY_BUDGET || started_at.elapsed() >= CHUNK_TIME_BUDGET {
                break;
            }
            if self.active.is_none() {
                let Some(dir) = self.pending.pop_front() else {
                    self.exhausted = true;
                    break;
                };
                examined = examined.saturating_add(1);
                self.open_directory(dir);
                continue;
            }

            let next = self.active.as_mut().and_then(|active| {
                active
                    .entries
                    .next()
                    .map(|entry| (entry, active.dir.relative.clone(), active.dir.depth))
            });
            let Some((entry, parent_relative, parent_depth)) = next else {
                self.active = None;
                continue;
            };
            examined = examined.saturating_add(1);
            if self.process_directory_entry(entry, &parent_relative, parent_depth, bounds) {
                capped_now = true;
            }
        }

        if cancelled {
            self.exhausted = true;
            Some(DirectoryScanStatus::Cancelled)
        } else if let Some(failure) = self.root_failure {
            self.exhausted = true;
            Some(DirectoryScanStatus::Failed(failure))
        } else if capped_now || self.exhausted {
            if capped_now {
                self.capped = true;
            }
            self.exhausted = true;
            self.totals.capped = self.capped;
            Some(DirectoryScanStatus::Completed)
        } else {
            None
        }
    }

    /// Build the bounded publication for the current session state.
    fn snapshot(
        &self,
        terminal: Option<DirectoryScanStatus>,
        _observed_at_ms: u64,
    ) -> DirectoryUsageSnapshot {
        let status = terminal.unwrap_or(DirectoryScanStatus::Scanning);
        let entries = report_entries(
            self.entries.values().cloned().collect(),
            self.spec.bounds.max_reported,
        );
        DirectoryUsageSnapshot {
            scan_id: self.scan_id,
            root: self.spec.root.clone(),
            status,
            entries,
            totals: self.totals,
        }
    }

    fn open_directory(&mut self, dir: PendingDir) {
        if dir.depth > self.totals.depth_reached {
            self.totals.depth_reached = dir.depth;
        }
        match fs::read_dir(&dir.absolute) {
            Ok(entries) => {
                self.active = Some(ActiveDir { dir, entries });
            }
            Err(error) => {
                let failure = io_failure_kind(error.kind());
                self.totals.record_unreadable(failure);
                self.mark_entry_unreadable(&dir.relative, failure);
                if dir.relative.is_empty() {
                    // The scan root itself cannot be listed: an honest typed
                    // terminal failure, not an "empty tree".
                    self.root_failure = Some(failure);
                }
            }
        }
    }

    /// Consume exactly one item from the retained `ReadDir` cursor. Returns
    /// `true` only when admitting this item would cross the global entry cap.
    fn process_directory_entry(
        &mut self,
        entry: Result<fs::DirEntry, std::io::Error>,
        parent_relative: &str,
        parent_depth: u32,
        bounds: DirectoryScanBounds,
    ) -> bool {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                // A dangling entry raced away mid-listing: the directory
                // stays readable; nothing is counted for it.
                self.totals.record_partial(io_failure_kind(error.kind()));
                return false;
            }
        };
        if self.admitted_entries >= bounds.max_entries {
            return true;
        }
        self.admitted_entries = self.admitted_entries.saturating_add(1);
        let child_path = entry.path();
        let file_name = entry.file_name();
        let Some(child_name) = file_name.to_str() else {
            self.totals.record_partial(FailureKind::Unsupported);
            return false;
        };
        let metadata = match fs::symlink_metadata(&child_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
            Err(error) => {
                self.totals.record_partial(io_failure_kind(error.kind()));
                return false;
            }
        };
        let file_type = metadata.file_type();
        if file_type.is_file() {
            let _recorded = self.totals.record_file(metadata.len(), u64::MAX);
            self.record_subtree_file(parent_relative, metadata.len());
        } else if file_type.is_dir() {
            let depth = parent_depth.saturating_add(1);
            if depth > bounds.max_depth {
                return false;
            }
            // The successful DirEntry already consumed the shared entry
            // budget before metadata interpretation, so this typed directory
            // total must not charge the same item a second time.
            let _recorded = self.totals.record_directory(u64::MAX);
            let relative = join_display(parent_relative, child_name);
            self.entries.insert(
                relative.clone(),
                DirectoryUsageEntry {
                    path: relative.clone(),
                    depth,
                    size_bytes: ScalarObservation::available(0_u64, self.totals.observed_at()),
                    file_count: ScalarObservation::available(0_u64, self.totals.observed_at()),
                    unreadable: None,
                },
            );
            self.pending.push_back(PendingDir {
                absolute: child_path,
                relative,
                depth,
            });
        }
        // Symlinks, sockets, and special files contribute no bytes and are
        // never followed.
        false
    }

    /// Add one counted file's size to every ancestor entry along the path
    /// (root included), keeping the subtree aggregates complete.
    fn record_subtree_file(&mut self, dir_relative: &str, size: u64) {
        let observed_at_ms = self.totals.observed_at();
        let mut current = dir_relative.to_string();
        loop {
            if let Some(entry) = self.entries.get_mut(&current) {
                if let Some(sum) = entry.size_bytes.current_value().copied() {
                    entry.size_bytes =
                        ScalarObservation::available(sum.saturating_add(size), observed_at_ms);
                }
                if let Some(files) = entry.file_count.current_value().copied() {
                    entry.file_count = ScalarObservation::available(files + 1, observed_at_ms);
                }
            }
            if current.is_empty() {
                break;
            }
            current = parent_display(&current).to_string();
        }
    }

    fn mark_entry_unreadable(&mut self, relative: &str, failure: FailureKind) {
        let Some(entry) = self.entries.get_mut(relative) else {
            return;
        };
        entry.unreadable = Some(failure);
        entry.size_bytes = ScalarObservation::unavailable(failure);
        entry.file_count = ScalarObservation::unavailable(failure);
    }
}

/// Display-path parent: `"a/b"` -> `"a"`, `"a"` -> `""`, root stays `""`.
fn parent_display(relative: &str) -> &str {
    match relative.rfind('/') {
        Some(index) => &relative[..index],
        None => "",
    }
}

/// Join one display path component with `/` (root contributes nothing).
fn join_display(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// Map one `std::io` error kind to the typed failure vocabulary. Permission
/// problems are per-directory `PermissionDenied`; anything else the scanner
/// cannot attribute stays `TemporarilyUnavailable` -- never fabricated as
/// `PermissionDenied` and never a silent zero.
const fn io_failure_kind(kind: std::io::ErrorKind) -> FailureKind {
    match kind {
        std::io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        _ => FailureKind::TemporarilyUnavailable,
    }
}

#[cfg(test)]
#[path = "../tests/headless/directory_usage.rs"]
mod tests;
