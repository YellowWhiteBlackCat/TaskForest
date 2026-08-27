//! Correlation-gated, gap-aware histories for independent system domains.

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Producer};
use taskmanager_core::{DeviceId, GpuEngineMetricPoint, SystemObservationState};

mod device;
mod dynamic_history;
mod gpu;
mod ingest;

pub use dynamic_history::DynamicTelemetryHistory;
pub use gpu::GpuMetricPoint;
pub use ingest::CorrelatedSystemTelemetryIngestor;

/// Application correlation identity copied from one accepted domain event.
///
/// Construction is public because this inward adapter cannot depend on the
/// application crate. Append authority is instead held by
/// [`CorrelatedSystemTelemetryIngestor`], which is returned separately from the
/// read store. Revisions must increase independently within each domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CorrelatedTelemetryStamp {
    revision: NonZeroU64,
    completed_at_ms: u64,
}

impl CorrelatedTelemetryStamp {
    #[must_use]
    pub const fn from_accepted_event(revision: u64, completed_at_ms: u64) -> Option<Self> {
        match NonZeroU64::new(revision) {
            Some(revision) => Some(Self {
                revision,
                completed_at_ms,
            }),
            None => None,
        }
    }

    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision.get()
    }

    #[must_use]
    pub const fn completed_at_ms(self) -> u64 {
        self.completed_at_ms
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemHistoryDomain {
    Host,
    Cpu,
    Memory,
    Storage,
    Network,
    Gpu,
}

/// Dynamic device history is intentionally separate from the six system
/// telemetry domains and from static hardware inventory. Battery supplies and
/// hwmon channels can appear/disappear at runtime, so their histories are
/// generation-scoped read models rather than fixed snapshot fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicHistoryDomain {
    Power,
    Sensor,
}

/// Hard identity ceiling for each dynamic-device history family.
///
/// Partial/unavailable enumeration cannot prove that an older battery or
/// sensor disappeared, so those histories are retained. Once this ceiling is
/// reached, an unrecognized identity is rejected from history (while the
/// point-in-time snapshot remains available) instead of allowing perpetual
/// partial identity churn to grow memory without bound.
pub const MAX_DYNAMIC_HISTORY_IDENTITIES: usize = 256;

impl DynamicHistoryDomain {
    const fn index(self) -> usize {
        match self {
            Self::Power => 0,
            Self::Sensor => 1,
        }
    }
}

impl SystemHistoryDomain {
    const fn index(self) -> usize {
        match self {
            Self::Host => 0,
            Self::Cpu => 1,
            Self::Memory => 2,
            Self::Storage => 3,
            Self::Network => 4,
            Self::Gpu => 5,
        }
    }
}

/// Receipt proving what freshness state entered history for one accepted event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CorrelatedDomainReceipt {
    pub stamp: CorrelatedTelemetryStamp,
    pub state: SystemObservationState,
}

/// One gap-aware metric point. `None` is an explicit missing measurement.
#[derive(Clone, Debug, PartialEq)]
pub struct CorrelatedMetricSample<T> {
    pub stamp: CorrelatedTelemetryStamp,
    /// Actual sampling time from the typed observation state. This is `None`
    /// for stale, unavailable, and unknown observations.
    pub measured_at_ms: Option<u64>,
    pub value: Option<T>,
}

struct BoundedHistory<T> {
    capacity: usize,
    commit_gate: Arc<Mutex<()>>,
    buffer: Mutex<HeapRb<T>>,
}

impl<T> BoundedHistory<T> {
    fn new(capacity: usize, commit_gate: Arc<Mutex<()>>) -> Self {
        Self {
            capacity,
            commit_gate,
            buffer: Mutex::new(HeapRb::new(capacity)),
        }
    }

    /// Publish one value while the caller owns this history's domain commit
    /// gate. Keeping this operation distinct from [`Self::samples`] prevents a
    /// writer from recursively acquiring its own non-reentrant gate.
    fn push_in_transaction(&self, value: T) {
        let mut buffer = self
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if buffer.is_full() {
            let _ = buffer.try_pop();
        }
        let _ = buffer.try_push(value);
    }

