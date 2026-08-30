//! Platform-neutral per-process resource-history rules.
//!
//! Native adapters provide a verified process identity, typed current sample,
//! and one monotonic timestamp per refresh. This module owns the trailing
//! window, identity replacement, hard capacity, synchronized projections, and
//! stale identity pruning.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use super::{ProcessItem, ProcessLiveKey};

const PROCESS_HISTORY_WINDOW: Duration = Duration::from_secs(60);
const PROCESS_HISTORY_MAX_SAMPLES: usize = 121;
const PROCESS_HISTORY_STALE_TICKS: u64 = 3;

/// Typed current resource values captured for one process refresh.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProcessHistorySample {
    cpu: Option<f32>,
    memory: Option<f32>,
    disk_read: Option<f32>,
    disk_write: Option<f32>,
}

impl ProcessHistorySample {
    /// Read only typed-current projections; unavailable fields remain gaps.
    #[must_use]
    pub fn from_process(process: &ProcessItem) -> Self {
        Self {
            cpu: process.current_cpu_percentage(),
            memory: process.current_memory_bytes().map(|value| value as f32),
            disk_read: process
                .current_disk_read_bytes_per_sec()
                .map(|value| value as f32),
            disk_write: process
                .current_disk_write_bytes_per_sec()
                .map(|value| value as f32),
        }
    }
}

/// History vectors stamped onto a process row, oldest first.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcessHistorySnapshot {
    pub cpu: Vec<f32>,
    pub memory: Vec<f32>,
    pub disk: Vec<f32>,
    pub disk_read: Vec<f32>,
    pub disk_write: Vec<f32>,
}

#[derive(Debug, Clone, Default)]
struct ProcessHistoryRing {
    samples: VecDeque<TimedProcessHistorySample>,
    last_seen: u64,
}

#[derive(Debug, Clone, Copy)]
struct TimedProcessHistorySample {
    value: ProcessHistorySample,
    observed_at: Duration,
}

impl ProcessHistoryRing {
    fn push(&mut self, sample: ProcessHistorySample, tick: u64, observed_at: Duration) {
        while self.samples.front().is_some_and(|oldest| {
            observed_at.saturating_sub(oldest.observed_at) > PROCESS_HISTORY_WINDOW
        }) {
            self.samples.pop_front();
        }
        if self.samples.len() >= PROCESS_HISTORY_MAX_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(TimedProcessHistorySample {
            value: sample,
            observed_at,
        });
        self.last_seen = tick;
    }

    fn snapshot(&self) -> ProcessHistorySnapshot {
        ProcessHistorySnapshot {
            cpu: self
                .samples
                .iter()
                .map(|sample| sample.value.cpu.unwrap_or(f32::NAN))
                .collect(),
            memory: self
                .samples
                .iter()
                .map(|sample| sample.value.memory.unwrap_or(f32::NAN))
                .collect(),
            disk: self
                .samples
                .iter()
                .map(|sample| {
                    sample
                        .value
                        .disk_read
                        .zip(sample.value.disk_write)
                        .map_or(f32::NAN, |(read, write)| read + write)
                })
                .collect(),
            disk_read: self
                .samples
                .iter()
                .map(|sample| sample.value.disk_read.unwrap_or(f32::NAN))
                .collect(),
            disk_write: self
                .samples
                .iter()
                .map(|sample| sample.value.disk_write.unwrap_or(f32::NAN))
                .collect(),
        }
    }
}

/// Bounded histories keyed by the complete provider-issued live identity.
#[derive(Debug, Clone, Default)]
pub struct ProcessHistoryStore {
    rings: HashMap<ProcessLiveKey, ProcessHistoryRing>,
    tick: u64,
    observed_at: Duration,
}

impl ProcessHistoryStore {
    /// Advance once per provider refresh using a caller-supplied monotonic age.
    pub fn begin_refresh(&mut self, observed_at: Duration) {
        self.tick = self.tick.wrapping_add(1);
        self.observed_at = observed_at;
    }

    /// Append the current sample under its exact live identity. A PID reuse
    /// produces a different key and therefore starts a new trajectory without
    /// any secondary compatibility check.
    pub fn record(
        &mut self,
        identity: ProcessLiveKey,
        sample: ProcessHistorySample,
    ) -> ProcessHistorySnapshot {
        let ring = self.rings.entry(identity).or_default();
        ring.push(sample, self.tick, self.observed_at);
        ring.snapshot()
    }

    /// Materialize one retained process history on demand.
    ///
    /// Native adapters normally call [`Self::record`] and immediately receive
    /// the current projection. This read path is used when a provider must
    /// materialize a retained row after a refresh failure.
    #[must_use]
    pub fn snapshot_for(&self, identity: ProcessLiveKey) -> ProcessHistorySnapshot {
        self.rings.get(&identity).map_or_else(
            ProcessHistorySnapshot::default,
            ProcessHistoryRing::snapshot,
        )
    }

    /// Drop rings that remained absent beyond the fixed refresh grace.
    pub fn finish_refresh(&mut self) {
        let tick = self.tick;
        self.rings
            .retain(|_, ring| tick.saturating_sub(ring.last_seen) <= PROCESS_HISTORY_STALE_TICKS);
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_history_tests.rs"]
mod tests;
