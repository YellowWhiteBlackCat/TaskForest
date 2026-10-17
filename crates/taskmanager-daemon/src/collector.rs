//! Platform-neutral metrics collector trait, adapters, and persistence driver.

use std::io;

use serde::{Deserialize, Serialize};
use taskmanager_core::{HistoricalSample, HistoryMetric, HistorySeriesKey};

use crate::persistence::DaemonHistoryManager;

/// System metrics snapshot sampled by the daemon collector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemTelemetrySnapshot {
    pub timestamp_ms: u64,
    pub cpu_usage_pct: Option<f64>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
}

/// Trait defining the platform-specific hardware sampling provider.
pub trait MetricsCollector: Send + Sync {
    /// Perform one sampling pass from native operating system sources.
    fn sample(&mut self, now_ms: u64) -> SystemTelemetrySnapshot;
}

#[cfg(target_os = "linux")]
#[path = "collector/linux.rs"]
mod linux;

#[cfg(target_os = "windows")]
#[path = "collector/windows.rs"]
mod windows;

#[cfg(target_os = "macos")]
#[path = "collector/macos.rs"]
mod macos;

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
#[path = "collector/fallback.rs"]
mod fallback;

/// Instantiate the active platform's native metrics collector implementation.
#[must_use]
pub fn create_platform_collector() -> Box<dyn MetricsCollector> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxMetricsCollector::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsMetricsCollector::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacosMetricsCollector::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Box::new(fallback::FallbackMetricsCollector::new())
    }
}

/// Daemon background collector driving the platform adapter and updating persistence.
pub struct DaemonCollector {
    collector: Box<dyn MetricsCollector>,
    revision: u64,
}

impl Default for DaemonCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonCollector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            collector: create_platform_collector(),
            revision: 0,
        }
    }

    /// Construct with an explicit collector implementation (for testing or custom providers).
    #[must_use]
    pub fn with_collector(collector: Box<dyn MetricsCollector>) -> Self {
        Self {
            collector,
            revision: 0,
        }
    }

    /// Perform one metric sampling pass from the native OS provider and record into history.
    pub fn sample_and_record(
        &mut self,
        now_ms: u64,
        history: &DaemonHistoryManager,
    ) -> io::Result<SystemTelemetrySnapshot> {
        self.revision = self.revision.wrapping_add(1);

        let snapshot = self.collector.sample(now_ms);

        // 1. Record CPU usage history
        if let Some(cpu) = snapshot.cpu_usage_pct {
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
        if let (Some(used), Some(total)) = (snapshot.memory_used_bytes, snapshot.memory_total_bytes)
            && total > 0
        {
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

        Ok(snapshot)
    }
}