    fn samples(&self) -> Vec<T>
    where
        T: Clone,
    {
        let _commit = lock_unpoisoned(&self.commit_gate);
        self.buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .cloned()
            .collect()
    }
}

/// Bounded typed metric history shared by readers and the ingestion capability.
#[derive(Clone)]
pub struct CorrelatedMetricHistory<T> {
    inner: Arc<BoundedHistory<CorrelatedMetricSample<T>>>,
}

impl<T> CorrelatedMetricHistory<T> {
    fn new(capacity: usize, commit_gate: Arc<Mutex<()>>) -> Self {
        Self {
            inner: Arc::new(BoundedHistory::new(capacity, commit_gate)),
        }
    }

    fn push(&self, stamp: CorrelatedTelemetryStamp, measured_at_ms: Option<u64>, value: Option<T>) {
        self.inner.push_in_transaction(CorrelatedMetricSample {
            stamp,
            measured_at_ms,
            value,
        });
    }

    /// Latest real sampling time while the caller already owns this ring's
    /// domain commit gate. This is deliberately not a public read path: taking
    /// the gate again from freshness fan-out would deadlock the transaction.
    fn latest_measured_at_in_transaction(&self) -> Option<u64> {
        self.inner
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .rev()
            .filter_map(|sample| sample.measured_at_ms)
            .next()
    }

    #[must_use]
    pub fn samples(&self) -> Vec<CorrelatedMetricSample<T>>
    where
        T: Clone,
    {
        self.inner.samples()
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }

    /// Stable identity of the underlying storage (the shared buffer's
    /// address). Derived-projection caches include it so two distinct
    /// histories that happen to agree on `(len, revision)` can never serve
    /// each other's cached vector.
    #[must_use]
    pub fn ring_id(&self) -> usize {
        std::sync::Arc::as_ptr(&self.inner) as usize
    }

    /// Content watermark: `(len, latest_revision)`. Two histories with the
    /// same watermark cannot differ, so derived projections (graph sample
    /// vectors) can key their caches on it and skip the sample clone entirely.
    /// Reads the lock without cloning any sample.
    #[must_use]
    pub fn watermark(&self) -> (usize, Option<u64>) {
        let _commit = lock_unpoisoned(&self.inner.commit_gate);
        let buffer = self
            .inner
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            buffer.occupied_len(),
            buffer
                .iter()
                .next_back()
                .map(|sample| sample.stamp.revision()),
        )
    }
}

/// Generation-scoped history for one stable device identity.
#[derive(Clone)]
pub struct DeviceMetricHistory<T> {
    generation: u64,
    metric: CorrelatedMetricHistory<T>,
}

