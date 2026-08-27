//! Cross-layer vocabulary for persisted telemetry history (roadmap #4, R1).
//!
//! These types are the shared language between three layers: the correlated
//! ingestion seam (`taskmanager-telemetry-store`) emits
//! [`HistoricalSample`] records through the [`HistoryRecordSink`] port, the
//! pure-safe persistence crate (`taskmanager-history-store`) serializes them
//! and answers [`HistoryWindow`] queries as [`HistoricalSeries`] /
//! [`PeakSummary`] read models, and history surfaces consume the read models
//! only. Core owns just the vocabulary — file layout, retention,
//! and path selection belong to the outer layers.
//!
//! Honesty rules baked into the vocabulary: a missing measurement is an
//! explicit `value: None` gap (never a fabricated zero), a wall-clock step
//! backwards is a counted `clock_jumps` fact (never silently rewritten), and a
//! [`PeakSummary`] reports the observed peak only — it never infers cause.

use serde::{Deserialize, Serialize};

use crate::core::DeviceId;

/// One scalar series in the persisted history vocabulary.
///
/// Composite observations (e.g. the periodic GPU metric point) are persisted as
/// one scalar series per field, so every series has a single `f64` axis and a
/// query never has to downcast a mixed payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HistoryMetric {
    UptimeSecs,
    ProcessCount,
    ThreadCount,
    CpuUsagePct,
    CpuCoreUsagePct,
    CpuTemperatureC,
    CpuFrequencyMhz,
    CpuPowerW,
    MemoryUsedPct,
    SwapUsedPct,
    StorageActivityPct,
    NetworkRateBps,
    GpuUsagePct,
    GpuPowerW,
    GpuTemperatureC,
    GpuFrequencyMhz,
    BatteryCapacityPct,
    BatteryPowerW,
    BatteryHealthPct,
    FanRpm,
    FanPwmPct,
    FanTemperatureC,
    ApplicationCpuUsagePct,
    ApplicationMemoryBytes,
    ApplicationProcessCount,
}

impl HistoryMetric {
    /// Every variant, in declaration order — tests and parsers enumerate this
    /// list instead of maintaining a duplicated one.
    pub const ALL: [Self; 25] = [
        Self::UptimeSecs,
        Self::ProcessCount,
        Self::ThreadCount,
        Self::CpuUsagePct,
        Self::CpuCoreUsagePct,
        Self::CpuTemperatureC,
        Self::CpuFrequencyMhz,
        Self::CpuPowerW,
        Self::MemoryUsedPct,
        Self::SwapUsedPct,
        Self::StorageActivityPct,
        Self::NetworkRateBps,
        Self::GpuUsagePct,
        Self::GpuPowerW,
        Self::GpuTemperatureC,
        Self::GpuFrequencyMhz,
        Self::BatteryCapacityPct,
        Self::BatteryPowerW,
        Self::BatteryHealthPct,
        Self::FanRpm,
        Self::FanPwmPct,
        Self::FanTemperatureC,
        Self::ApplicationCpuUsagePct,
        Self::ApplicationMemoryBytes,
        Self::ApplicationProcessCount,
    ];

