//! Renderer-neutral live graph projection over the correlated telemetry store.
//!
//! This is a read capability. It never accepts point-in-time snapshots and
//! therefore cannot create a second history authority beside the correlation-
//! gated ingestor. Missing measurements remain `NaN` gaps in the returned
//! graph vectors. Per-device rings are additionally generation-scoped at this
//! read edge: a ring serves a window only at the generation it was reset for,
//! so a projection row and a ring that briefly disagree across a batch
//! boundary yield an honest empty window instead of the previous device
//! instance's samples.
//!
//! Series reads go through one scope-aware model: every [`MetricSeries`]
//! declares the domain it lives in ([`SeriesScope`]), and
//! [`LiveGraphHistory::resolve_series`] routes a [`ChartSeriesQuery`] to the
//! host ring or the per-device ring of the same accepted fact. Wrong-domain
//! queries are explicit [`ChartSeriesError`]s, never silent redirects.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use taskmanager_core::{DeviceId, GpuEngineMetricPoint};

use crate::{
    CorrelatedMetricHistory, CorrelatedMetricSample, CorrelatedSystemTelemetryIngestor,
    DeviceMetricHistory, GpuMetricPoint, TelemetryStore,
};

pub const MIN_HISTORY_CAPACITY: usize = 10;
pub const MAX_HISTORY_CAPACITY: usize = 600;
pub const DEFAULT_HISTORY_CAPACITY: usize = 64;

#[must_use]
pub const fn clamp_history_capacity(capacity: usize) -> usize {
    if capacity < MIN_HISTORY_CAPACITY {
        MIN_HISTORY_CAPACITY
    } else if capacity > MAX_HISTORY_CAPACITY {
        MAX_HISTORY_CAPACITY
    } else {
        capacity
    }
}

/// The device family a per-device chart series resolves through. This is a
/// resolution vocabulary for the ring maps, not a hardware taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceDomain {
    Storage,
    Network,
    Gpu,
}

/// The resolution domain every [`MetricSeries`] declares for itself. The
/// scope is the authority the read edge enforces: a query may only ask for a
/// series inside a domain the series actually lives in, and a wrong-domain
/// query is rejected instead of being redirected to the other domain's ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesScope {
    /// Exactly one host-wide ring; carrying a device identity is a scope
    /// error.
    Host,
    /// Per-device rings in one [`DeviceDomain`]; the device identity is
    /// required and no host aggregate exists for this series.
    Device(DeviceDomain),
    /// A host aggregate ring plus per-device rings built from the same
    /// accepted observation — one fact, two projections. Both legs resolve:
    /// the host leg is the aggregate (`*_total` / `*_mean`), the device leg
    /// is the per-device ring.
    HostAndDevice(DeviceDomain),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricSeries {
    CpuUsagePercent,
    MemoryUsagePercent,
    DiskBytesPerSec,
    NetworkBytesPerSec,
    GpuUsagePercent,
    CpuTemperatureC,
    CpuFrequencyMhz,
    CpuPowerW,
    /// Per-disk active-time percentage (0..=100) from the generation-scoped
    /// device ring — the persisted `storage-activity-pct` authority. The
    /// series is device-only by scope: the store observes per-device facts
    /// and fabricates no host aggregate for them.
    DiskActiveTimePct,
}

impl MetricSeries {
    /// Every series exactly once, in the canonical order that backs
    /// [`Self::slot`]. A new variant must extend this array; the slot
    /// round-trip test keeps enum and array in lockstep.
    pub const ALL: [Self; 9] = [
        Self::CpuUsagePercent,
        Self::MemoryUsagePercent,
        Self::DiskBytesPerSec,
        Self::NetworkBytesPerSec,
        Self::GpuUsagePercent,
        Self::CpuTemperatureC,
        Self::CpuFrequencyMhz,
        Self::CpuPowerW,
        Self::DiskActiveTimePct,
    ];