impl<T> DeviceMetricHistory<T> {
    fn new(generation: u64, capacity: usize, commit_gate: Arc<Mutex<()>>) -> Self {
        Self {
            generation,
            metric: CorrelatedMetricHistory::new(capacity, commit_gate),
        }
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn samples(&self) -> Vec<CorrelatedMetricSample<T>>
    where
        T: Clone,
    {
        self.metric.samples()
    }

    /// Content watermark of the underlying series (see
    /// [`CorrelatedMetricHistory::watermark`]).
    #[must_use]
    pub fn watermark(&self) -> (usize, Option<u64>) {
        self.metric.watermark()
    }

    /// Storage identity of the underlying series (see
    /// [`CorrelatedMetricHistory::ring_id`]).
    #[must_use]
    pub fn ring_id(&self) -> usize {
        self.metric.ring_id()
    }
}

struct CorrelatedSystemTelemetryHistoryInner {
    capacity: usize,
    /// One transaction gate per independently scheduled fixed domain. Writers
    /// in different domains remain concurrent; readers of a domain ring wait
    /// until that domain's complete fan-out and persistence commit finishes.
    system_commit_gates: [Arc<Mutex<()>>; 6],
    /// Power and sensor histories have independent correlation sequences and
    /// therefore independent commit gates as well.
    dynamic_commit_gates: [Arc<Mutex<()>>; 2],
    /// Monotonic mutation generation shared by every fixed and dynamic lane.
    /// Render caches use this instead of comparing unrelated domain revisions.
    revision: AtomicU64,
    last_revisions: Mutex<[Option<u64>; 6]>,
    dynamic_last_revisions: Mutex<[Option<u64>; 2]>,
    receipts: [BoundedHistory<CorrelatedDomainReceipt>; 6],
    uptime_secs: CorrelatedMetricHistory<u64>,
    process_count: CorrelatedMetricHistory<u64>,
    thread_count: CorrelatedMetricHistory<u64>,
    cpu_usage: CorrelatedMetricHistory<f32>,
    cpu_cores: Mutex<Vec<CorrelatedMetricHistory<f32>>>,
    /// Per-core temperature rings (°C), indexed by logical core. Same
    /// acceptance fan-out as `cpu_cores`; per-core sensors that the provider
    /// cannot resolve append explicit gaps, not fabricated values.
    cpu_core_temperatures: Mutex<Vec<CorrelatedMetricHistory<f32>>>,
    /// Per-core frequency rings (MHz), indexed by logical core.
    cpu_core_frequencies: Mutex<Vec<CorrelatedMetricHistory<u64>>>,
    cpu_temperature: CorrelatedMetricHistory<f32>,
    cpu_frequency_mhz: CorrelatedMetricHistory<u64>,
    cpu_power_w: CorrelatedMetricHistory<f32>,
    memory_usage: CorrelatedMetricHistory<f32>,
    swap_usage: CorrelatedMetricHistory<f32>,
    storage_activity: Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>,
    /// Per-device read+write bytes/sec; distinct from activity percentage.
    storage_rate: Mutex<HashMap<DeviceId, DeviceMetricHistory<u64>>>,
    /// Per-device read-direction bytes/sec — the split-direction companion of
    /// `storage_rate`, fed by the same accepted observation and lifecycle map
    /// so the two views never disagree. Live-graph only; not persisted (see
    /// [`CorrelatedSystemTelemetryHistory::storage_read_rate`]).
    storage_read_rate: Mutex<HashMap<DeviceId, DeviceMetricHistory<u64>>>,
    /// Per-device write-direction bytes/sec — see `storage_read_rate`.
    storage_write_rate: Mutex<HashMap<DeviceId, DeviceMetricHistory<u64>>>,
    /// Per-device SMART temperature in °C. Kept generation-scoped so two
    /// physical disks can never share a detail-page trend.
    storage_temperature_c: Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>,
    /// Host-wide sums/means used by renderer-neutral overview graphs.
    storage_rate_total: CorrelatedMetricHistory<u64>,
    network_rate: Mutex<HashMap<DeviceId, DeviceMetricHistory<u64>>>,
    /// Per-device receive-direction bytes/sec — the split-direction companion
    /// of `network_rate`, fed by the same accepted observation and lifecycle
    /// map. Live-graph only; not persisted (see `network_rx_rate` accessors).
    network_rx_rate: Mutex<HashMap<DeviceId, DeviceMetricHistory<u64>>>,
    /// Per-device transmit-direction bytes/sec — see `network_rx_rate`.
    network_tx_rate: Mutex<HashMap<DeviceId, DeviceMetricHistory<u64>>>,
    network_rate_total: CorrelatedMetricHistory<u64>,
    gpu_metrics: Mutex<HashMap<DeviceId, DeviceMetricHistory<GpuMetricPoint>>>,
    gpu_engine_metrics: Mutex<HashMap<DeviceId, DeviceMetricHistory<GpuEngineMetricPoint>>>,
    gpu_usage: Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>,
    gpu_usage_mean: CorrelatedMetricHistory<f32>,
    battery_capacity_pct: Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>,
    battery_power_w: Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>,
    /// Degradation health (full/design × 100) per power supply; the pure
    /// rule lives in core, this ring stores its current value or a gap.
    battery_health_pct: Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>,
    fan_rpm: Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>,
    fan_pwm_pct: Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>,
    fan_temperature_c: Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>,
}

impl CorrelatedSystemTelemetryHistoryInner {
    fn new(capacity: usize) -> Self {
        let system_commit_gates = std::array::from_fn(|_| Arc::new(Mutex::new(())));
        let dynamic_commit_gates = std::array::from_fn(|_| Arc::new(Mutex::new(())));
        let host_gate = system_commit_gates[SystemHistoryDomain::Host.index()].clone();
        let cpu_gate = system_commit_gates[SystemHistoryDomain::Cpu.index()].clone();
        let memory_gate = system_commit_gates[SystemHistoryDomain::Memory.index()].clone();
        let storage_gate = system_commit_gates[SystemHistoryDomain::Storage.index()].clone();
        let network_gate = system_commit_gates[SystemHistoryDomain::Network.index()].clone();
        let gpu_gate = system_commit_gates[SystemHistoryDomain::Gpu.index()].clone();
        Self {
            capacity,
            system_commit_gates: system_commit_gates.clone(),
            dynamic_commit_gates: dynamic_commit_gates.clone(),
            revision: AtomicU64::new(0),
            last_revisions: Mutex::new([None; 6]),
            dynamic_last_revisions: Mutex::new([None; 2]),
            receipts: std::array::from_fn(|index| {
                BoundedHistory::new(capacity, system_commit_gates[index].clone())
            }),
            uptime_secs: CorrelatedMetricHistory::new(capacity, host_gate.clone()),
            process_count: CorrelatedMetricHistory::new(capacity, host_gate.clone()),
            thread_count: CorrelatedMetricHistory::new(capacity, host_gate),
            cpu_usage: CorrelatedMetricHistory::new(capacity, cpu_gate.clone()),
            cpu_cores: Mutex::new(Vec::new()),
            cpu_core_temperatures: Mutex::new(Vec::new()),
            cpu_core_frequencies: Mutex::new(Vec::new()),
            cpu_temperature: CorrelatedMetricHistory::new(capacity, cpu_gate.clone()),
            cpu_frequency_mhz: CorrelatedMetricHistory::new(capacity, cpu_gate.clone()),
            cpu_power_w: CorrelatedMetricHistory::new(capacity, cpu_gate),
            memory_usage: CorrelatedMetricHistory::new(capacity, memory_gate.clone()),
            swap_usage: CorrelatedMetricHistory::new(capacity, memory_gate),
            storage_activity: Mutex::new(HashMap::new()),
            storage_rate: Mutex::new(HashMap::new()),
            storage_read_rate: Mutex::new(HashMap::new()),
            storage_write_rate: Mutex::new(HashMap::new()),
            storage_temperature_c: Mutex::new(HashMap::new()),
            storage_rate_total: CorrelatedMetricHistory::new(capacity, storage_gate),
            network_rate: Mutex::new(HashMap::new()),
            network_rx_rate: Mutex::new(HashMap::new()),
            network_tx_rate: Mutex::new(HashMap::new()),
            network_rate_total: CorrelatedMetricHistory::new(capacity, network_gate),
            gpu_metrics: Mutex::new(HashMap::new()),
            gpu_engine_metrics: Mutex::new(HashMap::new()),
            gpu_usage: Mutex::new(HashMap::new()),
            gpu_usage_mean: CorrelatedMetricHistory::new(capacity, gpu_gate),
            battery_capacity_pct: Mutex::new(HashMap::new()),
            battery_power_w: Mutex::new(HashMap::new()),
            battery_health_pct: Mutex::new(HashMap::new()),
            fan_rpm: Mutex::new(HashMap::new()),
            fan_pwm_pct: Mutex::new(HashMap::new()),
            fan_temperature_c: Mutex::new(HashMap::new()),
        }
    }

