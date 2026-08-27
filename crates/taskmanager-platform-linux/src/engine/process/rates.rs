//! Identity-bound rates derived from per-process procfs counters.

use std::collections::{HashMap, HashSet};

use taskmanager_core::{CounterDelta, CumulativeCounter, FailureKind, ScalarObservation};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ProcessRateObservations {
    pub(super) cpu_percentage: ScalarObservation<f32>,
    pub(super) disk_read_bytes_per_sec: ScalarObservation<u64>,
    pub(super) disk_write_bytes_per_sec: ScalarObservation<u64>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ProcessRateInput<'a> {
    pub(super) pid: u32,
    pub(super) start_token: u64,
    pub(super) observed_at_ms: u64,
    pub(super) clock_ticks: &'a Result<u64, FailureKind>,
    pub(super) cpu_ticks: Result<u64, FailureKind>,
    pub(super) disk_read_bytes: Result<u64, FailureKind>,
    pub(super) disk_write_bytes: Result<u64, FailureKind>,
}

#[derive(Debug, Clone, Copy)]
struct ProcessRateBaseline {
    start_token: u64,
    cpu_ticks: CumulativeCounter,
    disk_read_bytes: CumulativeCounter,
    disk_write_bytes: CumulativeCounter,
}

impl ProcessRateBaseline {
    fn new(start_token: u64) -> Self {
        Self {
            start_token,
            cpu_ticks: CumulativeCounter::default(),
            disk_read_bytes: CumulativeCounter::default(),
            disk_write_bytes: CumulativeCounter::default(),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct ProcessRateState {
    baselines: HashMap<u32, ProcessRateBaseline>,
}

impl ProcessRateState {
    pub(super) fn observe(&mut self, input: ProcessRateInput<'_>) -> ProcessRateObservations {
        let ProcessRateInput {
            pid,
            start_token,
            observed_at_ms,
            clock_ticks,
            cpu_ticks,
            disk_read_bytes,
            disk_write_bytes,
        } = input;
        // Single hash lookup in the steady state (the previous get + insert
        // + entry triple-hashed every process every tick).
        let mut identity_changed = false;
        let baseline = self
            .baselines
            .entry(pid)
            .or_insert_with(|| ProcessRateBaseline::new(start_token));
        if baseline.start_token != start_token {
            *baseline = ProcessRateBaseline::new(start_token);
            identity_changed = true;
        }
        let gap_failure = if identity_changed {
            FailureKind::IdentityChanged
        } else {
            FailureKind::TemporarilyUnavailable
        };

        let cpu_delta = baseline
            .cpu_ticks
            .observe(cpu_ticks, observed_at_ms, gap_failure);
        let read_delta =
            baseline
                .disk_read_bytes
                .observe(disk_read_bytes, observed_at_ms, gap_failure);
        let write_delta =
            baseline
                .disk_write_bytes
                .observe(disk_write_bytes, observed_at_ms, gap_failure);

        ProcessRateObservations {
            cpu_percentage: cpu_percentage(cpu_delta, clock_ticks, observed_at_ms),
            disk_read_bytes_per_sec: read_delta.per_second(observed_at_ms),
            disk_write_bytes_per_sec: write_delta.per_second(observed_at_ms),
        }
    }

    pub(super) fn reset(&mut self, pid: u32) {
        self.baselines.remove(&pid);
    }

    pub(super) fn retain_pids(&mut self, pids: &HashSet<u32>) {
        self.baselines.retain(|pid, _| pids.contains(pid));
    }

    pub(super) fn clear(&mut self) {
        self.baselines.clear();
    }
}

fn cpu_percentage(
    delta: CounterDelta,
    clock_ticks: &Result<u64, FailureKind>,
    observed_at_ms: u64,
) -> ScalarObservation<f32> {
    let CounterDelta::Available { value, elapsed_ms } = delta else {
        return ScalarObservation::unavailable(delta.failure());
    };
    let ticks_per_second = match clock_ticks {
        Ok(0) => return ScalarObservation::unavailable(FailureKind::ProviderFault),
        Ok(value) => *value,
        Err(failure) => return ScalarObservation::unavailable(*failure),
    };
    let percentage = (value as f64 * 100_000.0) / (ticks_per_second as f64 * elapsed_ms as f64);
    let percentage = percentage as f32;
    if percentage.is_finite() && percentage >= 0.0 {
        ScalarObservation::available(percentage, observed_at_ms)
    } else {
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_rates_tests.rs"]
mod tests;
