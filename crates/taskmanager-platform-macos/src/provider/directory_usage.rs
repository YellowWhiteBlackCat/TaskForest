//! Directory-usage scan provider (Wave-A #1 macOS parity).
//!
//! Thin delegate over the canonical shared scanner
//! `taskmanager_platform_portable::DirectoryUsageScanner`: the chunked,
//! bounded, cancellable, pure safe-`std::fs` traversal (ADR-019 route-C) is
//! implemented exactly once in the shared crate and reused 1:1 here. No `du`
//! shell-out (uncancellable, no per-directory typed degradation), no
//! `unsafe`, no escalation. One `scan_chunk` call performs one bounded unit
//! of work and returns a `Scanning` snapshot until the session reaches a
//! terminal state. APFS firmlinks, clones, and high-density symlink trees
//! are not followed by construction: only `symlink_metadata` is consulted,
//! so a symlink is counted as an entry and never enters the size aggregate,
//! which makes symlink loops impossible.
//!
//! On-box receipt for real APFS large-directory behavior is pending (no
//! mac host in this environment); the scanner is verified against fixture
//! trees on Linux CI inside the shared provider crate.

use taskmanager_core::{DirectoryScanControl, DirectoryScanSpec, DirectoryUsageSnapshot};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_portable::DirectoryUsageScanner;
use taskmanager_platform_provider::DirectoryUsageProvider;

/// macOS directory-usage provider: a thin newtype wrapper around the shared
/// [`DirectoryUsageScanner`]. Behavior is byte-for-byte identical to the
/// shared scanner — `scan_chunk` is a one-line forward — so the macOS
/// adapter carries no traversal logic of its own.
pub struct MacDirectoryUsageProvider(pub(crate) DirectoryUsageScanner);

impl MacDirectoryUsageProvider {
    #[must_use]
    pub const fn new() -> Self {
        Self(DirectoryUsageScanner::new())
    }
}

impl Default for MacDirectoryUsageProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryUsageProvider for MacDirectoryUsageProvider {
    fn scan_chunk(
        &mut self,
        spec: &DirectoryScanSpec,
        control: &DirectoryScanControl,
        observed_at_ms: u64,
    ) -> Result<DirectoryUsageSnapshot, ProviderFailure> {
        self.0.scan_chunk(spec, control, observed_at_ms)
    }
}