    fn transact<R>(
        &self,
        domain: SystemHistoryDomain,
        stamp: CorrelatedTelemetryStamp,
        state: SystemObservationState,
        write: impl FnOnce(Option<u64>) -> R,
    ) -> Result<R, CorrelatedIngestionError> {
        let _commit = lock_unpoisoned(&self.system_commit_gates[domain.index()]);
        let measured_at_ms = state.observed_at_ms();
        if measured_at_ms.is_some_and(|measured| stamp.completed_at_ms() < measured) {
            return Err(CorrelatedIngestionError::CompletionPrecedesMeasurement {
                domain,
                measured_at_ms: measured_at_ms.unwrap_or_default(),
                completed_at_ms: stamp.completed_at_ms(),
            });
        }
        let last_revisions = self
            .last_revisions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let slot = last_revisions[domain.index()];
        if slot.is_some_and(|last| stamp.revision() <= last) {
            return Err(CorrelatedIngestionError::NonIncreasingRevision {
                domain,
                last_revision: slot.unwrap_or_default(),
                rejected_revision: stamp.revision(),
            });
        }
        drop(last_revisions);
        let output = write(measured_at_ms);
        self.last_revisions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())[domain.index()] =
            Some(stamp.revision());
        self.receipts[domain.index()].push_in_transaction(CorrelatedDomainReceipt { stamp, state });
        // Release publishes every ring/map/sink mutation above as one committed
        // generation to cache readers that load the public revision with Acquire.
        self.revision.fetch_add(1, Ordering::Release);
        Ok(output)
    }

