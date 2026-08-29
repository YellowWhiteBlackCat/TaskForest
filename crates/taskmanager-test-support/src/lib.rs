//! Dev-only typed behavior-fixture assembly.

#![forbid(unsafe_code)]

use taskmanager_core::{
    DiskMetrics, DiskPartition, DiskPartitionScalarObservations, DiskScalarObservations,
    MemoryMetrics, MemoryOptionalObservations, MemoryScalarObservations, NetworkAdapterType,
    NetworkMetrics, NetworkScalarObservations, NetworkWirelessObservations, OptionalObservation,
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataObservation,
    ProcessMetadataObservations, ProcessScalarObservations, ScalarObservation, SmartAvailability,
};

mod memory;
mod metrics;
mod process;

// ── locale pinning for text-asserting tests ──────────────────────────────────

/// Pin the process-global i18n language to English for the calling test.
///
/// `taskmanager_application::i18n::t` seeds its active language lazily from the
/// host environment (`LC_ALL` → `LC_MESSAGES` → `LANG`; `zh*` → `Zh`), so on a
/// zh_CN host every unpinned `t()` call resolves the Chinese catalog. Tests
/// that assert catalog-driven copy (footers, feedback lines, page titles) must
/// call this first so their assertions hold on every host locale — the repo
/// rule is that tests never depend on host-specific values. Product behavior
/// (follow the host language by default) is correct and stays untouched.
///
/// # Parallelism
///
/// The language is a process-level global shared by all tests in one binary.
/// `pin_english` is an atomic store, so concurrent callers are safe, but a test
/// that flips to another language mid-run (e.g. an En/Zh cycle asserting both
/// catalogs) must serialize itself against concurrent English-asserting tests
/// in the same binary — see the TUI render tests' `LANG_TEST_GUARD` mutex for
/// the established pattern. Language-flipping tests are rare; plain English
/// assertions just call this once at test start.
pub fn pin_english() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
}

/// Initial typestate for an optional whole observation group. Only this
/// stage exposes the group-level base setter.
#[doc(hidden)]
#[derive(Debug)]
pub struct GroupBaseOpen;

/// Typestate after a whole observation group or any named override was
/// applied. Group replacement is intentionally unavailable from this stage.
#[doc(hidden)]
#[derive(Debug)]
pub struct NamedOverrides;

pub use memory::MemoryMetricsFixtureBuilder;
pub use metrics::{
    DiskMetricsFixtureBuilder, DiskPartitionFixtureBuilder, NetworkMetricsFixtureBuilder,
};
pub use process::{
    ProcessItemFixtureBuilder, SortFixtureMetrics, category_fixture_with_empty_bucket,
    fixture_start_token, mixed_availability_category_fixture, sort_fixture_row,
    sort_parity_fixture,
};
