//! Operating-system metric sampling engine feeding history and live IPC telemetry.

#[cfg(target_os = "linux")]
use std::fs;
use std::io;

use taskmanager_core::{HistoricalSample, HistoryMetric, HistorySeriesKey};

use crate::persistence::DaemonHistoryManager;

/// System metrics snapshot sampled by the daemon collector.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemTelemetrySnapshot {
    pub timestamp_ms: u64,
    pub cpu_usage_pct: Option<f64>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
}

/// Daemon background collector reading platform metrics and updating persistence.
#[derive(Default)]
pub struct DaemonCollector {
    #[cfg(target_os = "linux")]
    last_cpu_idle: u64,
    #[cfg(target_os = "linux")]
    last_cpu_total: u64,
    revision: u64,
}

impl DaemonCollector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Perform one metric sampling pass from OS sources.
    pub fn sample_and_record(
        &mut self,
        now_ms: u64,
        history: &DaemonHistoryManager,
    ) -> io::Result<SystemTelemetrySnapshot> {
        self.revision = self.revision.wrapping_add(1);

        let cpu_usage = self.sample_cpu();
        let (mem_used, mem_total) = self.sample_memory();

        // 1. Record CPU usage history
        if let Some(cpu) = cpu_usage {
            let cpu_key = HistorySeriesKey::system(HistoryMetric::CpuUsagePct);
            let sample = HistoricalSample {
                revision: self.revision,
                completed_at_ms: now_ms,
                measured_at_ms: Some(now_ms),
                value: Some(cpu),
            };
            let _ = history.record_sample(cpu_key, sample);
        }

        // 2. Record Memory usage history
        if let (Some(used), Some(total)) = (mem_used, mem_total) {
            if total > 0 {
                let mem_pct = (used as f64 / total as f64) * 100.0;
                let mem_key = HistorySeriesKey::system(HistoryMetric::MemoryUsedPct);
                let sample = HistoricalSample {
                    revision: self.revision,
                    completed_at_ms: now_ms,
                    measured_at_ms: Some(now_ms),
                    value: Some(mem_pct),
                };
                let _ = history.record_sample(mem_key, sample);
            }
        }

        Ok(SystemTelemetrySnapshot {
            timestamp_ms: now_ms,
            cpu_usage_pct: cpu_usage,
            memory_used_bytes: mem_used,
            memory_total_bytes: mem_total,
        })
    }

    #[cfg(target_os = "linux")]
    fn sample_cpu(&mut self) -> Option<f64> {
        let content = fs::read_to_string("/proc/stat").ok()?;
        let first_line = content.lines().next()?;
        if !first_line.starts_with("cpu ") {
            return None;
        }

        let parts: Vec<u64> = first_line
            .split_whitespace()
            .skip(1)
            .filter_map(|s| s.parse().ok())
            .collect();

        if parts.len() < 4 {
            return None;
        }

        let user = parts[0];
        let nice = parts[1];
        let system = parts[2];
        let idle = parts[3];
        let iowait = parts.get(4).copied().unwrap_or(0);
        let irq = parts.get(5).copied().unwrap_or(0);
        let softirq = parts.get(6).copied().unwrap_or(0);
        let steal = parts.get(7).copied().unwrap_or(0);

        let total = user + nice + system + idle + iowait + irq + softirq + steal;
        let idle_total = idle + iowait;

        if self.last_cpu_total == 0 || total <= self.last_cpu_total {
            self.last_cpu_idle = idle_total;
            self.last_cpu_total = total;
            return None;
        }

        let total_delta = total - self.last_cpu_total;
        let idle_delta = idle_total.saturating_sub(self.last_cpu_idle);

        self.last_cpu_idle = idle_total;
        self.last_cpu_total = total;

        let usage = 100.0 * (1.0 - (idle_delta as f64 / total_delta as f64));
        Some(usage.clamp(0.0, 100.0))
    }

    #[cfg(not(target_os = "linux"))]
    fn sample_cpu(&mut self) -> Option<f64> {
        None
    }

    #[cfg(target_os = "linux")]
    fn sample_memory(&self) -> (Option<u64>, Option<u64>) {
        let content = match fs::read_to_string("/proc/meminfo") {
            Ok(c) => c,
            Err(_) => return (None, None),
        };

        let mut total_kb = None;
        let mut avail_kb = None;

        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = parse_meminfo_kb(line);
            } else if line.starts_with("MemAvailable:") {
                avail_kb = parse_meminfo_kb(line);
            }
            if total_kb.is_some() && avail_kb.is_some() {
                break;
            }
        }

        match (total_kb, avail_kb) {
            (Some(tot), Some(avail)) => {
                let used = tot.saturating_sub(avail);
                (Some(used * 1024), Some(tot * 1024))
            }
            _ => (None, None),
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn sample_memory(&self) -> (Option<u64>, Option<u64>) {
        (None, None)
    }
}

#[cfg(target_os = "linux")]
fn parse_meminfo_kb(line: &str) -> Option<u64> {
    let mut parts = line.split_whitespace();
    let _label = parts.next()?;
    let value = parts.next()?.parse::<u64>().ok()?;
    Some(value)
}