    fn transact_dynamic<R>(
        &self,
        domain: DynamicHistoryDomain,
        stamp: CorrelatedTelemetryStamp,
        measured_at_ms: Option<u64>,
        write: impl FnOnce() -> R,
    ) -> Result<R, DynamicIngestionError> {
        let _commit = lock_unpoisoned(&self.dynamic_commit_gates[domain.index()]);
        if measured_at_ms.is_some_and(|measured| stamp.completed_at_ms() < measured) {
            return Err(DynamicIngestionError::CompletionPrecedesMeasurement {
                domain,
                measured_at_ms: measured_at_ms.unwrap_or_default(),
                completed_at_ms: stamp.completed_at_ms(),
            });
        }
        let last_revisions = self
            .dynamic_last_revisions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let slot = last_revisions[domain.index()];
        if slot.is_some_and(|last| stamp.revision() <= last) {
            return Err(DynamicIngestionError::NonIncreasingRevision {
                domain,
                last_revision: slot.unwrap_or_default(),
                rejected_revision: stamp.revision(),
            });
        }
        drop(last_revisions);
        let output = write();
        self.dynamic_last_revisions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())[domain.index()] =
            Some(stamp.revision());
        self.revision.fetch_add(1, Ordering::Release);
        Ok(output)
    }

    fn system_commit_gate(&self, domain: SystemHistoryDomain) -> Arc<Mutex<()>> {
        self.system_commit_gates[domain.index()].clone()
    }

    fn dynamic_commit_gate(&self, domain: DynamicHistoryDomain) -> Arc<Mutex<()>> {
        self.dynamic_commit_gates[domain.index()].clone()
    }
}

fn lock_unpoisoned(mutex: &Mutex<()>) -> MutexGuard<'_, ()> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Read side of independently scheduled system telemetry histories.
#[derive(Clone)]
pub struct CorrelatedSystemTelemetryHistory {
    inner: Arc<CorrelatedSystemTelemetryHistoryInner>,
}

impl CorrelatedSystemTelemetryHistory {
    pub(crate) fn shared(capacity: usize) -> (Self, CorrelatedSystemTelemetryIngestor) {
        let inner = Arc::new(CorrelatedSystemTelemetryHistoryInner::new(capacity));
        (
            Self {
                inner: inner.clone(),
            },
            CorrelatedSystemTelemetryIngestor {
                inner,
                record_sink: None,
            },
        )
    }

    #[must_use]
    pub fn receipts(&self, domain: SystemHistoryDomain) -> Vec<CorrelatedDomainReceipt> {
        self.inner.receipts[domain.index()].samples()
    }