    /// Stable filesystem-safe slug (round-trips through [`Self::from_slug`]).
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::UptimeSecs => "uptime-secs",
            Self::ProcessCount => "process-count",
            Self::ThreadCount => "thread-count",
            Self::CpuUsagePct => "cpu-usage-pct",
            Self::CpuCoreUsagePct => "cpu-core-usage-pct",
            Self::CpuTemperatureC => "cpu-temperature-c",
            Self::CpuFrequencyMhz => "cpu-frequency-mhz",
            Self::CpuPowerW => "cpu-power-w",
            Self::MemoryUsedPct => "memory-used-pct",
            Self::SwapUsedPct => "swap-used-pct",
            Self::StorageActivityPct => "storage-activity-pct",
            Self::NetworkRateBps => "network-rate-bps",
            Self::GpuUsagePct => "gpu-usage-pct",
            Self::GpuPowerW => "gpu-power-w",
            Self::GpuTemperatureC => "gpu-temperature-c",
            Self::GpuFrequencyMhz => "gpu-frequency-mhz",
            Self::BatteryCapacityPct => "battery-capacity-pct",
            Self::BatteryPowerW => "battery-power-w",
            Self::BatteryHealthPct => "battery-health-pct",
            Self::FanRpm => "fan-rpm",
            Self::FanPwmPct => "fan-pwm-pct",
            Self::FanTemperatureC => "fan-temperature-c",
            Self::ApplicationCpuUsagePct => "application-cpu-usage-pct",
            Self::ApplicationMemoryBytes => "application-memory-bytes",
            Self::ApplicationProcessCount => "application-process-count",
        }
    }

    /// Parse a slug produced by [`Self::slug`]; unknown tokens are `None`, so a
    /// newer vocabulary degrades to an unread series instead of an error.
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|metric| metric.slug() == slug)
    }

    #[must_use]
    pub const fn is_application(self) -> bool {
        matches!(
            self,
            Self::ApplicationCpuUsagePct
                | Self::ApplicationMemoryBytes
                | Self::ApplicationProcessCount
        )
    }
}

/// Stable, privacy-bounded identity for a persisted application series.
///
/// A provider-verified launcher id is the durable authority. Platforms that
/// cannot prove that association may still record an explicitly unverified
/// normalized process name; the provenance stays in the type so replay never
/// presents that fallback as a verified desktop application identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApplicationHistoryIdentity {
    VerifiedLauncher(String),
    UnverifiedProcessName(String),
}

impl ApplicationHistoryIdentity {
    #[must_use]
    pub fn verified_launcher(value: impl Into<String>) -> Option<Self> {
        non_empty_identity(value).map(Self::VerifiedLauncher)
    }

    #[must_use]
    pub fn unverified_process_name(value: impl Into<String>) -> Option<Self> {
        non_empty_identity(value).map(Self::UnverifiedProcessName)
    }

    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            Self::VerifiedLauncher(value) | Self::UnverifiedProcessName(value) => value,
        }
    }

    #[must_use]
    pub const fn is_verified(&self) -> bool {
        matches!(self, Self::VerifiedLauncher(_))
    }

    fn file_token(&self) -> String {
        let (kind, value) = match self {
            Self::VerifiedLauncher(value) => ("launcher", value),
            Self::UnverifiedProcessName(value) => ("process", value),
        };
        format!("{kind}:{}", encode_scope(value))
    }

    fn from_file_token(token: &str) -> Option<Self> {
        let (kind, encoded) = token.split_once(':')?;
        let value = decode_scope(encoded)?;
        match kind {
            "launcher" => Self::verified_launcher(value),
            "process" => Self::unverified_process_name(value),
            _ => None,
        }
    }
}

fn non_empty_identity(value: impl Into<String>) -> Option<String> {
    let value = value.into().trim().to_owned();
    (!value.is_empty()).then_some(value)
}

/// Fully-qualified identity of one persisted scalar series.
///
/// `device` scopes per-device series (storage/network/GPU/battery/fan), and
/// `core_index` scopes per-CPU-core series; a series uses at most one of the
/// two, mirroring how the correlated ingestion fans observations out.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HistorySeriesKey {
    metric: HistoryMetric,
    device: Option<DeviceId>,
    core_index: Option<u16>,
    application: Option<ApplicationHistoryIdentity>,
}

impl HistorySeriesKey {
    #[must_use]
    pub const fn metric(&self) -> HistoryMetric {
        self.metric
    }

    #[must_use]
    pub const fn device(&self) -> Option<&DeviceId> {
        self.device.as_ref()
    }

    #[must_use]
    pub const fn core_index(&self) -> Option<u16> {
        self.core_index
    }

    #[must_use]
    pub const fn application(&self) -> Option<&ApplicationHistoryIdentity> {
        self.application.as_ref()
    }

    #[must_use]
    pub const fn is_application_series(&self) -> bool {
        self.application.is_some() && self.metric.is_application()
    }

    #[must_use]
    pub const fn is_valid(&self) -> bool {
        let scope_count = self.device.is_some() as u8
            + self.core_index.is_some() as u8
            + self.application.is_some() as u8;
        scope_count <= 1 && self.metric.is_application() == self.application.is_some()
    }

