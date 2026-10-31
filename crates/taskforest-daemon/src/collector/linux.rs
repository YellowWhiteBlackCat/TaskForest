//! Linux procfs metrics collector implementation.

use std::fs;

use super::{MetricsCollector, SystemTelemetrySnapshot};

#[derive(Default)]
pub(crate) struct LinuxMetricsCollector {
    last_cpu_idle: u64,
    last_cpu_total: u64,
}

impl LinuxMetricsCollector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

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

    fn sample_memory(&mut self) -> (Option<u64>, Option<u64>) {
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
}

impl MetricsCollector for LinuxMetricsCollector {
    fn sample(&mut self, now_ms: u64) -> SystemTelemetrySnapshot {
        let cpu_usage_pct = self.sample_cpu();
        let (memory_used_bytes, memory_total_bytes) = self.sample_memory();
        SystemTelemetrySnapshot {
            timestamp_ms: now_ms,
            cpu_usage_pct,
            memory_used_bytes,
            memory_total_bytes,
        }
    }
}

fn parse_meminfo_kb(line: &str) -> Option<u64> {
    let mut parts = line.split_whitespace();
    let _label = parts.next()?;
    let value = parts.next()?.parse::<u64>().ok()?;
    Some(value)
}
