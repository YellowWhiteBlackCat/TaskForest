//! Linux telemetry collector root: per-domain submodules and shared disk state.
//!
//! Declares the compute, disk, network, lifecycle, and host-domain collectors
//! and owns the cross-tick disk-stats and SMART provider state they share.

use taskmanager_core::DeviceId;
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuPerformancePolicy, DiskMetrics, MemoryCompositionObservations,
    MemoryCompressionObservations, MemoryMetrics, MemoryModuleObservations,
    MemoryOptionalObservations, MemoryScalarObservations, OptionalObservation, ScalarObservation,
    VirtualMemoryCommitObservations,
};

use crate::engine::smart;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;
use taskmanager_platform_contract::{FailureKind, ProviderId, SourceOutcome, SourceStatus};

mod compute;
mod disks;
pub(crate) mod domains;
mod lifecycle;
mod network;
mod sources;
// Gated hop for the zram mm_stat parser seam (fuzz workspace reachability).
#[cfg(feature = "test-support")]
pub use compute::parse_zram_mm_stat;
use sources::*;

#[derive(Default)]
struct DiskStatsState {
    reads_completed: u64,
    sectors_read: u64,
    writes_completed: u64,
    sectors_written: u64,
    io_time_ms: u64,
    /// `/proc/diskstats` field 14 — weighted time spent doing I/Os (ms). This
    /// is the sum over each interval of (in-flight I/Os × interval), so it
    /// captures queueing + service and is the correct denominator for average
    /// response time per I/O. (`io_time_ms` is field 13 — busy/wall time, which
    /// is capped by the wall clock and under-reports latency under concurrency
    /// by up to ~the queue depth; it is still the right value for busy-time
    /// utilization, i.e. `active_time_pct`.)
    weighted_time_ms: u64,
    timestamp: Option<Instant>,
}

struct DiskCollectionState {
    stats: HashMap<String, DiskStatsState>,
    smart_cache: HashMap<String, smart::DiskSmart>,
    smart_source_cache: HashMap<String, SourceStatus>,
    identity_cache: HashMap<String, DeviceId>,
    smart_providers: smart::SmartProviderRegistry,
}

impl DiskCollectionState {
    fn new() -> Self {
        Self {
            stats: HashMap::new(),
            smart_cache: HashMap::new(),
            smart_source_cache: HashMap::new(),
            identity_cache: HashMap::new(),
            smart_providers: smart::SmartProviderRegistry::standard(),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_engine_collector_tests.rs"]
mod tests;