    /// A host-wide series with no device or core scope.
    #[must_use]
    pub const fn system(metric: HistoryMetric) -> Self {
        Self {
            metric,
            device: None,
            core_index: None,
            application: None,
        }
    }

    /// A per-device series (storage/network/GPU/battery/fan channel).
    #[must_use]
    pub fn for_device(metric: HistoryMetric, device: DeviceId) -> Self {
        Self {
            metric,
            device: Some(device),
            core_index: None,
            application: None,
        }
    }

    /// A per-CPU-core series (the trailing core index of the fan-out).
    #[must_use]
    pub const fn for_core(metric: HistoryMetric, core_index: u16) -> Self {
        Self {
            metric,
            device: None,
            core_index: Some(core_index),
            application: None,
        }
    }

    /// A per-application series. The identity provenance is part of the key;
    /// display names and localized labels deliberately are not.
    #[must_use]
    pub const fn for_application(
        metric: HistoryMetric,
        application: ApplicationHistoryIdentity,
    ) -> Self {
        Self {
            metric,
            device: None,
            core_index: None,
            application: Some(application),
        }
    }

    /// Filesystem stem for this key. Existing system/device/core series retain
    /// the v1 three-part shape; application series add a fourth typed scope.
    /// This keeps old files canonical while adding one unambiguous namespace.
    #[must_use]
    pub fn file_stem(&self) -> String {
        let device = self
            .device
            .as_ref()
            .map_or_else(|| "-".to_owned(), |device| encode_scope(device.as_str()));
        let core = self
            .core_index
            .map_or_else(|| "-".to_owned(), |index| index.to_string());
        match &self.application {
            Some(application) => format!(
                "{}__{}__{}__{}",
                self.metric.slug(),
                device,
                core,
                application.file_token()
            ),
            None => format!("{}__{}__{}", self.metric.slug(), device, core),
        }
    }

    /// Parse a stem produced by [`Self::file_stem`]. Anything malformed — wrong
    /// part count, unknown metric slug, empty device, non-numeric core — is
    /// `None`, so an unreadable file name never takes the store down.
    #[must_use]
    pub fn from_file_stem(stem: &str) -> Option<Self> {
        let parts = stem.split("__").collect::<Vec<_>>();
        let (metric, device, core, application) = match parts.as_slice() {
            [metric, device, core] => (*metric, *device, *core, None),
            [metric, device, core, application] => (*metric, *device, *core, Some(*application)),
            _ => return None,
        };
        let metric = HistoryMetric::from_slug(metric)?;
        let device = match device {
            "-" => None,
            encoded => {
                let decoded = decode_scope(encoded)?;
                if decoded.is_empty() {
                    return None;
                }
                Some(DeviceId::new(decoded))
            }
        };
        let core_index = match core {
            "-" => None,
            digits => Some(digits.parse::<u16>().ok()?),
        };
        let application = match application {
            Some(token) => Some(ApplicationHistoryIdentity::from_file_token(token)?),
            None => None,
        };
        let key = Self {
            metric,
            device,
            core_index,
            application,
        };
        key.is_valid().then_some(key)
    }
}

/// Percent-encode everything outside the unreserved filename set so any device
/// id round-trips through a file name (and `%` itself is always encoded).
fn encode_scope(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Inverse of [`encode_scope`]; a stray `%` (not followed by two hex digits)
/// yields `None` instead of a lossy guess.
fn decode_scope(encoded: &str) -> Option<String> {
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hex = bytes.get(index + 1..index + 3)?;
                let high = (hex[0] as char).to_digit(16)?;
                let low = (hex[1] as char).to_digit(16)?;
                out.push((high * 16 + low) as u8);
                index += 3;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// One persisted sample. `value: None` is an explicit measurement gap (the
/// accepted observation carried no value) — replay surfaces render it as a
/// gap, never as zero.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoricalSample {
    /// Monotonic per-domain application revision (ordering authority).
    pub revision: u64,
    /// Wall-clock completion time of the accepted correlated event.
    pub completed_at_ms: u64,
    /// Actual measurement time; `None` for stale/unavailable observations.
    pub measured_at_ms: Option<u64>,
    pub value: Option<f64>,
}

impl HistoricalSample {
    /// Whether this sample records the absence of a measurement.
    #[must_use]
    pub const fn is_gap(self) -> bool {
        self.value.is_none()
    }
}

/// Query/read model for one series over a window.
///
/// Samples stay in ingestion (revision) order; `clock_jumps` counts wall-clock
/// steps backwards observed across that span — the data is kept as recorded,
/// only the fact is surfaced.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoricalSeries {
    pub key: HistorySeriesKey,
    pub samples: Vec<HistoricalSample>,
    pub clock_jumps: u32,
}

