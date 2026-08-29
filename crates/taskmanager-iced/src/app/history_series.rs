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

use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::trend::{self, TrendSeries};

use super::IcedApp;

#[derive(Clone)]
struct Entry {
    revision: u64,
    capacity: usize,
    samples: Rc<[f32]>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum DeviceSeriesKind {
    /// The host-wide swap-usage window (percent) — host-scoped, keyed once.
    SwapUsedPct,
    DiskBytesPerSec,
    DiskActiveTimePct,
    /// The SMART temperature window for the Disk page's temperature-trend
    /// stat row (GPUI `storage_temperature_samples` parity).
    DiskTemperatureC,
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
    fn slot(series: TrendSeries) -> usize {
        match series {
            TrendSeries::CpuUsagePercent => 0,
            TrendSeries::MemoryUsagePercent => 1,
            TrendSeries::DiskBytesPerSec => 2,
            TrendSeries::NetworkBytesPerSec => 3,
            TrendSeries::GpuUsagePercent => 4,
            TrendSeries::CpuTemperatureC => 5,
            TrendSeries::CpuFrequencyMhz => 6,
            TrendSeries::CpuPowerW => 7,
            TrendSeries::DiskActiveTimePct => 8,
        }
    }

    pub(super) fn get(
        &mut self,
        shell: &ShellApp,
        revision: u64,
        series: TrendSeries,
    ) -> Rc<[f32]> {
        let slot = Self::slot(series);
        if let Some(entry) = self.entries.get(slot).and_then(Option::as_ref)
            && entry.revision == revision
            && entry.capacity == shell.history.capacity()
        {
            return Rc::clone(&entry.samples);
        }

        let samples = Rc::from(trend::window(&shell.history, series).into_boxed_slice());
        if self.entries.len() <= slot {
            self.entries.resize_with(slot + 1, || None);
        }
        self.entries[slot] = Some(Entry {
            revision,
            capacity: shell.history.capacity(),
            samples: Rc::clone(&samples),
        });
        samples
    }

    pub(super) fn core(&mut self, shell: &ShellApp, revision: u64) -> Rc<Vec<Rc<[f32]>>> {
        if let Some(entry) = &self.core
            && entry.revision == revision
            && entry.capacity == shell.history.capacity()
        {
            return Rc::clone(&entry.samples);
        }
        let samples = Rc::new(
            trend::per_core_usage_percent(&shell.history)
                .into_iter()
                .map(|window| Rc::from(window.into_boxed_slice()))
                .collect(),
        );
        self.core = Some(CoreEntry {
            revision,
            capacity: shell.history.capacity(),
            samples: Rc::clone(&samples),
        });
        samples
    }

    fn device(
        &mut self,
        shell: &ShellApp,
        revision: u64,
        key: DeviceSeriesKey,
        load: impl FnOnce(&ShellApp) -> Vec<f32>,
    ) -> Rc<[f32]> {
        if self.device_revision != Some(revision) {
            self.device_revision = Some(revision);
            self.device_entries.clear();
        }
        if let Some(samples) = self.device_entries.get(&key) {
            return Rc::clone(samples);
        }
        let samples = Rc::from(load(shell).into_boxed_slice());
        self.device_entries.insert(key, Rc::clone(&samples));
        samples
    }

    pub(crate) fn cached_device(
        &mut self,
        shell: &ShellApp,
        revision: u64,
        key: DeviceSeriesKey,
        load: impl FnOnce(&ShellApp) -> Vec<f32>,
    ) -> Rc<[f32]> {
        self.device(shell, revision, key, load)
    }
}

impl IcedApp {
    /// Return a shared contiguous series snapshot for the current Iced data
    /// epoch. Cache hits clone only the `Rc`; the bounded `VecDeque`→slice copy
    /// happens once per metric after a real refresh/capacity change.
    #[must_use]
    pub(crate) fn cached_metric_series(&self, series: TrendSeries) -> Rc<[f32]> {
        self.projection_caches.metric_series(&self.shell, series)
    }

    /// Shared per-core windows for the CPU detail grid. The outer vector is
    /// retained too, so an idle frame clones neither the bounded windows nor a
    /// new list of core handles.
    #[must_use]
    pub(crate) fn cached_per_core_usage_series(&self) -> Rc<Vec<Rc<[f32]>>> {
        self.projection_caches.per_core_series(&self.shell)
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
            |shell| shell.history.disk_bytes_per_sec_for(device_id, generation),
        )
    }

    /// The host-wide swap-usage window (percent) for the Memory page's swap
    /// headline chart (GPUI `swap_usage` parity): revision-keyed like every
    /// other window, read through the live-graph accessor over the same
    /// `SystemHistory` rings.
    #[must_use]
    pub(crate) fn cached_swap_series(&self) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(DeviceSeriesKind::SwapUsedPct, "host", "", ""),
            |shell| shell.history.swap_usage_pct(),
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
            |shell| {
                shell
                    .history
                    .disk_active_time_pct_for(device_id, generation)
            },
        )
    }

    /// The disk's SMART temperature window for the stats-rail trend row (GPUI
    /// `storage_temperature_samples` parity); generation-scoped like every
    /// disk family.
    #[must_use]
    pub(crate) fn cached_disk_temperature_series(
        &self,
        device_id: &str,
        generation: u64,
    ) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(
                DeviceSeriesKind::DiskTemperatureC,
                device_id,
                "",
                &generation.to_string(),
            ),
            |shell| shell.history.disk_temperature_c_for(device_id, generation),
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
            |shell| {
                shell
                    .history
                    .network_bytes_per_sec_for(device_id, generation)
            },
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
            |shell| shell.history.gpu_usage_pct_for(device_id, generation),
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
            |shell| {
                taskmanager_shell::presentation::gpu_chart_metric::gpu_chart_metric_history(
                    &shell.history,
                    device_id,
                    generation,
                    metric,
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
            |shell| {
                shell
                    .history
                    .gpu_engine_usage_pct_for(device_id, generation, engine_name)
            },
        )
    }

    #[must_use]
    pub(crate) fn cached_fan_series(&self, channel_id: &str) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(DeviceSeriesKind::FanRpm, channel_id, "", ""),
            |shell| shell.history.fan_rpm_for(channel_id),
        )
    }

    #[must_use]
    pub(crate) fn cached_fan_temperature_series(&self, channel_id: &str) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(DeviceSeriesKind::FanTemperatureC, channel_id, "", ""),
            |shell| shell.history.fan_temperature_c_for(channel_id),
        )
    }

    #[must_use]
    pub(crate) fn cached_battery_series(&self, id: &str) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(DeviceSeriesKind::BatteryCapacityPercent, id, "", ""),
            |shell| shell.history.battery_capacity_pct_for(id),
        )
    }

    #[must_use]
    pub(crate) fn cached_battery_power_series(&self, id: &str) -> Rc<[f32]> {
        self.cached_device_series(
            DeviceSeriesKey::new(DeviceSeriesKind::BatteryPowerW, id, "", ""),
            |shell| shell.history.battery_power_w_for(id),
        )
    }

    fn cached_device_series(
        &self,
        key: DeviceSeriesKey,
        load: impl FnOnce(&ShellApp) -> Vec<f32>,
    ) -> Rc<[f32]> {
        self.projection_caches.device_series(&self.shell, key, load)
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/history_series_tests.rs"]
mod tests;
