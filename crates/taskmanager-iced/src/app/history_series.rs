//! Shared history snapshots for Iced Canvas programs.
//!
//! `LiveGraphHistory` stores bounded `VecDeque`s, so its public read surfaces must
//! materialize contiguous `Vec`s. The Iced view can still avoid repeating that
//! copy on every frame: this cache keys system, per-core, and identity-specific
//! snapshots by the history revision and capacity, then shares them with all
//! charts built during the same data epoch. The revision covers the independent
//! system, sensor, and power feeds; a sensor update cannot leave a fan cache
//! stale merely because the shell's process refresh watermark did not move.

use std::collections::HashMap;
use std::rc::Rc;

use taskmanager_shell::history::{LiveGraphHistory, MetricSeries};

use super::IcedApp;

#[derive(Clone)]
struct Entry {
    revision: u64,
    capacity: usize,
    samples: Rc<[f32]>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum DeviceSeriesKind {
    DiskBytesPerSec,
    DiskActiveTimePct,
    NetworkBytesPerSec,
    GpuUsagePercent,
    /// The engine name rides the key's `second` field, like every series
    /// with a per-device discriminator.
    GpuEngineUsagePercent,
    /// One GPU headline-chart metric family (ADR-034): the device key's
    /// `second` field carries the family's id stem.
    GpuChartMetricSeries,
    FanRpm,
    FanTemperatureC,
    BatteryCapacityPercent,
    BatteryPowerW,
}

/// Every generation-scoped device series rides the viewed device generation
/// in the key's `third` field, so a row/ring generation flip can never serve
/// the previous device instance's cached window — the cache-side twin of the
/// store's read-time generation scope.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct DeviceSeriesKey {
    kind: DeviceSeriesKind,
    first: String,
    second: String,
    third: String,
}

impl DeviceSeriesKey {
    pub(crate) fn new(kind: DeviceSeriesKind, first: &str, second: &str, third: &str) -> Self {
        Self {
            kind,
            first: first.to_owned(),
            second: second.to_owned(),
            third: third.to_owned(),
        }
    }
}

#[derive(Clone)]
struct CoreEntry {
    revision: u64,
    capacity: usize,
    samples: Rc<Vec<Rc<[f32]>>>,
}

/// Bounded cache for the system-wide, per-core, and identity-specific graph
/// series. Every entry is keyed by the history revision, so the bounded
/// `VecDeque` → contiguous-slice copy happens once after a real history write,
/// not once per retained Iced frame or per device card.
#[derive(Clone, Default)]
pub(crate) struct HistorySeriesCache {
    entries: Vec<Option<Entry>>,
    core: Option<CoreEntry>,
    device_revision: Option<u64>,
    device_entries: HashMap<DeviceSeriesKey, Rc<[f32]>>,
}

impl HistorySeriesCache {
    fn slot(series: MetricSeries) -> usize {
        match series {
            MetricSeries::CpuUsagePercent => 0,
            MetricSeries::MemoryUsagePercent => 1,
            MetricSeries::DiskBytesPerSec => 2,
            MetricSeries::NetworkBytesPerSec => 3,
            MetricSeries::GpuUsagePercent => 4,
            MetricSeries::CpuTemperatureC => 5,
            MetricSeries::CpuFrequencyMhz => 6,
            MetricSeries::CpuPowerW => 7,
            MetricSeries::DiskActiveTimePct => 8,
        }
    }

    pub(super) fn get(
        &mut self,
        history: &LiveGraphHistory,
        revision: u64,
        series: MetricSeries,
    ) -> Rc<[f32]> {
        let slot = Self::slot(series);
        if let Some(entry) = self.entries.get(slot).and_then(Option::as_ref)
            && entry.revision == revision
            && entry.capacity == history.capacity()
        {
            return Rc::clone(&entry.samples);
        }

        let samples = Rc::from(history.series(series).into_boxed_slice());
        if self.entries.len() <= slot {
            self.entries.resize_with(slot + 1, || None);
        }
        self.entries[slot] = Some(Entry {
            revision,
            capacity: history.capacity(),
            samples: Rc::clone(&samples),
        });
        samples
    }

    pub(super) fn core(&mut self, history: &LiveGraphHistory, revision: u64) -> Rc<Vec<Rc<[f32]>>> {
        if let Some(entry) = &self.core
            && entry.revision == revision
            && entry.capacity == history.capacity()
        {
            return Rc::clone(&entry.samples);
        }
        let samples = Rc::new(
            history
                .per_core_usage_series()
                .into_iter()
                .map(|window| Rc::from(window.into_boxed_slice()))
                .collect(),
        );
        self.core = Some(CoreEntry {
            revision,
            capacity: history.capacity(),
            samples: Rc::clone(&samples),
        });
        samples
    }

    fn device(
        &mut self,
        history: &LiveGraphHistory,
        revision: u64,
        key: DeviceSeriesKey,
        load: impl FnOnce(&LiveGraphHistory) -> Vec<f32>,
    ) -> Rc<[f32]> {
        if self.device_revision != Some(revision) {
            self.device_revision = Some(revision);
            self.device_entries.clear();
        }
        if let Some(samples) = self.device_entries.get(&key) {
            return Rc::clone(samples);
        }
        let samples = Rc::from(load(history).into_boxed_slice());
        self.device_entries.insert(key, Rc::clone(&samples));
        samples
    }