impl HistoricalSeries {
    /// Build a series and count the clock jumps across its samples.
    #[must_use]
    pub fn new(key: HistorySeriesKey, samples: Vec<HistoricalSample>) -> Self {
        let clock_jumps = count_clock_jumps(&samples);
        Self {
            key,
            samples,
            clock_jumps,
        }
    }

    /// The highest measured value in the series (gaps ignored, first sample
    /// wins ties). Fact-only — no time-of-peak causal claim.
    #[must_use]
    pub fn peak(&self) -> Option<HistoricalSample> {
        self.samples
            .iter()
            .filter(|sample| sample.value.is_some())
            .copied()
            .max_by(|left, right| {
                left.value
                    .partial_cmp(&right.value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// How many samples in the series are explicit gaps.
    #[must_use]
    pub fn gap_count(&self) -> usize {
        self.samples.iter().filter(|sample| sample.is_gap()).count()
    }
}

/// Count wall-clock steps backwards in revision-ordered samples, comparing
/// each sample against its immediate predecessor. A step forward — however
/// large (suspend, resumed clock skew) — is a gap in time, not a clock jump,
/// and is not counted.
#[must_use]
pub fn count_clock_jumps(samples: &[HistoricalSample]) -> u32 {
    let mut jumps = 0u32;
    let mut previous: Option<u64> = None;
    for sample in samples {
        if previous.is_some_and(|last| sample.completed_at_ms < last) {
            jumps = jumps.saturating_add(1);
        }
        previous = Some(sample.completed_at_ms);
    }
    jumps
}

/// Replay window for history-mode queries (roadmap #4: 1h / 24h / 7d).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HistoryWindow {
    OneHour,
    TwentyFourHours,
    SevenDays,
}

impl HistoryWindow {
    /// Every variant, in declaration order.
    pub const ALL: [Self; 3] = [Self::OneHour, Self::TwentyFourHours, Self::SevenDays];

    #[must_use]
    pub const fn duration_ms(self) -> u64 {
        match self {
            Self::OneHour => 60 * 60 * 1000,
            Self::TwentyFourHours => 24 * 60 * 60 * 1000,
            Self::SevenDays => 7 * 24 * 60 * 60 * 1000,
        }
    }
}

/// Fact-only peak summary for one series over one window.
///
/// Reports what was observed (peak value and when, sample/gap counts, clock
/// jumps). It never infers cause or attributes the peak to a process.
#[derive(Clone, Debug, PartialEq)]
pub struct PeakSummary {
    pub key: HistorySeriesKey,
    pub window: HistoryWindow,
    pub peak_value: Option<f64>,
    pub peak_measured_at_ms: Option<u64>,
    pub observed_samples: usize,
    pub gap_samples: usize,
    pub clock_jumps: u32,
}

/// Write port for the persistence layer (roadmap #4 store seam).
///
/// Implemented by the pure-safe history store; driven only from the correlated
/// ingestion seam so every persisted sample passed the same revision/
/// completion-time acceptance the in-memory rings enforce. The store receives
/// records synchronously but is free to buffer and flush on its own cadence.
pub trait HistoryRecordSink: Send + Sync {
    /// Record one accepted sample (or explicit gap) for one series.
    fn record_sample(&self, key: HistorySeriesKey, sample: HistoricalSample);
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_history_tests.rs"]
mod tests;
