//! Platform-neutral per-process resource-history rules.
//!
//! Native adapters provide a verified process identity, typed current sample,
//! and one monotonic timestamp per refresh. This module owns the trailing
//! window, PID-reuse reset, hard capacity, synchronized projections, and stale
//! PID pruning.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use super::ProcessItem;

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

#[derive(Debug, Clone)]
struct ProcessHistoryRing {
    samples: VecDeque<TimedProcessHistorySample>,
    last_seen: u64,
    start_token: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct TimedProcessHistorySample {
    value: ProcessHistorySample,
    observed_at: Duration,
}

impl Default for ProcessHistoryRing {
    fn default() -> Self {
        Self {
            samples: VecDeque::with_capacity(PROCESS_HISTORY_MAX_SAMPLES),
            last_seen: 0,
            start_token: None,
        }
    }
}

impl ProcessHistoryRing {
    fn note_identity(&mut self, start_token: Option<u64>) {
        let Some(start_token) = start_token else {
            self.samples.clear();
            self.start_token = None;
            return;
        };
        if self.start_token.is_none() && !self.samples.is_empty()
            || self
                .start_token
                .is_some_and(|current| current != start_token)
        {
            self.samples.clear();
        }
        self.start_token = Some(start_token);
    }

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
                .filter_map(|sample| sample.value.cpu)
                .collect(),
            memory: self
                .samples
                .iter()
                .filter_map(|sample| sample.value.memory)
                .collect(),
            disk: self
                .samples
                .iter()
                .filter_map(|sample| {
                    sample
                        .value
                        .disk_read
                        .zip(sample.value.disk_write)
                        .map(|(read, write)| read + write)
                })
                .collect(),
            disk_read: self
                .samples
                .iter()
                .filter_map(|sample| sample.value.disk_read)
                .collect(),
            disk_write: self
                .samples
                .iter()
                .filter_map(|sample| sample.value.disk_write)
                .collect(),
        }
    }
}

/// Bounded histories keyed by PID and guarded by provider-native start token.
#[derive(Debug, Clone, Default)]
pub struct ProcessHistoryStore {
    rings: HashMap<u32, ProcessHistoryRing>,
    tick: u64,
    observed_at: Duration,
}

impl ProcessHistoryStore {
    /// Advance once per provider refresh using a caller-supplied monotonic age.
    pub fn begin_refresh(&mut self, observed_at: Duration) {
        self.tick = self.tick.wrapping_add(1);
        self.observed_at = observed_at;
    }

    /// Reset on unprovable/replaced identity, then append the current sample.
    pub fn record(
        &mut self,
        pid: u32,
        start_token: Option<u64>,
        sample: ProcessHistorySample,
    ) -> ProcessHistorySnapshot {
        let ring = self.rings.entry(pid).or_default();
        ring.note_identity(start_token);
        ring.push(sample, self.tick, self.observed_at);
        ring.snapshot()
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
