//! Conservative, explainable process-anomaly heuristics.
//!
//! These are triage signals, not verdicts. Every rule requires enough current
//! evidence and never treats a missing scalar as zero. The renderer receives
//! typed kinds and may localize the wording at the shell boundary.

use super::ProcessItem;

/// Minimum zombie population before one global zombie-storm signal is emitted.
pub const ZOMBIE_STORM_THRESHOLD: usize = 4;
/// A high current descriptor count that deserves a bounded operator hint.
pub const FD_PRESSURE_THRESHOLD: u32 = 4_096;
/// Minimum number of contiguous memory samples for the monotonic-growth rule.
pub const MEMORY_GROWTH_MIN_SAMPLES: usize = 4;

/// Explainable process anomaly categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessAnomalyKind {
    /// Several zombie rows exist in the same committed process snapshot.
    ZombieStorm,
    /// The process currently owns an unusually large number of descriptors.
    FileDescriptorPressure,
    /// The retained memory trajectory is finite, monotonic, and materially
    /// larger than its first sample.
    MonotonicMemoryGrowth,
}

impl ProcessAnomalyKind {
    /// Stable catalog key used by the shell presentation fold.
    #[must_use]
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::ZombieStorm => "proc.anomaly_zombie_storm",
            Self::FileDescriptorPressure => "proc.anomaly_fd_pressure",
            Self::MonotonicMemoryGrowth => "proc.anomaly_memory_growth",
        }
    }
}

/// One anomaly tied to a process when possible. The storm is a snapshot-wide
/// condition and therefore carries `None` rather than inventing a culprit PID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessAnomaly {
    pub pid: Option<u32>,
    pub kind: ProcessAnomalyKind,
}

/// Evaluate bounded, evidence-based heuristics for one process snapshot.
#[must_use]
pub fn detect_process_anomalies(processes: &[ProcessItem]) -> Vec<ProcessAnomaly> {
    let zombie_count = processes
        .iter()
        .filter(|process| process.status_kind().is_zombie())
        .count();
    let mut anomalies = Vec::new();
    if zombie_count >= ZOMBIE_STORM_THRESHOLD {
        anomalies.push(ProcessAnomaly {
            pid: None,
            kind: ProcessAnomalyKind::ZombieStorm,
        });
    }
    for process in processes {
        if process
            .current_fds()
            .is_some_and(|fds| fds >= FD_PRESSURE_THRESHOLD)
        {
            anomalies.push(ProcessAnomaly {
                pid: Some(process.pid),
                kind: ProcessAnomalyKind::FileDescriptorPressure,
            });
        }
        if has_monotonic_memory_growth(&process.mem_history) {
            anomalies.push(ProcessAnomaly {
                pid: Some(process.pid),
                kind: ProcessAnomalyKind::MonotonicMemoryGrowth,
            });
        }
    }
    anomalies
}

fn has_monotonic_memory_growth(samples: &[f32]) -> bool {
    if samples.len() < MEMORY_GROWTH_MIN_SAMPLES {
        return false;
    }
    let samples = &samples[samples.len() - MEMORY_GROWTH_MIN_SAMPLES..];
    if samples
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        return false;
    }
    let first = samples[0];
    let last = *samples.last().unwrap_or(&first);
    first > 0.0 && samples.windows(2).all(|pair| pair[1] >= pair[0]) && last >= first * 1.10
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_anomaly_tests.rs"]
mod tests;