    pub(crate) fn cached_device(
        &mut self,
        history: &LiveGraphHistory,
        revision: u64,
        key: DeviceSeriesKey,
        load: impl FnOnce(&LiveGraphHistory) -> Vec<f32>,
    ) -> Rc<[f32]> {
        self.device(history, revision, key, load)
    }
}

impl IcedApp {
    /// Return a shared contiguous series snapshot for the current Iced data
    /// epoch. Cache hits clone only the `Rc`; the bounded `VecDeque`→slice copy
    /// happens once per metric after a real refresh/capacity change.
    #[must_use]
    pub(crate) fn cached_metric_series(&self, series: MetricSeries) -> Rc<[f32]> {
        self.projection_caches
            .metric_series(&self.shell.history, series)
    }

    /// Shared per-core windows for the CPU detail grid. The outer vector is
    /// retained too, so an idle frame clones neither the bounded windows nor a
    /// new list of core handles.
    #[must_use]
    pub(crate) fn cached_per_core_usage_series(&self) -> Rc<Vec<Rc<[f32]>>> {
        self.projection_caches.per_core_series(&self.shell.history)
    }

    #[must_use]
    pub(crate) fn cached_disk_series(&self, device_id: &str, generation: u64) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(
                DeviceSeriesKind::DiskBytesPerSec,
                device_id,
                "",
                &generation.to_string(),
            ),
            |history| history.disk_bytes_per_sec_for(device_id, generation),
        )
    }

    /// The disk's active-time percentage window for the Disk page's secondary
    /// curve; the percent ring is distinct from every rate family, so it keeps
    /// its own `DeviceSeriesKind` entry.
    #[must_use]
    pub(crate) fn cached_disk_active_time_series(
        &self,
        device_id: &str,
        generation: u64,
    ) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(
                DeviceSeriesKind::DiskActiveTimePct,
                device_id,
                "",
                &generation.to_string(),
            ),
            |history| history.disk_active_time_pct_for(device_id, generation),
        )
    }

    #[must_use]
    pub(crate) fn cached_network_series(&self, device_id: &str, generation: u64) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(
                DeviceSeriesKind::NetworkBytesPerSec,
                device_id,
                "",
                &generation.to_string(),
            ),
            |history| history.network_bytes_per_sec_for(device_id, generation),
        )
    }

    #[must_use]
    pub(crate) fn cached_gpu_utilization_series(
        &self,
        device_id: &str,
        generation: u64,
    ) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(
                DeviceSeriesKind::GpuUsagePercent,
                device_id,
                "",
                &generation.to_string(),
            ),
            |history| history.gpu_usage_pct_for(device_id, generation),
        )
    }

    /// The GPU headline chart's selected series family (ADR-034): the
    /// shared shell dispatch through `gpu_chart_metric_history`, cached by
    /// device + family + device generation per data epoch like every other
    /// device window. The generation rides the key so a row/ring generation
    /// flip can never serve the previous device instance's cached window.
    #[must_use]
    pub(crate) fn cached_gpu_chart_metric_series(
        &self,
        device_id: &str,
        generation: u64,
        metric: taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric,
    ) -> Rc<[f32]> {
        let stem = metric.id_stem();
        self.cached_device_series(
            DeviceSeriesKey::new(
                DeviceSeriesKind::GpuChartMetricSeries,
                device_id,
                stem,
                &generation.to_string(),
            ),
            |history| {
                taskmanager_shell::presentation::gpu_chart_metric::gpu_chart_metric_history(
                    history, device_id, generation, metric,
                )
            },
        )
    }

    #[must_use]
    pub(crate) fn cached_gpu_engine_series(
        &self,
        device_id: &str,
        generation: u64,
        engine_name: &str,
    ) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(
                DeviceSeriesKind::GpuEngineUsagePercent,
                device_id,
                engine_name,
                &generation.to_string(),
            ),
            |history| history.gpu_engine_usage_pct_for(device_id, generation, engine_name),
        )
    }

    #[must_use]
    pub(crate) fn cached_fan_series(&self, channel_id: &str) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(DeviceSeriesKind::FanRpm, channel_id, "", ""),
            |history| history.fan_rpm_for(channel_id),
        )
    }

    #[must_use]
    pub(crate) fn cached_fan_temperature_series(&self, channel_id: &str) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(DeviceSeriesKind::FanTemperatureC, channel_id, "", ""),
            |history| history.fan_temperature_c_for(channel_id),
        )
    }

    #[must_use]
    pub(crate) fn cached_battery_series(&self, id: &str) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(DeviceSeriesKind::BatteryCapacityPercent, id, "", ""),
            |history| history.battery_capacity_pct_for(id),
        )
    }

    #[must_use]
    pub(crate) fn cached_battery_power_series(&self, id: &str) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(DeviceSeriesKind::BatteryPowerW, id, "", ""),
            |history| history.battery_power_w_for(id),
        )
    }

    fn cached_device_series(
        &self,
        key: DeviceSeriesKey,
        load: impl FnOnce(&LiveGraphHistory) -> Vec<f32>,
    ) -> Rc<[f32]> {
        self.projection_caches
            .device_series(&self.shell.history, key, load)
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/history_series_tests.rs"]
mod tests;
