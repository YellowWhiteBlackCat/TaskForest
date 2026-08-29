//! Bounded evidence window used only by threshold suggestions.
//!
//! Live graph curves belong to `taskmanager-telemetry-store`. This window is
//! deliberately narrower: it retains only the four numeric facts consumed by
//! `AlertEngine::suggest_threshold`. Binary SMART warning has no numeric
//! series and therefore never acquires a fabricated one.

use std::collections::VecDeque;

use taskmanager_core::core::alerts::{
    AlertEngine, AlertMetric, InsufficientReason, RollingStatSnapshot, SUGGESTION_MIN_SAMPLES,
    SuggestedThreshold,
};
use taskmanager_core::core::metrics::SystemSnapshot;

pub const DEFAULT_SUGGESTION_WINDOW_CAPACITY: usize = 64;
const MIN_SUGGESTION_WINDOW_CAPACITY: usize = 10;
const MAX_SUGGESTION_WINDOW_CAPACITY: usize = 600;

#[derive(Debug, Clone)]
pub struct AlertSuggestionWindow {
    capacity: usize,
    cpu_usage: VecDeque<f32>,
    memory_usage: VecDeque<f32>,
    disk_temperature_c: VecDeque<f32>,
    smart_percent_used: VecDeque<f32>,
}

impl Default for AlertSuggestionWindow {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_SUGGESTION_WINDOW_CAPACITY,
            cpu_usage: VecDeque::new(),
            memory_usage: VecDeque::new(),
            disk_temperature_c: VecDeque::new(),
            smart_percent_used: VecDeque::new(),
        }
    }
}

impl AlertSuggestionWindow {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.clamp(
            MIN_SUGGESTION_WINDOW_CAPACITY,
            MAX_SUGGESTION_WINDOW_CAPACITY,
        );
        for samples in [
            &mut self.cpu_usage,
            &mut self.memory_usage,
            &mut self.disk_temperature_c,
            &mut self.smart_percent_used,
        ] {
            retain_newest(samples, self.capacity);
        }
    }

    /// Fold one committed snapshot into suggestion evidence only.
    pub fn record_snapshot(&mut self, snapshot: &SystemSnapshot) {
        if let Some(value) = snapshot.cpu.current_global_usage_pct() {
            push(&mut self.cpu_usage, value, self.capacity);
        }
        if let Some(value) = snapshot.memory.used_percentage_observed() {
            push(&mut self.memory_usage, value, self.capacity);
        }
        for disk in &snapshot.disks {
            if let Some(value) = disk.smart_temperature_c {
                push(&mut self.disk_temperature_c, value, self.capacity);
            }
            if let Some(value) = disk.smart_percent_used {
                push(&mut self.smart_percent_used, value, self.capacity);
            }
        }
    }

    #[must_use]
    pub fn sample_count(&self, metric: AlertMetric) -> usize {
        self.samples(metric).len()
    }

    #[must_use]
    pub fn suggest(&self, metric: AlertMetric) -> SuggestedThreshold {
        let samples = self.samples(metric);
        if let Some(rolling) = RollingStatSnapshot::from_samples(&samples) {
            return AlertEngine::suggest_threshold(metric, &rolling);
        }
        SuggestedThreshold::Insufficient {
            sample_count: 0,
            required: SUGGESTION_MIN_SAMPLES,
            reason: if matches!(metric, AlertMetric::SmartCriticalWarning) {
                InsufficientReason::UnsupportedMetric
            } else {
                InsufficientReason::TooFewSamples
            },
        }
    }

    fn samples(&self, metric: AlertMetric) -> Vec<f32> {
        match metric {
            AlertMetric::CpuUsagePercent => self.cpu_usage.iter().copied().collect(),
            AlertMetric::MemoryUsagePercent => self.memory_usage.iter().copied().collect(),
            AlertMetric::DiskTemperatureC => self.disk_temperature_c.iter().copied().collect(),
            AlertMetric::SmartPercentUsed => self.smart_percent_used.iter().copied().collect(),
            AlertMetric::SmartCriticalWarning => Vec::new(),
        }
    }
}

fn push(samples: &mut VecDeque<f32>, value: f32, capacity: usize) {
    if value.is_finite() {
        if samples.len() == capacity {
            samples.pop_front();
        }
        samples.push_back(value);
    }
}

fn retain_newest(samples: &mut VecDeque<f32>, capacity: usize) {
    while samples.len() > capacity {
        samples.pop_front();
    }
}

#[cfg(test)]
#[path = "../tests/headless/alert_suggestion_window.rs"]
mod tests;
