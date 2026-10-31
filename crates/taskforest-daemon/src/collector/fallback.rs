//! Fallback metrics collector for unmapped or portable environments.

use super::{MetricsCollector, SystemTelemetrySnapshot};

#[derive(Default)]
pub(crate) struct FallbackMetricsCollector;

impl FallbackMetricsCollector {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl MetricsCollector for FallbackMetricsCollector {
    fn sample(&mut self, now_ms: u64) -> SystemTelemetrySnapshot {
        SystemTelemetrySnapshot {
            timestamp_ms: now_ms,
            cpu_usage_pct: None,
            memory_used_bytes: None,
            memory_total_bytes: None,
        }
    }
}
