//! Dashboard projection over the correlated telemetry-store authority.
//!
//! `RootView` owns only the memoized/downsampled GPUI projection. Accepted
//! samples, gaps, correlation revisions and bounds remain owned by
//! `taskmanager-telemetry-store`; this module has no append path of its own.

use std::cell::RefCell;
use std::rc::Rc;

use taskmanager_telemetry_store::{
    CorrelatedMetricHistory, CorrelatedMetricSample, CorrelatedSystemTelemetryHistory,
};

const MAX_GRAPH_POINTS: usize = 240;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HistoryWindow {
    OneMinute,
    FiveMinutes,
    FifteenMinutes,
    #[default]
    SixtyMinutes,
}

impl HistoryWindow {
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimelineMetric {
    #[default]
    Cpu,
    Memory,
    Disk,
    Network,
}

impl TimelineMetric {
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
/// Each sample buffer is shared through `Rc` so an unchanged frame clones
/// four pointer-sized handles instead of four sample `Vec`s — the shared
/// allocation identity is what `graph::scene_cache` keys its scene replay
/// on under gpui 0.2.2's full-window repaint model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimelineSeries {
    pub cpu_percent: Rc<[f32]>,
    pub memory_percent: Rc<[f32]>,
    pub disk_mib_per_sec: Rc<[f32]>,
    pub network_mib_per_sec: Rc<[f32]>,
    pub covered_ms: u64,
    readouts: [MetricReadouts; 4],
}

impl TimelineSeries {
    /// One metric's sample buffer by shared handle. The dashboard history
    /// cards pass the returned `Rc` straight into the graph element, so a
    /// memoized frame reuses the exact allocation the scene cache holds.
    pub fn samples(&self, metric: TimelineMetric) -> Rc<[f32]> {
        match metric {
            TimelineMetric::Cpu => Rc::clone(&self.cpu_percent),
            TimelineMetric::Memory => Rc::clone(&self.memory_percent),
            TimelineMetric::Disk => Rc::clone(&self.disk_mib_per_sec),
            TimelineMetric::Network => Rc::clone(&self.network_mib_per_sec),
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
    window: HistoryWindow,
    series: TimelineSeries,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MetricSourceKey {
    ring: usize,
    len: usize,
    last_revision: Option<u64>,
}

impl<T> From<&CorrelatedMetricHistory<T>> for MetricSourceKey {
    fn from(history: &CorrelatedMetricHistory<T>) -> Self {
        let (len, last_revision) = history.watermark();
        Self {
            ring: history.ring_id(),
            len,
            last_revision,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimelineSourceKey {
    cpu: MetricSourceKey,
    memory: MetricSourceKey,
    disk: MetricSourceKey,
    network: MetricSourceKey,
}

impl TimelineSourceKey {
    fn from_history(history: &CorrelatedSystemTelemetryHistory) -> Self {
        Self {
            cpu: MetricSourceKey::from(&history.cpu_usage()),
            memory: MetricSourceKey::from(&history.memory_usage()),
            disk: MetricSourceKey::from(&history.storage_rate_total()),
            network: MetricSourceKey::from(&history.network_rate_total()),
        }
    }
}

/// Root-owned one-entry memo over telemetry-store read capabilities.
///
/// gpui 0.2.2 repaints the whole window every frame. Keying the derived
/// projection by each authoritative ring's identity and watermark preserves
/// the graph scene-cache allocation across UI-only frames without retaining a
/// second sample window or a frontend append path.
#[derive(Clone, Debug, Default)]
pub struct TimelineState {
    series_cache: RefCell<Option<SeriesMemo>>,
}

impl TimelineState {
    /// Windowed series for the dashboard history cards, memoized on
    /// (authoritative ring identity/watermark, window).
    ///
    /// A hit clones only the `Rc` handles — the full-window repaints gpui
    /// 0.2.2 issues for unchanged frames then reuse the exact sample
    /// allocations and `graph::scene_cache` replays its cached scene
    /// instead of rebuilding it. A miss scans and downsamples once, then
    /// re-stores the memo for the following frames.
    pub fn series(
        &self,
        history: &CorrelatedSystemTelemetryHistory,
        window: HistoryWindow,
    ) -> TimelineSeries {
        let source = TimelineSourceKey::from_history(history);
        {
            let cache = self.series_cache.borrow();
            if let Some(memo) = cache.as_ref()
                && memo.source == source
                && memo.window == window
            {
                return memo.series.clone();
            }
        }
        let series = Self::rebuild_series(history, window);
        *self.series_cache.borrow_mut() = Some(SeriesMemo {
            source,
            window,
            series: series.clone(),
        });
        series
    }

    fn rebuild_series(
        history: &CorrelatedSystemTelemetryHistory,
        window: HistoryWindow,
    ) -> TimelineSeries {
        let cpu = f32_window(history.cpu_usage(), window, |value| value);
        let memory = f32_window(history.memory_usage(), window, |value| value);
        let disk = u64_window(history.storage_rate_total(), window, bytes_per_sec_to_mib);
        let network = u64_window(history.network_rate_total(), window, bytes_per_sec_to_mib);
        TimelineSeries {
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
    values: Rc<[f32]>,
    covered_ms: u64,
    readouts: MetricReadouts,
}

fn f32_window(
    history: CorrelatedMetricHistory<f32>,
    window: HistoryWindow,
    project: impl Fn(f32) -> f32,
) -> MetricWindow {
    metric_window(history.samples(), window, |sample| {
        match (sample.measured_at_ms, sample.value) {
            (Some(_), Some(value)) if value.is_finite() => project(value),
            _ => f32::NAN,
        }
    })
}

fn u64_window(
    history: CorrelatedMetricHistory<u64>,
    window: HistoryWindow,
    project: impl Fn(u64) -> f32,
) -> MetricWindow {
    metric_window(history.samples(), window, |sample| {
        match (sample.measured_at_ms, sample.value) {
            (Some(_), Some(value)) => project(value),
            _ => f32::NAN,
        }
    })
}

fn metric_window<T>(
    samples: Vec<CorrelatedMetricSample<T>>,
    window: HistoryWindow,
    value: impl Fn(&CorrelatedMetricSample<T>) -> f32,
) -> MetricWindow {
    let Some(anchor) = samples.last().map(|sample| sample.stamp.completed_at_ms()) else {
        return MetricWindow {
            values: Rc::from([]),
            covered_ms: 0,
            readouts: MetricReadouts::default(),
        };
    };
    let cutoff = anchor.saturating_sub(window.minutes() * 60_000);
    let selected = samples
        .iter()
        .filter(|sample| {
            let completed_at_ms = sample.stamp.completed_at_ms();
            completed_at_ms >= cutoff && completed_at_ms <= anchor
        })
        .collect::<Vec<_>>();
    let oldest = selected
        .iter()
        .map(|sample| sample.stamp.completed_at_ms())
        .min()
        .unwrap_or(anchor);
    let raw = selected.into_iter().map(value).collect::<Vec<_>>();
    MetricWindow {
        values: Rc::from(downsample(&raw)),
        covered_ms: anchor.saturating_sub(oldest),
        readouts: raw_readouts(&raw),
    }
}

fn raw_readouts(samples: &[f32]) -> MetricReadouts {
    let latest = samples
        .iter()
        .enumerate()
        .rev()
        .find(|(_, value)| value.is_finite())
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

fn downsample(samples: &[f32]) -> Vec<f32> {
    if samples.len() <= MAX_GRAPH_POINTS {
        return samples.to_vec();
    }
    let chunk_size = samples.len().div_ceil(MAX_GRAPH_POINTS);
    samples
        .chunks(chunk_size)
        .map(|chunk| {
            // Peak-preserving finite buckets may never bridge a typed gap.
            // Any gap in the bucket keeps the bucket a gap; readout latest/peak
            // are computed separately from the raw authoritative window.
            if chunk.iter().any(|value| !value.is_finite()) {
                f32::NAN
            } else {
                chunk.iter().copied().fold(f32::NAN, f32::max)
            }
        })
        .collect()
}

fn bytes_per_sec_to_mib(value: u64) -> f32 {
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

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_timeline_tests.rs"]
mod tests;
