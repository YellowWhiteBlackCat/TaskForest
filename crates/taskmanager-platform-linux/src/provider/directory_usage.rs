//! Directory-usage scan provider (Linux product path, ADR-019 route-C).
//!
//! Thin delegate over the canonical shared scanner
//! (`taskmanager_platform_portable::DirectoryUsageScanner`): the Linux
//! adapter carries no traversal logic of its own. The shared scanner is a
//! pure safe-`std::fs`, bounded (depth + counted entries), bounded Top-N
//! publishing, symlink-loop-safe (only `symlink_metadata` is consulted),
//! per-directory typed-`PermissionDenied` mapping, cancellable traversal;
//! one `scan_chunk` call performs one bounded unit of work. Behavior is
//! byte-for-byte identical to the previous per-adapter copy -- the single
//! source of truth (and its corrected test suite) lives in the shared crate.

use taskmanager_core::{DirectoryScanControl, DirectoryScanSpec, DirectoryUsageSnapshot};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_portable::DirectoryUsageScanner;
use taskmanager_platform_provider::DirectoryUsageProvider;

/// Linux native directory-usage provider: a one-line forwarder over the
/// shared [`DirectoryUsageScanner`]. Kept as a distinct type so the Linux
/// provider graph retains its concrete provider identity and registration.
pub(super) struct NativeDirectoryUsageProvider(DirectoryUsageScanner);

impl NativeDirectoryUsageProvider {
    pub(super) const fn new() -> Self {
        Self(DirectoryUsageScanner::new())
    }
}

impl Default for NativeDirectoryUsageProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryUsageProvider for NativeDirectoryUsageProvider {
    fn scan_chunk(
        &mut self,
        spec: &DirectoryScanSpec,
        control: &DirectoryScanControl,
        observed_at_ms: u64,
    ) -> Result<DirectoryUsageSnapshot, ProviderFailure> {
        self.0.scan_chunk(spec, control, observed_at_ms)
    }
}