    /// The resolution domain this series can be read from; see
    /// [`SeriesScope`].
    #[must_use]
    pub const fn scope(self) -> SeriesScope {
        match self {
            Self::CpuUsagePercent
            | Self::MemoryUsagePercent
            | Self::CpuTemperatureC
            | Self::CpuFrequencyMhz
            | Self::CpuPowerW => SeriesScope::Host,
            Self::DiskActiveTimePct => SeriesScope::Device(DeviceDomain::Storage),
            Self::DiskBytesPerSec => SeriesScope::HostAndDevice(DeviceDomain::Storage),
            Self::NetworkBytesPerSec => SeriesScope::HostAndDevice(DeviceDomain::Network),
            Self::GpuUsagePercent => SeriesScope::HostAndDevice(DeviceDomain::Gpu),
        }
    }

    /// Stable storage ordinal of this series, derived from [`Self::ALL`].
    /// Slot-keyed caches read this one function instead of hand-maintained
    /// numeric tables; ordinals are distinct and stay in `ALL` order, and
    /// the round-trip test pins `ALL[series.slot()] == series` for every
    /// variant.
    #[must_use]
    pub fn slot(self) -> usize {
        Self::ALL
            .into_iter()
            .position(|candidate| candidate == self)
            .unwrap_or(Self::ALL.len())
    }
}

/// One scope-aware live-chart read: a series plus the device identity and
/// generation its scope demands. Build it with [`ChartSeriesQuery::host`] or
/// [`ChartSeriesQuery::device`]; [`LiveGraphHistory::resolve_series`]
/// validates the pair against [`MetricSeries::scope`] and never redirects a
/// query to the other domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChartSeriesQuery<'a> {
    series: MetricSeries,
    device: Option<&'a DeviceId>,
    /// The viewed device instance's generation. The device leg serves a ring
    /// only at the generation that ring was reset for; `0` (an unbound
    /// projection row) serves no device ring. Host legs ignore this field.
    generation: u64,
}

impl<'a> ChartSeriesQuery<'a> {
    /// Read the series in its host domain — the host ring, or the host
    /// aggregate of a [`SeriesScope::HostAndDevice`] series. Host-only
    /// series must use this form.
    #[must_use]
    pub const fn host(series: MetricSeries) -> Self {
        Self {
            series,
            device: None,
            generation: 0,
        }
    }

    /// Read the series for one device identity at the viewed instance's
    /// generation. Device and dual-scoped series must use this form; a
    /// device the store has never accepted resolves to an empty window,
    /// never fabricated samples, and a ring from another generation of the
    /// same identity is refused the same way.
    #[must_use]
    pub const fn device(series: MetricSeries, device: &'a DeviceId, generation: u64) -> Self {
        Self {
            series,
            device: Some(device),
            generation,
        }
    }

    /// The requested series.
    #[must_use]
    pub const fn series(self) -> MetricSeries {
        self.series
    }

    /// The requested device identity, when the query is device-scoped.
    #[must_use]
    pub const fn device_identity(self) -> Option<&'a DeviceId> {
        self.device
    }
}

/// A chart query asked a series for a window in a domain it does not have.
/// The read edge rejects the query instead of silently retargeting it to the
/// other domain's ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartSeriesError {
    /// A host-only series was queried with a device identity.
    DeviceIdentityOnHostSeries { series: MetricSeries },
    /// A device-domain series was queried without a device identity.
    MissingDeviceIdentity {
        series: MetricSeries,
        domain: DeviceDomain,
    },
}

/// Cloneable read projection for every live graph family.
#[derive(Clone)]
pub struct LiveGraphHistory {
    store: Arc<TelemetryStore>,
    projection_capacity: Arc<AtomicUsize>,
}

impl std::fmt::Debug for LiveGraphHistory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LiveGraphHistory")
            .field("capacity", &self.capacity())
            .field("revision", &self.revision())
            .finish_non_exhaustive()
    }
}

impl Default for LiveGraphHistory {
    fn default() -> Self {
        Self::shared(DEFAULT_HISTORY_CAPACITY).0
    }
}

impl LiveGraphHistory {
    /// Construct the one read store and its separate correlation-gated writer.
    #[must_use]
    pub fn shared(projection_capacity: usize) -> (Self, CorrelatedSystemTelemetryIngestor) {
        // The physical rings retain the maximum product window. A preference
        // changes only the read tail, so growing it never replaces authority or
        // fabricates points and shrinking does not mutate accepted telemetry.
        let (store, ingestor) =
            TelemetryStore::shared_with_correlated_ingestion(MAX_HISTORY_CAPACITY);
        (Self::from_store(store, projection_capacity), ingestor)
    }

