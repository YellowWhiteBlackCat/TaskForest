//! Windows native metrics collector implementation using approved safe wrappers.

use sysinfo::System;

use super::{MetricsCollector, SystemTelemetrySnapshot};

pub(crate) struct WindowsMetricsCollector {
    sys: System,
}

impl WindowsMetricsCollector {
    #[must_use]
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        Self { sys }
    }
}

impl MetricsCollector for WindowsMetricsCollector {
    fn sample(&mut self, now_ms: u64) -> SystemTelemetrySnapshot {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();

        let cpu_usage_pct = Some((self.sys.global_cpu_usage() as f64).clamp(0.0, 100.0));
        let total = self.sys.total_memory();
        let used = self.sys.used_memory();

        let (memory_used_bytes, memory_total_bytes) = if total > 0 {
            (Some(used), Some(total))
        } else {
            (None, None)
        };

        SystemTelemetrySnapshot {
            timestamp_ms: now_ms,
            cpu_usage_pct,
            memory_used_bytes,
            memory_total_bytes,
        }
    }
}
