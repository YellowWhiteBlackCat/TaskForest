//! Shared System dashboard windows and cached read projection over authoritative history.

use std::sync::{Arc, Mutex};

pub const MAX_GRAPH_POINTS: usize = 241;
mod samples;
use samples::metric_window;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SystemHistoryWindow {
    OneMinute,
    FiveMinutes,
    FifteenMinutes,
    #[default]
    SixtyMinutes,
}

impl SystemHistoryWindow {
    pub const ALL: [Self; 4] = [
        Self::OneMinute,
        Self::FiveMinutes,
        Self::FifteenMinutes,
        Self::SixtyMinutes,
    ];

    pub fn minutes(self) -> u64 {
        match self {
            Self::OneMinute => 1,
            Self::FiveMinutes => 5,
            Self::FifteenMinutes => 15,
            Self::SixtyMinutes => 60,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::OneMinute => "history-1m",
            Self::FiveMinutes => "history-5m",
            Self::FifteenMinutes => "history-15m",
            Self::SixtyMinutes => "history-60m",
        }
    }
}

impl SystemHistoryWindow {
    pub const fn label(self) -> &'static str {
        match self {
            Self::OneMinute => "1m",
            Self::FiveMinutes => "5m",
            Self::FifteenMinutes => "15m",
            Self::SixtyMinutes => "60m",
        }
    }
}
impl std::fmt::Display for SystemHistoryWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SystemPageSection {
    #[default]
    Dashboard,
    Hardware,
    Health,
}