    /// Monotonic generation of accepted writes across every fixed and dynamic
    /// lane. Rejected duplicate/out-of-order events never advance it.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.inner.revision.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn uptime_secs(&self) -> CorrelatedMetricHistory<u64> {
        self.inner.uptime_secs.clone()
    }

    #[must_use]
    pub fn process_count(&self) -> CorrelatedMetricHistory<u64> {
        self.inner.process_count.clone()
    }

    #[must_use]
    pub fn thread_count(&self) -> CorrelatedMetricHistory<u64> {
        self.inner.thread_count.clone()
    }

    #[must_use]
    pub fn cpu_usage(&self) -> CorrelatedMetricHistory<f32> {
        self.inner.cpu_usage.clone()
    }

    #[must_use]
    pub fn cpu_core_usage(&self) -> Vec<CorrelatedMetricHistory<f32>> {
        let _commit =
            lock_unpoisoned(&self.inner.system_commit_gates[SystemHistoryDomain::Cpu.index()]);
        self.inner
            .cpu_cores
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Per-core temperature histories (°C), oldest..newest, gap-aware.
    #[must_use]
    pub fn cpu_core_temperature(&self) -> Vec<CorrelatedMetricHistory<f32>> {
        let _commit =
            lock_unpoisoned(&self.inner.system_commit_gates[SystemHistoryDomain::Cpu.index()]);
        self.inner
            .cpu_core_temperatures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Per-core frequency histories (MHz), oldest..newest, gap-aware.
    #[must_use]
    pub fn cpu_core_frequency_mhz(&self) -> Vec<CorrelatedMetricHistory<u64>> {
        let _commit =
            lock_unpoisoned(&self.inner.system_commit_gates[SystemHistoryDomain::Cpu.index()]);
        self.inner
            .cpu_core_frequencies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    #[must_use]
    pub fn cpu_temperature(&self) -> CorrelatedMetricHistory<f32> {
        self.inner.cpu_temperature.clone()
    }

    #[must_use]
    pub fn cpu_frequency_mhz(&self) -> CorrelatedMetricHistory<u64> {
        self.inner.cpu_frequency_mhz.clone()
    }

    #[must_use]
    pub fn cpu_power_w(&self) -> CorrelatedMetricHistory<f32> {
        self.inner.cpu_power_w.clone()
    }

    #[must_use]
    pub fn memory_usage(&self) -> CorrelatedMetricHistory<f32> {
        self.inner.memory_usage.clone()
    }

    #[must_use]
    pub fn swap_usage(&self) -> CorrelatedMetricHistory<f32> {
        self.inner.swap_usage.clone()
    }

    #[must_use]
    pub fn storage_activity(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<f32>> {
        device_history(
            &self.inner.storage_activity,
            device_id,
            &self.inner.system_commit_gates[SystemHistoryDomain::Storage.index()],
        )
    }

    /// Per-device read+write throughput in bytes/sec.
    #[must_use]
    pub fn storage_rate(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<u64>> {
        device_history(
            &self.inner.storage_rate,
            device_id,
            &self.inner.system_commit_gates[SystemHistoryDomain::Storage.index()],
        )
    }

    /// Per-device read-direction throughput in bytes/sec. Split-direction
    /// companion of [`Self::storage_rate`]: the same accepted events, lifecycle
    /// gates, and generation resets, with each direction keeping its OWN
    /// availability (a missing measurement is a gap, never zero). Deliberately
    /// not mirrored to the persistence sink — the persisted vocabulary carries
    /// the summed series, and split-direction curves are a live-graph
    /// projection (the same precedent as the GPU engine ring).
    #[must_use]
    pub fn storage_read_rate(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<u64>> {
        device_history(
            &self.inner.storage_read_rate,
            device_id,
            &self.inner.system_commit_gates[SystemHistoryDomain::Storage.index()],
        )
    }

    /// Per-device write-direction throughput in bytes/sec; see
    /// [`Self::storage_read_rate`].
    #[must_use]
    pub fn storage_write_rate(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<u64>> {
        device_history(
            &self.inner.storage_write_rate,
            device_id,
            &self.inner.system_commit_gates[SystemHistoryDomain::Storage.index()],
        )
    }

    /// Per-device SMART temperature in °C.
    #[must_use]
    pub fn storage_temperature_c(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<f32>> {
        device_history(
            &self.inner.storage_temperature_c,
            device_id,
            &self.inner.system_commit_gates[SystemHistoryDomain::Storage.index()],
        )
    }

    #[must_use]
    pub fn storage_rate_total(&self) -> CorrelatedMetricHistory<u64> {
        self.inner.storage_rate_total.clone()
    }

    #[must_use]
    pub fn network_rate(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<u64>> {
        device_history(
            &self.inner.network_rate,
            device_id,
            &self.inner.system_commit_gates[SystemHistoryDomain::Network.index()],
        )
    }

    /// Per-device receive-direction throughput in bytes/sec.
    /// Split-direction companion of [`Self::network_rate`]: the same accepted
    /// events, lifecycle gates, and generation resets, with each direction
    /// keeping its OWN availability (a missing measurement is a gap, never
    /// zero). Deliberately not mirrored to the persistence sink — the persisted
    /// vocabulary carries the summed `network-rate-bps` series, and
    /// split-direction curves are a live-graph projection (the same precedent
    /// as the GPU engine ring).
    #[must_use]
    pub fn network_rx_rate(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<u64>> {
        device_history(
            &self.inner.network_rx_rate,
            device_id,
            &self.inner.system_commit_gates[SystemHistoryDomain::Network.index()],
        )
    }

    /// Per-device transmit-direction throughput in bytes/sec; see
    /// [`Self::network_rx_rate`].
    #[must_use]
    pub fn network_tx_rate(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<u64>> {
        device_history(
            &self.inner.network_tx_rate,
            device_id,
            &self.inner.system_commit_gates[SystemHistoryDomain::Network.index()],
        )
    }

    #[must_use]
    pub fn network_rate_total(&self) -> CorrelatedMetricHistory<u64> {
        self.inner.network_rate_total.clone()
    }

    #[must_use]
    pub fn gpu_metrics(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<GpuMetricPoint>> {
        device_history(
            &self.inner.gpu_metrics,
            device_id,
            &self.inner.system_commit_gates[SystemHistoryDomain::Gpu.index()],
        )
    }

    #[must_use]
    pub fn gpu_engine_metrics(
        &self,
        device_id: &DeviceId,
    ) -> Option<DeviceMetricHistory<GpuEngineMetricPoint>> {
        device_history(
            &self.inner.gpu_engine_metrics,
            device_id,
            &self.inner.system_commit_gates[SystemHistoryDomain::Gpu.index()],
        )
    }

    #[must_use]
    pub fn gpu_usage(&self, device_id: &DeviceId) -> Option<DeviceMetricHistory<f32>> {
        device_history(
            &self.inner.gpu_usage,
            device_id,
            &self.inner.system_commit_gates[SystemHistoryDomain::Gpu.index()],
        )
    }

    #[must_use]
    pub fn gpu_usage_mean(&self) -> CorrelatedMetricHistory<f32> {
        self.inner.gpu_usage_mean.clone()
    }

    #[must_use]
    pub fn dynamic_history(&self) -> DynamicTelemetryHistory {
        DynamicTelemetryHistory::new(self.inner.clone())
    }
}

fn device_history<T: Clone>(
    histories: &Mutex<HashMap<DeviceId, DeviceMetricHistory<T>>>,
    device_id: &DeviceId,
    commit_gate: &Mutex<()>,
) -> Option<DeviceMetricHistory<T>> {
    let _commit = lock_unpoisoned(commit_gate);
    histories
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(device_id)
        .cloned()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorrelatedIngestionError {
    CompletionPrecedesMeasurement {
        domain: SystemHistoryDomain,
        measured_at_ms: u64,
        completed_at_ms: u64,
    },
    NonIncreasingRevision {
        domain: SystemHistoryDomain,
        last_revision: u64,
        rejected_revision: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicIngestionError {
    CompletionPrecedesMeasurement {
        domain: DynamicHistoryDomain,
        measured_at_ms: u64,
        completed_at_ms: u64,
    },
    NonIncreasingRevision {
        domain: DynamicHistoryDomain,
        last_revision: u64,
        rejected_revision: u64,
    },
}

/// Observable resource-bound result of one accepted dynamic-device snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicIngestionReport {
    /// Distinct current identities whose history could not be admitted because
    /// the retained identity ceiling was already occupied.
    pub rejected_identity_capacity: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CorrelatedIngestionReport {
    pub rejected_device_values: usize,
    pub reset_device_histories: usize,
    pub pruned_device_histories: usize,
}

#[cfg(test)]
#[path = "../tests/headless/system_history.rs"]
mod tests;
