//! Typed process facts retained by toolkit-neutral semantic rows.

use taskmanager_core::core::ScalarObservation;
use taskmanager_core::core::process::aggregate::AggregateMetric;

/// Current and retained facts for the process scalar columns.
///
/// The observations are the core-owned values verbatim, including freshness,
/// failure reason, current/last-known value, and last-success time. They are
/// kept beside any localized row text so semantic consumers do not have to
/// infer availability from a formatted string.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessRowObservations {
    cpu_percentage: ScalarObservation<f32>,
    memory_bytes: ScalarObservation<u64>,
}

impl ProcessRowObservations {
    #[must_use]
    pub const fn new(
        cpu_percentage: ScalarObservation<f32>,
        memory_bytes: ScalarObservation<u64>,
    ) -> Self {
        Self {
            cpu_percentage,
            memory_bytes,
        }
    }

    #[must_use]
    pub const fn cpu_percentage(&self) -> &ScalarObservation<f32> {
        &self.cpu_percentage
    }

    #[must_use]
    pub const fn memory_bytes(&self) -> &ScalarObservation<u64> {
        &self.memory_bytes
    }
}

/// Typed availability and coverage for an application aggregate row.
///
/// These are the core-owned [`AggregateMetric`] values copied from the shell's
/// canonical aggregate projection. Keeping each value intact preserves
/// availability, member coverage, current and last-known values, and the
/// source observation time without reducing the contract to current values.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessAggregateSemantics {
    /// Aggregate CPU percentage.
    pub cpu: AggregateMetric<f32>,
    /// Aggregate resident-memory value used by the display policy.
    pub memory: AggregateMetric<u64>,
    /// Aggregate proportional-set-size value.
    pub memory_pss: AggregateMetric<u64>,
    /// Aggregate swap value.
    pub swap: AggregateMetric<u64>,
    /// Aggregate disk-read rate.
    pub disk_read: AggregateMetric<u64>,
    /// Aggregate disk-write rate.
    pub disk_write: AggregateMetric<u64>,
    /// Aggregate cumulative CPU time.
    pub cpu_time: AggregateMetric<u64>,
    /// Aggregate thread count.
    pub threads: AggregateMetric<u64>,
    /// Aggregate file-descriptor count.
    pub fds: AggregateMetric<u64>,
}

impl ProcessAggregateSemantics {
    #[must_use]
    pub const fn cpu(&self) -> &AggregateMetric<f32> {
        &self.cpu
    }

    #[must_use]
    pub const fn memory(&self) -> &AggregateMetric<u64> {
        &self.memory
    }

    #[must_use]
    pub const fn memory_pss(&self) -> &AggregateMetric<u64> {
        &self.memory_pss
    }

    #[must_use]
    pub const fn swap(&self) -> &AggregateMetric<u64> {
        &self.swap
    }

    #[must_use]
    pub const fn disk_read(&self) -> &AggregateMetric<u64> {
        &self.disk_read
    }

    #[must_use]
    pub const fn disk_write(&self) -> &AggregateMetric<u64> {
        &self.disk_write
    }

    #[must_use]
    pub const fn cpu_time(&self) -> &AggregateMetric<u64> {
        &self.cpu_time
    }

    #[must_use]
    pub const fn threads(&self) -> &AggregateMetric<u64> {
        &self.threads
    }

    #[must_use]
    pub const fn fds(&self) -> &AggregateMetric<u64> {
        &self.fds
    }
}