impl SystemPageSection {
    pub const ALL_REVIEW: [Self; 2] = [Self::Dashboard, Self::Hardware];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TimelineMetric {
    #[default]
    Cpu,
    Memory,
    Disk,
    Network,
}

impl TimelineMetric {
    pub const ALL: [Self; 4] = [Self::Cpu, Self::Memory, Self::Disk, Self::Network];
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Cpu => "common.cpu",
            Self::Memory => "common.memory",
            Self::Disk => "common.disk",
            Self::Network => "common.network",
        }
    }
    pub const fn unit(self) -> &'static str {
        match self {
            Self::Cpu | Self::Memory => "%",
            Self::Disk | Self::Network => "MiB/s",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Disk => "disk",
            Self::Network => "network",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimelineStatistic {
    #[default]
    Latest,
    Peak,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimelineSelection {
    pub metric: TimelineMetric,
    pub statistic: TimelineStatistic,
}

impl TimelineSelection {
    pub const fn new(metric: TimelineMetric, statistic: TimelineStatistic) -> Self {
        Self { metric, statistic }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineReadout {
    pub value: f32,
    pub sample_index: usize,
}

/// Windowed dashboard series, one family per history card.
///
/// Immutable sample allocations remain stable across UI-only updates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimelineSeries {
    pub cpu_percent: Arc<[f32]>,
    pub memory_percent: Arc<[f32]>,
    pub disk_mib_per_sec: Arc<[f32]>,
    pub network_mib_per_sec: Arc<[f32]>,
    pub covered_ms: u64,
    pub anchor_ms: u64,
    pub window_ms: u64,
    coverage: [u64; 4],
    readouts: [MetricReadouts; 4],
}

impl TimelineSeries {
    pub fn coverage_ms(&self, metric: TimelineMetric) -> u64 {
        self.coverage[metric.index()]
    }

    /// One metric's sample buffer by shared handle. The dashboard history
    /// cards pass the returned `Arc` straight into the graph element, so a
    /// memoized frame reuses the exact allocation the scene cache holds.
    pub fn samples(&self, metric: TimelineMetric) -> Arc<[f32]> {
        match metric {
            TimelineMetric::Cpu => Arc::clone(&self.cpu_percent),
            TimelineMetric::Memory => Arc::clone(&self.memory_percent),
            TimelineMetric::Disk => Arc::clone(&self.disk_mib_per_sec),
            TimelineMetric::Network => Arc::clone(&self.network_mib_per_sec),
        }
    }

    /// Stable keyboard/pointer-independent summary selection contract. Graph
    /// painting can evolve independently while latest/peak readouts stay typed.
    pub fn readout(&self, selection: TimelineSelection) -> Option<TimelineReadout> {
        let readouts = self.readouts[selection.metric.index()];
        match selection.statistic {
            TimelineStatistic::Latest => readouts.latest,
            TimelineStatistic::Peak => readouts.peak,
        }
    }
}

impl TimelineMetric {
    const fn index(self) -> usize {
        match self {
            Self::Cpu => 0,
            Self::Memory => 1,
            Self::Disk => 2,
            Self::Network => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct MetricReadouts {
    latest: Option<TimelineReadout>,
    peak: Option<TimelineReadout>,
}

/// Versioned memo entry for the last `series` build: the sample-mutation
/// version it captured, the window it filtered for, and the shared payload.
#[derive(Clone, Debug)]
struct SeriesMemo {
    source: TimelineSourceKey,
    window: SystemHistoryWindow,
    series: TimelineSeries,
}

/// Correlation watermark and stable identity of one authoritative metric reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetricHistoryRevision {
    pub source_id: usize,
    pub len: usize,
    pub last_revision: Option<u64>,
}

/// One accepted sample in the metric's canonical graph unit. Missing and stale
/// observations carry no measurement; ports never substitute a zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineSample {
    pub completed_at_ms: u64,
    pub measured_at_ms: Option<u64>,
    pub value: Option<f32>,
}

/// Read-only inward port. Revision probes do not clone retained samples;
/// sample windows are read only when the application projection cache misses.
pub trait TimelineHistorySource {
    fn revision(&self, metric: TimelineMetric) -> MetricHistoryRevision;
    fn latest_completion_ms(&self, metric: TimelineMetric) -> Option<u64>;
    fn samples(&self, metric: TimelineMetric) -> Vec<TimelineSample>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimelineSourceKey {
    cpu: MetricHistoryRevision,
    memory: MetricHistoryRevision,
    disk: MetricHistoryRevision,
    network: MetricHistoryRevision,
}

impl TimelineSourceKey {
    fn from_history(history: &impl TimelineHistorySource) -> Self {
        Self {
            cpu: history.revision(TimelineMetric::Cpu),
            memory: history.revision(TimelineMetric::Memory),
            disk: history.revision(TimelineMetric::Disk),
            network: history.revision(TimelineMetric::Network),
        }
    }
}

/// Cached projection keyed by authoritative ring identities, revisions and the selected window.
#[derive(Clone, Debug, Default)]
pub struct TimelineState {
    series_cache: Arc<Mutex<Option<SeriesMemo>>>,
}

impl TimelineState {
    /// An unchanged read reuses immutable allocations; accepted telemetry or a new window rebuilds them.
    pub fn series(
        &self,
        history: &impl TimelineHistorySource,
        window: SystemHistoryWindow,
    ) -> TimelineSeries {
        let source = TimelineSourceKey::from_history(history);
        {
            let cache = self
                .series_cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(memo) = cache.as_ref()
                && memo.source == source
                && memo.window == window
            {
                return memo.series.clone();
            }
        }
        let series = Self::rebuild_series(history, window);
        *self
            .series_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(SeriesMemo {
            source,
            window,
            series: series.clone(),
        });
        series
    }

    fn rebuild_series(
        history: &impl TimelineHistorySource,
        window: SystemHistoryWindow,
    ) -> TimelineSeries {
        let anchor = [
            history.latest_completion_ms(TimelineMetric::Cpu),
            history.latest_completion_ms(TimelineMetric::Memory),
            history.latest_completion_ms(TimelineMetric::Disk),
            history.latest_completion_ms(TimelineMetric::Network),
        ]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(0);
        let cpu = metric_window(history.samples(TimelineMetric::Cpu), window, anchor);
        let memory = metric_window(history.samples(TimelineMetric::Memory), window, anchor);
        let disk = metric_window(history.samples(TimelineMetric::Disk), window, anchor);
        let network = metric_window(history.samples(TimelineMetric::Network), window, anchor);
        TimelineSeries {
            anchor_ms: anchor,
            window_ms: window.minutes() * 60_000,
            coverage: [
                cpu.covered_ms,
                memory.covered_ms,
                disk.covered_ms,
                network.covered_ms,
            ],
            cpu_percent: cpu.values,
            memory_percent: memory.values,
            disk_mib_per_sec: disk.values,
            network_mib_per_sec: network.values,
            covered_ms: [
                cpu.covered_ms,
                memory.covered_ms,
                disk.covered_ms,
                network.covered_ms,
            ]
            .into_iter()
            .max()
            .unwrap_or_default(),
            readouts: [
                cpu.readouts,
                memory.readouts,
                disk.readouts,
                network.readouts,
            ],
        }
    }
}

struct MetricWindow {
    values: Arc<[f32]>,
    covered_ms: u64,
    readouts: MetricReadouts,
}

fn raw_readouts(samples: &[f32]) -> MetricReadouts {
    let latest = samples
        .iter()
        .enumerate()
        .next_back()
        .filter(|(_, value)| value.is_finite())
        .map(|(sample_index, value)| TimelineReadout {
            value: *value,
            sample_index,
        });
    let peak = samples
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_finite())
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(sample_index, value)| TimelineReadout {
            value: *value,
            sample_index,
        });
    MetricReadouts { latest, peak }
}

pub fn bytes_per_sec_to_mib(value: u64) -> f32 {
    bounded_graph_f32(u64_as_f64(value) / (1024.0 * 1024.0))
}

fn u64_as_f64(value: u64) -> f64 {
    const RADIX: f64 = 65_536.0;
    value
        .to_be_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|bytes| f64::from(u16::from_be_bytes(*bytes)))
        .fold(0.0, |accumulator, word| accumulator.mul_add(RADIX, word))
}

fn bounded_graph_f32(value: f64) -> f32 {
    value.clamp(f64::from(f32::MIN), f64::from(f32::MAX)) as f32
}