    #[must_use]
    pub fn from_store(store: Arc<TelemetryStore>, projection_capacity: usize) -> Self {
        Self {
            store,
            projection_capacity: Arc::new(AtomicUsize::new(clamp_history_capacity(
                projection_capacity,
            ))),
        }
    }

    #[must_use]
    pub fn store(&self) -> &Arc<TelemetryStore> {
        &self.store
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.projection_capacity.load(Ordering::Relaxed)
    }

    pub fn set_capacity(&self, capacity: usize) {
        self.projection_capacity
            .store(clamp_history_capacity(capacity), Ordering::Relaxed);
    }

    /// Cache key spanning accepted writes and the selected projection tail.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.store
            .system_history
            .revision()
            .wrapping_mul(1_024)
            .wrapping_add(self.capacity() as u64)
    }

    /// Resolve one chart series through the scope model — the single read
    /// entry for host rings, host aggregates and per-device rings alike.
    /// A wrong-domain query returns a typed [`ChartSeriesError`]; the
    /// resolver never substitutes the other domain's window.
    pub fn resolve_series(
        &self,
        query: ChartSeriesQuery<'_>,
    ) -> Result<Vec<f32>, ChartSeriesError> {
        match (query.series.scope(), query.device) {
            (SeriesScope::Host, Some(_)) => Err(ChartSeriesError::DeviceIdentityOnHostSeries {
                series: query.series,
            }),
            (SeriesScope::Device(domain), None) => Err(ChartSeriesError::MissingDeviceIdentity {
                series: query.series,
                domain,
            }),
            _ => Ok(self.window_for(query)),
        }
    }

    /// Window lookup for an already scope-validated query. The trailing arm
    /// covers pairs [`Self::resolve_series`] rejects first and the legacy
    /// accessors never build; it keeps this dispatch total and panic-free.
    fn window_for(&self, query: ChartSeriesQuery<'_>) -> Vec<f32> {
        let history = &self.store.system_history;
        match (query.series, query.device) {
            (MetricSeries::CpuUsagePercent, None) => self.f32_history(history.cpu_usage()),
            (MetricSeries::MemoryUsagePercent, None) => self.f32_history(history.memory_usage()),
            (MetricSeries::DiskBytesPerSec, None) => self.u64_history(history.storage_rate_total()),
            (MetricSeries::DiskBytesPerSec, Some(device)) => {
                self.u64_device(history.storage_rate(device), query.generation)
            }
            (MetricSeries::NetworkBytesPerSec, None) => {
                self.u64_history(history.network_rate_total())
            }
            (MetricSeries::NetworkBytesPerSec, Some(device)) => {
                self.u64_device(history.network_rate(device), query.generation)
            }
            (MetricSeries::GpuUsagePercent, None) => self.f32_history(history.gpu_usage_mean()),
            (MetricSeries::GpuUsagePercent, Some(device)) => {
                self.f32_device(history.gpu_usage(device), query.generation)
            }
            (MetricSeries::CpuTemperatureC, None) => self.f32_history(history.cpu_temperature()),
            (MetricSeries::CpuFrequencyMhz, None) => self.u64_history(history.cpu_frequency_mhz()),
            (MetricSeries::CpuPowerW, None) => self.f32_history(history.cpu_power_w()),
            (MetricSeries::DiskActiveTimePct, Some(device)) => {
                self.f32_device(history.storage_activity(device), query.generation)
            }
            (
                MetricSeries::CpuUsagePercent
                | MetricSeries::MemoryUsagePercent
                | MetricSeries::CpuTemperatureC
                | MetricSeries::CpuFrequencyMhz
                | MetricSeries::CpuPowerW,
                Some(_),
            )
            | (MetricSeries::DiskActiveTimePct, None) => Vec::new(),
        }
    }

    /// Legacy host-domain window kept for existing callers; new code uses
    /// [`Self::resolve_series`] with an explicit [`ChartSeriesQuery`].
    ///
    /// Thin wrapper over the host leg of the scope model. A device-only
    /// series has no host window, so the legacy infallible signature returns
    /// an empty window for it (the only reachable error — a host query never
    /// carries a device identity); [`Self::resolve_series`] rejects the same
    /// query with [`ChartSeriesError::MissingDeviceIdentity`] instead.
    #[must_use]
    pub fn series(&self, series: MetricSeries) -> Vec<f32> {
        self.resolve_series(ChartSeriesQuery::host(series))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn series_sample_count(&self, series: MetricSeries) -> usize {
        self.series(series)
            .into_iter()
            .filter(|value| value.is_finite())
            .count()
    }

    #[must_use]
    pub fn per_core_usage_series(&self) -> Vec<Vec<f32>> {
        self.store
            .system_history
            .cpu_core_usage()
            .into_iter()
            .map(|history| self.f32_history(history))
            .collect()
    }

    /// The correlated host swap-usage window (percent, oldest..newest, NaN =
    /// gap) — the same `SystemHistory` rings every frontend reads. Frontends
    /// whose live-graph handle is this type (Iced) read swap through here
    /// instead of growing a second store handle just for the Memory page.
    #[must_use]
    pub fn swap_usage_pct(&self) -> Vec<f32> {
        self.f32_history(self.store.system_history.swap_usage())
    }

    /// The disk's read+write throughput window in bytes/sec — the device leg
    /// of [`MetricSeries::DiskBytesPerSec`]; the host leg is the summed
    /// aggregate of the same accepted observation. The read is
    /// generation-scoped: `0` and any generation but the ring's own yield an
    /// honest empty window, so a previous physical instance's curve can never
    /// cross a hot-plug boundary into the viewed row's chart.
    #[must_use]
    pub fn disk_bytes_per_sec_for(&self, device_id: &str, generation: u64) -> Vec<f32> {
        self.window_for(ChartSeriesQuery::device(
            MetricSeries::DiskBytesPerSec,
            &device(device_id),
            generation,
        ))
    }

    /// The disk's read-direction window in bytes/sec — the split-direction
    /// companion of [`Self::disk_bytes_per_sec_for`], from the same accepted
    /// events with its own per-direction gaps (`NaN`, never a fabricated 0)
    /// and the same read-time generation scope.
    #[must_use]
    pub fn disk_read_bytes_per_sec_for(&self, device_id: &str, generation: u64) -> Vec<f32> {
        self.u64_device(
            self.store
                .system_history
                .storage_read_rate(&device(device_id)),
            generation,
        )
    }

    /// The disk's write-direction window in bytes/sec; see
    /// [`Self::disk_read_bytes_per_sec_for`].
    #[must_use]
    pub fn disk_write_bytes_per_sec_for(&self, device_id: &str, generation: u64) -> Vec<f32> {
        self.u64_device(
            self.store
                .system_history
                .storage_write_rate(&device(device_id)),
            generation,
        )
    }

    /// The disk's SMART temperature window in °C, generation-scoped like
    /// every per-device curve so two physical disks can never share a
    /// detail-page trend.
    #[must_use]
    pub fn disk_temperature_c_for(&self, device_id: &str, generation: u64) -> Vec<f32> {
        self.f32_device(
            self.store
                .system_history
                .storage_temperature_c(&device(device_id)),
            generation,
        )
    }

    /// The disk's active-time percentage window (0..=100) from its
    /// generation-scoped activity ring — the device-only resolution of
    /// [`MetricSeries::DiskActiveTimePct`]. A device the store has never
    /// accepted, an unbound `0` generation, or any generation but the
    /// ring's own resolves to an empty window and a missing sample stays
    /// `NaN` — never a fabricated 0%.
    #[must_use]
    pub fn disk_active_time_pct_for(&self, device_id: &str, generation: u64) -> Vec<f32> {
        self.window_for(ChartSeriesQuery::device(
            MetricSeries::DiskActiveTimePct,
            &device(device_id),
            generation,
        ))
    }

    /// The adapter's read+write throughput window in bytes/sec — the device
    /// leg of [`MetricSeries::NetworkBytesPerSec`]; the host leg is the
    /// summed aggregate of the same accepted observation. Generation-scoped
    /// like every per-device curve.
    #[must_use]
    pub fn network_bytes_per_sec_for(&self, device_id: &str, generation: u64) -> Vec<f32> {
        self.window_for(ChartSeriesQuery::device(
            MetricSeries::NetworkBytesPerSec,
            &device(device_id),
            generation,
        ))
    }

    /// The adapter's receive-direction window in bytes/sec — the
    /// split-direction companion of [`Self::network_bytes_per_sec_for`], from
    /// the same accepted events with its own per-direction gaps (`NaN`, never
    /// a fabricated 0) and the same read-time generation scope.
    #[must_use]
    pub fn network_rx_bytes_per_sec_for(&self, device_id: &str, generation: u64) -> Vec<f32> {
        self.u64_device(
            self.store
                .system_history
                .network_rx_rate(&device(device_id)),
            generation,
        )
    }

    /// The adapter's transmit-direction window in bytes/sec; see
    /// [`Self::network_rx_bytes_per_sec_for`].
    #[must_use]
    pub fn network_tx_bytes_per_sec_for(&self, device_id: &str, generation: u64) -> Vec<f32> {
        self.u64_device(
            self.store
                .system_history
                .network_tx_rate(&device(device_id)),
            generation,
        )
    }

    /// The GPU's utilization window — the device leg of
    /// [`MetricSeries::GpuUsagePercent`]; the host leg is the mean aggregate
    /// of the same accepted observation. Generation-scoped like every
    /// per-device curve: `0` and a mismatched generation yield an honest
    /// empty window instead of the previous instance's samples.
    #[must_use]
    pub fn gpu_usage_pct_for(&self, device_id: &str, generation: u64) -> Vec<f32> {
        self.window_for(ChartSeriesQuery::device(
            MetricSeries::GpuUsagePercent,
            &device(device_id),
            generation,
        ))
    }

    /// The GPU's per-engine utilization window — the engine ring of the same
    /// device domain, with the same read-time generation scope: an unbound
    /// `0` or a ring from another generation of the identity yields an
    /// honest empty window.
    #[must_use]
    pub fn gpu_engine_usage_pct_for(
        &self,
        device_id: &str,
        generation: u64,
        engine_name: &str,
    ) -> Vec<f32> {
        let samples = generation_scoped(
            self.store
                .system_history
                .gpu_engine_metrics(&device(device_id)),
            generation,
        )
        .map_or_else(Vec::new, |history| history.samples());
        self.tail(
            samples
                .iter()
                .map(|sample| engine_value(sample, engine_name))
                .collect(),
        )
    }

    /// The GPU's typed-point window folded per sample by the caller's
    /// `project` — the single sampling read every GPU chart-metric family
    /// goes through (the ADR-034 chart families dispatch over it with their
    /// shared `value` fold). The read is generation-scoped like the rings it
    /// reads: a `0` generation and a ring generation that does not match the
    /// viewed device's generation yield an honest empty window, so samples
    /// from a previous device instance can never cross a hot-plug boundary
    /// even when the projection row and the ring briefly disagree. Missing
    /// measurements stay `NaN` gaps — never fabricated zeros — and the
    /// visible tail follows this history's capacity like every other
    /// live-graph window.
    #[must_use]
    pub fn gpu_metric_point_series_for(
        &self,
        device_id: &str,
        generation: u64,
        project: impl Fn(&GpuMetricPoint) -> Option<f32>,
    ) -> Vec<f32> {
        let samples = generation_scoped(
            self.store.system_history.gpu_metrics(&device(device_id)),
            generation,
        )
        .map_or_else(Vec::new, |history| history.samples());
        self.tail(
            samples
                .iter()
                .map(|sample| graph_value(sample, &project))
                .collect(),
        )
    }

    #[must_use]
    pub fn fan_rpm_for(&self, channel_id: &str) -> Vec<f32> {
        self.f32_dynamic_device(self.store.dynamic_history.fan_rpm(&device(channel_id)))
    }

    #[must_use]
    pub fn fan_temperature_c_for(&self, channel_id: &str) -> Vec<f32> {
        self.f32_dynamic_device(
            self.store
                .dynamic_history
                .fan_temperature_c(&device(channel_id)),
        )
    }

    #[must_use]
    pub fn battery_capacity_pct_for(&self, device_id: &str) -> Vec<f32> {
        self.f32_dynamic_device(
            self.store
                .dynamic_history
                .battery_capacity_pct(&device(device_id)),
        )
    }

    #[must_use]
    pub fn battery_power_w_for(&self, device_id: &str) -> Vec<f32> {
        self.f32_dynamic_device(
            self.store
                .dynamic_history
                .battery_power_w(&device(device_id)),
        )
    }

    fn f32_history(&self, history: CorrelatedMetricHistory<f32>) -> Vec<f32> {
        self.tail(history.samples().iter().map(f32_value).collect())
    }

    fn u64_history(&self, history: CorrelatedMetricHistory<u64>) -> Vec<f32> {
        self.tail(history.samples().iter().map(u64_value).collect())
    }

    fn f32_device(&self, history: Option<DeviceMetricHistory<f32>>, generation: u64) -> Vec<f32> {
        self.tail(
            generation_scoped(history, generation)
                .map_or_else(Vec::new, |history| history.samples())
                .iter()
                .map(f32_value)
                .collect(),
        )
    }

    fn u64_device(&self, history: Option<DeviceMetricHistory<u64>>, generation: u64) -> Vec<f32> {
        self.tail(
            generation_scoped(history, generation)
                .map_or_else(Vec::new, |history| history.samples())
                .iter()
                .map(u64_value)
                .collect(),
        )
    }

    /// Dynamic device histories (power supplies, sensor channels) still read
    /// by identity alone on this legacy leg; the generation discipline is the
    /// system-domain contract and the forward rule for any new per-device
    /// read edge.
    fn f32_dynamic_device(&self, history: Option<DeviceMetricHistory<f32>>) -> Vec<f32> {
        self.tail(
            history
                .map_or_else(Vec::new, |history| history.samples())
                .iter()
                .map(f32_value)
                .collect(),
        )
    }

    fn tail(&self, samples: Vec<f32>) -> Vec<f32> {
        let keep = self.capacity();
        let skip = samples.len().saturating_sub(keep);
        samples.into_iter().skip(skip).collect()
    }
}

