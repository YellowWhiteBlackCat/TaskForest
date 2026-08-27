//! Typed availability for independently fallible process-row scalars.

use serde::{Deserialize, Serialize};

use crate::core::{FailureKind, ScalarObservation};

use super::ProcessItem;

/// Cohesive typed observations behind the legacy numeric process-row fields.
///
/// The group is platform neutral. Native adapters may obtain several values
/// from one source, but consumers only see current/stale/unavailable semantics
/// and shared failure kinds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ProcessScalarObservations {
    /// Provider-native identity token, such as Linux start-time ticks.
    pub start_token: ScalarObservation<u64>,
    pub cpu_percentage: ScalarObservation<f32>,
    pub memory_bytes: ScalarObservation<u64>,
    /// Hybrid proportional-set-size estimate. This is deliberately separate
    /// from `memory_bytes`, whose legacy meaning remains resident set size.
    #[serde(default)]
    pub memory_pss_bytes: ScalarObservation<u64>,
    /// Swap charged to this process. It is not part of either RSS or PSS.
    #[serde(default)]
    pub swap_bytes: ScalarObservation<u64>,
    pub disk_read_bytes_total: ScalarObservation<u64>,
    pub disk_write_bytes_total: ScalarObservation<u64>,
    pub disk_read_bytes_per_sec: ScalarObservation<u64>,
    pub disk_write_bytes_per_sec: ScalarObservation<u64>,
    pub threads: ScalarObservation<u32>,
    pub start_time_secs: ScalarObservation<u64>,
    pub cpu_time_secs: ScalarObservation<u64>,
    pub fds: ScalarObservation<u32>,
    pub nice: ScalarObservation<i32>,
}

impl ProcessScalarObservations {
    /// Mark retained row values stale when the whole process inventory cannot
    /// be refreshed. Current-value projections deliberately become absent.
    #[must_use]
    pub fn transition_failure(self, failure: FailureKind) -> Self {
        Self {
            start_token: self.start_token.transition_failure(failure),
            cpu_percentage: self.cpu_percentage.transition_failure(failure),
            memory_bytes: self.memory_bytes.transition_failure(failure),
            memory_pss_bytes: self.memory_pss_bytes.transition_failure(failure),
            swap_bytes: self.swap_bytes.transition_failure(failure),
            disk_read_bytes_total: self.disk_read_bytes_total.transition_failure(failure),
            disk_write_bytes_total: self.disk_write_bytes_total.transition_failure(failure),
            disk_read_bytes_per_sec: self.disk_read_bytes_per_sec.transition_failure(failure),
            disk_write_bytes_per_sec: self.disk_write_bytes_per_sec.transition_failure(failure),
            threads: self.threads.transition_failure(failure),
            start_time_secs: self.start_time_secs.transition_failure(failure),
            cpu_time_secs: self.cpu_time_secs.transition_failure(failure),
            fds: self.fds.transition_failure(failure),
            nice: self.nice.transition_failure(failure),
        }
    }

    /// Retain independently failed values only after the caller has proven the
    /// exact same provider-native process identity.
    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        Self {
            start_token: self.start_token.retain_previous(previous.start_token),
            cpu_percentage: self.cpu_percentage.retain_previous(previous.cpu_percentage),
            memory_bytes: self.memory_bytes.retain_previous(previous.memory_bytes),
            memory_pss_bytes: self
                .memory_pss_bytes
                .retain_previous(previous.memory_pss_bytes),
            swap_bytes: self.swap_bytes.retain_previous(previous.swap_bytes),
            disk_read_bytes_total: self
                .disk_read_bytes_total
                .retain_previous(previous.disk_read_bytes_total),
            disk_write_bytes_total: self
                .disk_write_bytes_total
                .retain_previous(previous.disk_write_bytes_total),
            disk_read_bytes_per_sec: self
                .disk_read_bytes_per_sec
                .retain_previous(previous.disk_read_bytes_per_sec),
            disk_write_bytes_per_sec: self
                .disk_write_bytes_per_sec
                .retain_previous(previous.disk_write_bytes_per_sec),
            threads: self.threads.retain_previous(previous.threads),
            start_time_secs: self
                .start_time_secs
                .retain_previous(previous.start_time_secs),
            cpu_time_secs: self.cpu_time_secs.retain_previous(previous.cpu_time_secs),
            fds: self.fds.retain_previous(previous.fds),
            nice: self.nice.retain_previous(previous.nice),
        }
    }
}

impl ProcessItem {
    /// Replace the sole typed process-row scalar authority.
    pub fn apply_scalar_observations(&mut self, observations: ProcessScalarObservations) {
        self.scalar_observations = observations;
    }

    #[must_use]
    pub fn with_scalar_observations(mut self, observations: ProcessScalarObservations) -> Self {
        self.apply_scalar_observations(observations);
        self
    }

    #[must_use]
    pub const fn scalar_observations(&self) -> &ProcessScalarObservations {
        &self.scalar_observations
    }

    /// Exact provider-native identity; unlike the legacy wall-clock start time,
    /// this never uses a compatibility fallback.
    #[must_use]
    pub const fn current_start_token(&self) -> Option<u64> {
        self.scalar_observations
            .start_token
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_cpu_percentage(&self) -> Option<f32> {
        self.scalar_observations
            .cpu_percentage
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_memory_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .memory_bytes
            .current_value()
            .copied()
    }

    /// Current hybrid PSS, with no compatibility fallback to RSS. Older
    /// snapshots and failed `/proc/<pid>/maps` reads therefore remain visibly
    /// unavailable to consumers instead of silently changing measurement kind.
    #[must_use]
    pub const fn current_memory_pss_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .memory_pss_bytes
            .current_value()
            .copied()
    }

    /// Current per-process swap. Swap is a separate resource and must not be
    /// folded into the Apps memory value.
    #[must_use]
    pub const fn current_swap_bytes(&self) -> Option<u64> {
        self.scalar_observations.swap_bytes.current_value().copied()
    }

    #[must_use]
    pub const fn current_disk_read_bytes_total(&self) -> Option<u64> {
        self.scalar_observations
            .disk_read_bytes_total
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_disk_write_bytes_total(&self) -> Option<u64> {
        self.scalar_observations
            .disk_write_bytes_total
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_disk_read_bytes_per_sec(&self) -> Option<u64> {
        self.scalar_observations
            .disk_read_bytes_per_sec
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_disk_write_bytes_per_sec(&self) -> Option<u64> {
        self.scalar_observations
            .disk_write_bytes_per_sec
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_threads(&self) -> Option<u32> {
        self.scalar_observations.threads.current_value().copied()
    }

    #[must_use]
    pub const fn current_start_time_secs(&self) -> Option<u64> {
        self.scalar_observations
            .start_time_secs
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_cpu_time_secs(&self) -> Option<u64> {
        self.scalar_observations
            .cpu_time_secs
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_fds(&self) -> Option<u32> {
        self.scalar_observations.fds.current_value().copied()
    }

    #[must_use]
    pub const fn current_nice(&self) -> Option<i32> {
        self.scalar_observations.nice.current_value().copied()
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_scalars_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_scalars_scalar_gap_tests.rs"]
mod scalar_gap_tests;