fn device(id: &str) -> DeviceId {
    DeviceId::new(id.to_owned())
}

/// A device ring serves a read only at the generation it was reset for, and
/// `0` is never a servable generation (an unbound projection row must not
/// inherit any ring's samples).
fn ring_generation_matches(ring_generation: u64, requested: u64) -> bool {
    requested != 0 && ring_generation == requested
}

/// One discipline for every system-domain per-device read edge: keep the
/// ring only when it was reset for the requested generation. This is the
/// read-side half of the device curve contract — the write side swaps rings
/// inside the ingestion transaction, this side refuses the window whenever
/// the projection row and the ring briefly disagree across a batch boundary.
fn generation_scoped<T>(
    history: Option<DeviceMetricHistory<T>>,
    requested: u64,
) -> Option<DeviceMetricHistory<T>> {
    history.filter(|history| ring_generation_matches(history.generation(), requested))
}

fn f32_value(sample: &CorrelatedMetricSample<f32>) -> f32 {
    match (sample.measured_at_ms, sample.value) {
        (Some(_), Some(value)) if value.is_finite() => value,
        _ => f32::NAN,
    }
}

fn u64_value(sample: &CorrelatedMetricSample<u64>) -> f32 {
    match (sample.measured_at_ms, sample.value) {
        (Some(_), Some(value)) => u64_as_f32(value),
        _ => f32::NAN,
    }
}

fn graph_value<T>(sample: &CorrelatedMetricSample<T>, project: &impl Fn(&T) -> Option<f32>) -> f32 {
    match (
        sample.measured_at_ms,
        sample.value.as_ref().and_then(project),
    ) {
        (Some(_), Some(value)) if value.is_finite() => value,
        _ => f32::NAN,
    }
}

fn engine_value(sample: &CorrelatedMetricSample<GpuEngineMetricPoint>, name: &str) -> f32 {
    graph_value(sample, &|point| {
        point
            .engines
            .iter()
            .find(|engine| engine.name == name)
            .map(|engine| engine.utilization_pct)
    })
}

fn u64_as_f32(value: u64) -> f32 {
    bounded_graph_f32(u64_as_f64(value))
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
#[path = "../tests/headless/live_graph.rs"]
mod tests;
