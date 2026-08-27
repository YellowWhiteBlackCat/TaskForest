//! The `CorrelatedSystemTelemetryIngestor` write capability.
//!
//! Fans accepted host, CPU, memory, storage, network, and GPU observations
//! into the shared bounded history; intentionally distinct from the read store.
//! When a persistence sink is attached (roadmap #4, R1), every sample the
//! rings accept is mirrored to it through the same fan-out — so the on-disk
//! history, the ring contents, and the lifecycle decisions can never drift
//! apart, and a rejected observation never reaches disk.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use taskmanager_core::{
    CpuMetrics, CpuTelemetryObservation, DeviceId, FailureKind, GpuEngineMetricPoint,
    GpuTelemetryObservation, HistoricalSample, HistoryMetric, HistoryRecordSink, HistorySeriesKey,
    HostRuntimeObservation, MAX_TRACKED_LOGICAL_CPUS, MemoryMetrics, MemoryTelemetryObservation,
    SystemObservationState,
};

use super::device::{
    DeviceMeasurementFreshness, DeviceMetricIngest, DeviceMetricInput, PersistFanout,
    PersistedScalars, ingest_device_metrics,
};
use super::gpu::GpuMetricPoint;
use super::{
    CorrelatedIngestionError, CorrelatedIngestionReport, CorrelatedMetricHistory,
    CorrelatedSystemTelemetryHistoryInner, CorrelatedTelemetryStamp, DeviceMetricHistory,
    SystemHistoryDomain,
};

mod dynamic;
mod storage_network;

/// Write capability for application-correlated system telemetry only.
///
/// This type is intentionally separate from the read history. Platform
/// providers must publish observations through application ports and must
/// never receive this capability.
#[derive(Clone)]
pub struct CorrelatedSystemTelemetryIngestor {
    pub(super) inner: Arc<CorrelatedSystemTelemetryHistoryInner>,
    /// Optional persistence mirror (roadmap #4). `None` until the composition
    /// edge attaches a store — history persistence is strictly opt-in.
    pub(super) record_sink: Option<Arc<dyn HistoryRecordSink>>,
}

impl std::fmt::Debug for CorrelatedSystemTelemetryIngestor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CorrelatedSystemTelemetryIngestor")
            .field("persistence_attached", &self.record_sink.is_some())
            .finish_non_exhaustive()
    }
}

impl CorrelatedSystemTelemetryIngestor {
    /// Attach the persistence write port (the roadmap #4 store seam). From
    /// this point every sample the correlated rings accept — values and
    /// explicit gaps alike — is mirrored to the sink in acceptance order.
    /// Store isolation is preserved: only this ingestor may feed the sink,
    /// and the read store never learns it exists.
    ///
    /// The callback runs inside the affected telemetry domain's commit gate so
    /// ring, receipt, and persisted order stay identical. A custom sink must
    /// therefore not synchronously read that same domain through a history
    /// handle. The production persistent store only appends to its own bounded
    /// pending state and satisfies this non-reentrant contract.
    #[must_use]
    pub fn with_record_sink(mut self, sink: Arc<dyn HistoryRecordSink>) -> Self {
        self.record_sink = Some(sink);
        self
    }

    /// Remove the optional persistence mirror while retaining the same live
    /// graph store and its accepted-sample history.
    #[must_use]
    pub fn without_record_sink(mut self) -> Self {
        self.record_sink = None;
        self
    }

    /// Mirror one host-wide (or per-core) sample into the persistence sink.
    /// A `None` value emits an explicit gap, matching the ring exactly.
    fn emit_sample(
        &self,
        key: HistorySeriesKey,
        stamp: CorrelatedTelemetryStamp,
        measured_at_ms: Option<u64>,
        value: Option<f64>,
    ) {
        let Some(sink) = self.record_sink.as_deref() else {
            return;
        };
        sink.record_sample(
            key,
            HistoricalSample {
                revision: stamp.revision(),
                completed_at_ms: stamp.completed_at_ms(),
                measured_at_ms,
                value,
            },
        );
    }

    fn emit_system(
        &self,
        metric: HistoryMetric,
        stamp: CorrelatedTelemetryStamp,
        measured_at_ms: Option<u64>,
        value: Option<f64>,
    ) {
        self.emit_sample(
            HistorySeriesKey::system(metric),
            stamp,
            measured_at_ms,
            value,
        );
    }

    /// The per-core fan-out is the only core-scoped series family, so the
    /// metric is fixed here; the core index is the series scope.
    fn emit_core_sample(
        &self,
        core_index: u16,
        stamp: CorrelatedTelemetryStamp,
        measured_at_ms: Option<u64>,
        value: Option<f64>,
    ) {
        self.emit_sample(
            HistorySeriesKey::for_core(HistoryMetric::CpuCoreUsagePct, core_index),
            stamp,
            measured_at_ms,
            value,
        );
    }

    /// `u64` counts (uptime/processes/threads, frequencies, rates) persist as
    /// `f64`: every such magnitude in this vocabulary sits far below 2^53, so
    /// the widening keeps the value exact.
    fn scalar_u64(value: Option<u64>) -> Option<f64> {
        value.map(|value| value as f64)
    }

    /// Record an accepted terminal failure that produced no domain observation.
    ///
    /// Application submission/provider failures map into the shared
    /// [`FailureKind`] vocabulary before entering this adapter. Every existing
    /// metric series for the failed domain advances with an explicit gap.
    pub fn ingest_correlated_unavailable(
        &self,
        stamp: CorrelatedTelemetryStamp,
        domain: SystemHistoryDomain,
        failure: FailureKind,
    ) -> Result<CorrelatedIngestionReport, CorrelatedIngestionError> {
        let state = SystemObservationState::Unavailable { failure };
        self.inner.transact(domain, stamp, state, |_| {
            match domain {
                SystemHistoryDomain::Host => {
                    self.inner.uptime_secs.push(stamp, None, None);
                    self.inner.process_count.push(stamp, None, None);
                    self.inner.thread_count.push(stamp, None, None);
                    self.emit_system(HistoryMetric::UptimeSecs, stamp, None, None::<f64>);
                    self.emit_system(HistoryMetric::ProcessCount, stamp, None, None::<f64>);
                    self.emit_system(HistoryMetric::ThreadCount, stamp, None, None::<f64>);
                }
                SystemHistoryDomain::Cpu => {
                    self.inner.cpu_usage.push(stamp, None, None);
                    self.inner.cpu_temperature.push(stamp, None, None);
                    self.inner.cpu_frequency_mhz.push(stamp, None, None);
                    self.inner.cpu_power_w.push(stamp, None, None);
                    self.emit_system(HistoryMetric::CpuUsagePct, stamp, None, None::<f64>);
                    self.emit_system(HistoryMetric::CpuTemperatureC, stamp, None, None::<f64>);
                    self.emit_system(HistoryMetric::CpuFrequencyMhz, stamp, None, None::<f64>);
                    self.emit_system(HistoryMetric::CpuPowerW, stamp, None, None::<f64>);
                    self.ingest_cpu_cores(stamp, None, None);
                }
                SystemHistoryDomain::Memory => {
                    self.inner.memory_usage.push(stamp, None, None);
                    self.inner.swap_usage.push(stamp, None, None);
                    self.emit_system(HistoryMetric::MemoryUsedPct, stamp, None, None::<f64>);
                    self.emit_system(HistoryMetric::SwapUsedPct, stamp, None, None::<f64>);
                }
                SystemHistoryDomain::Storage => {
                    push_device_gaps(
                        &self.inner.storage_activity,
                        stamp,
                        &PersistFanout::maybe(
                            self.record_sink.as_deref(),
                            STORAGE_ACTIVITY_PERSISTED,
                        ),
                    );
                    push_device_gaps(&self.inner.storage_rate, stamp, &PersistFanout::disabled());
                    push_device_gaps(
                        &self.inner.storage_read_rate,
                        stamp,
                        &PersistFanout::disabled(),
                    );
                    push_device_gaps(
                        &self.inner.storage_write_rate,
                        stamp,
                        &PersistFanout::disabled(),
                    );
                    push_device_gaps(
                        &self.inner.storage_temperature_c,
                        stamp,
                        &PersistFanout::disabled(),
                    );
                    self.inner.storage_rate_total.push(stamp, None, None);
                }
                SystemHistoryDomain::Network => {
                    push_device_gaps(
                        &self.inner.network_rate,
                        stamp,
                        &PersistFanout::maybe(self.record_sink.as_deref(), NETWORK_RATE_PERSISTED),
                    );
                    push_device_gaps(
                        &self.inner.network_rx_rate,
                        stamp,
                        &PersistFanout::disabled(),
                    );
                    push_device_gaps(
                        &self.inner.network_tx_rate,
                        stamp,
                        &PersistFanout::disabled(),
                    );
                    self.inner.network_rate_total.push(stamp, None, None);
                }
                SystemHistoryDomain::Gpu => {
                    push_device_gaps(
                        &self.inner.gpu_metrics,
                        stamp,
                        &PersistFanout::maybe(self.record_sink.as_deref(), GPU_POINT_PERSISTED),
                    );
                    push_device_gaps(
                        &self.inner.gpu_usage,
                        stamp,
                        &PersistFanout::maybe(self.record_sink.as_deref(), GPU_USAGE_PERSISTED),
                    );
                    // The per-engine ring (the on-demand PMU lane) is intentionally
                    // not persisted: replay curves consume the scalar series.
                    push_device_gaps(
                        &self.inner.gpu_engine_metrics,
                        stamp,
                        &PersistFanout::disabled(),
                    );
                    self.inner.gpu_usage_mean.push(stamp, None, None);
                }
            }
            CorrelatedIngestionReport::default()
        })
    }

    pub fn ingest_correlated_host(
        &self,
        stamp: CorrelatedTelemetryStamp,
        observation: &HostRuntimeObservation,
    ) -> Result<CorrelatedIngestionReport, CorrelatedIngestionError> {
        self.inner.transact(
            SystemHistoryDomain::Host,
            stamp,
            observation.state(),
            |measured_at_ms| {
                let value = observation.current_value();
                let uptime = value.and_then(|facts| facts.uptime_secs.current_value().copied());
                let process_count =
                    value.and_then(|facts| facts.processes.current_value().copied());
                let thread_count = value.and_then(|facts| facts.threads.current_value().copied());
                self.inner.uptime_secs.push(stamp, measured_at_ms, uptime);
                self.inner
                    .process_count
                    .push(stamp, measured_at_ms, process_count);
                self.inner
                    .thread_count
                    .push(stamp, measured_at_ms, thread_count);
                self.emit_system(
                    HistoryMetric::UptimeSecs,
                    stamp,
                    measured_at_ms,
                    Self::scalar_u64(uptime),
                );
                self.emit_system(
                    HistoryMetric::ProcessCount,
                    stamp,
                    measured_at_ms,
                    Self::scalar_u64(process_count),
                );
                self.emit_system(
                    HistoryMetric::ThreadCount,
                    stamp,
                    measured_at_ms,
                    Self::scalar_u64(thread_count),
                );
                CorrelatedIngestionReport::default()
            },
        )
    }

    pub fn ingest_correlated_cpu(
        &self,
        stamp: CorrelatedTelemetryStamp,
        observation: &CpuTelemetryObservation,
    ) -> Result<CorrelatedIngestionReport, CorrelatedIngestionError> {
        self.inner.transact(
            SystemHistoryDomain::Cpu,
            stamp,
            observation.state(),
            |measured_at_ms| {
                let value = observation.current_value();
                let usage = value.and_then(|cpu| cpu.current_global_usage_pct().and_then(finite));
                let temperature =
                    value.and_then(|cpu| cpu.current_temperature_c().and_then(finite));
                let frequency = value.and_then(CpuMetrics::current_frequency_mhz);
                let power = value.and_then(|cpu| cpu.current_power_w().and_then(finite));
                self.inner.cpu_usage.push(stamp, measured_at_ms, usage);
                self.inner
                    .cpu_temperature
                    .push(stamp, measured_at_ms, temperature);
                self.inner
                    .cpu_frequency_mhz
                    .push(stamp, measured_at_ms, frequency);
                self.inner.cpu_power_w.push(stamp, measured_at_ms, power);
                self.emit_system(
                    HistoryMetric::CpuUsagePct,
                    stamp,
                    measured_at_ms,
                    usage.map(f64::from),
                );
                self.emit_system(
                    HistoryMetric::CpuTemperatureC,
                    stamp,
                    measured_at_ms,
                    temperature.map(f64::from),
                );
                self.emit_system(
                    HistoryMetric::CpuFrequencyMhz,
                    stamp,
                    measured_at_ms,
                    Self::scalar_u64(frequency),
                );
                self.emit_system(
                    HistoryMetric::CpuPowerW,
                    stamp,
                    measured_at_ms,
                    power.map(f64::from),
                );
                self.ingest_cpu_cores(stamp, measured_at_ms, value);
                CorrelatedIngestionReport::default()
            },
        )
    }

    fn ingest_cpu_cores(
        &self,
        stamp: CorrelatedTelemetryStamp,
        measured_at_ms: Option<u64>,
        cpu: Option<&CpuMetrics>,
    ) {
        let incoming_len = cpu
            .map_or(0, CpuMetrics::current_core_usage_len)
            .min(MAX_TRACKED_LOGICAL_CPUS);
        // All three per-core families share the logical-core index space and
        // the same accepted event; a provider that cannot resolve a core's
        // temperature/frequency appends an explicit gap for that core, never
        // a fabricated value. The per-core temperature/frequency rings are
        // intentionally not mirrored to the persistence sink yet (same
        // precedent as the GPU engine ring): the sink vocabulary has no
        // per-core series for them today.
        let mut usage_rings = self
            .inner
            .cpu_cores
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut temperature_rings = self
            .inner
            .cpu_core_temperatures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut frequency_rings = self
            .inner
            .cpu_core_frequencies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while usage_rings.len() < incoming_len {
            usage_rings.push(CorrelatedMetricHistory::new(
                self.inner.capacity,
                self.inner.system_commit_gate(SystemHistoryDomain::Cpu),
            ));
        }
        while temperature_rings.len() < incoming_len {
            temperature_rings.push(CorrelatedMetricHistory::new(
                self.inner.capacity,
                self.inner.system_commit_gate(SystemHistoryDomain::Cpu),
            ));
        }
        while frequency_rings.len() < incoming_len {
            frequency_rings.push(CorrelatedMetricHistory::new(
                self.inner.capacity,
                self.inner.system_commit_gate(SystemHistoryDomain::Cpu),
            ));
        }
        for (index, history) in usage_rings.iter().enumerate() {
            let value = cpu
                .and_then(|metrics| metrics.current_core_usage_pct(index))
                .and_then(finite);
            history.push(stamp, measured_at_ms, value);
            // A ring index beyond `u16` cannot be expressed as a series scope;
            // the ring keeps it, persistence honestly skips it.
            if let Ok(core_index) = u16::try_from(index) {
                self.emit_core_sample(core_index, stamp, measured_at_ms, value.map(f64::from));
            }
        }
        for (index, history) in temperature_rings.iter_mut().enumerate() {
            let value = cpu
                .and_then(|metrics| metrics.current_core_temperature_c(index))
                .and_then(finite);
            history.push(stamp, measured_at_ms, value);
        }
        for (index, history) in frequency_rings.iter_mut().enumerate() {
            let value = cpu.and_then(|metrics| metrics.current_core_frequency_mhz(index));
            history.push(stamp, measured_at_ms, value);
        }
    }

    pub fn ingest_correlated_memory(
        &self,
        stamp: CorrelatedTelemetryStamp,
        observation: &MemoryTelemetryObservation,
    ) -> Result<CorrelatedIngestionReport, CorrelatedIngestionError> {
        self.inner.transact(
            SystemHistoryDomain::Memory,
            stamp,
            observation.state(),
            |measured_at_ms| {
                let value = observation.current_value();
                let memory = value.and_then(MemoryMetrics::used_percentage_observed);
                let swap = value.and_then(MemoryMetrics::swap_percentage_observed);
                self.inner.memory_usage.push(stamp, measured_at_ms, memory);
                self.inner.swap_usage.push(stamp, measured_at_ms, swap);
                self.emit_system(
                    HistoryMetric::MemoryUsedPct,
                    stamp,
                    measured_at_ms,
                    memory.map(f64::from),
                );
                self.emit_system(
                    HistoryMetric::SwapUsedPct,
                    stamp,
                    measured_at_ms,
                    swap.map(f64::from),
                );
                CorrelatedIngestionReport::default()
            },
        )
    }

    pub fn ingest_correlated_gpu(
        &self,
        stamp: CorrelatedTelemetryStamp,
        observation: &GpuTelemetryObservation,
    ) -> Result<CorrelatedIngestionReport, CorrelatedIngestionError> {
        self.inner.transact(
            SystemHistoryDomain::Gpu,
            stamp,
            observation.state(),
            |measured_at_ms| {
                let point_inputs = observation.current_value().map(|metrics| {
                    metrics
                        .iter()
                        .map(|gpu| DeviceMetricInput {
                            device_id: DeviceId::new(gpu.device_id.clone()),
                            generation: gpu.device_generation.get(),
                            value: Some(GpuMetricPoint::from_metrics(gpu)),
                            measured_at_ms,
                            freshness: DeviceMeasurementFreshness::DomainTick,
                        })
                        .collect()
                });
                let usage_inputs = observation.current_value().map(|metrics| {
                    metrics
                        .iter()
                        .map(|gpu| DeviceMetricInput {
                            device_id: DeviceId::new(gpu.device_id.clone()),
                            generation: gpu.device_generation.get(),
                            value: gpu.current_utilization_pct().and_then(finite),
                            measured_at_ms,
                            freshness: DeviceMeasurementFreshness::DomainTick,
                        })
                        .collect()
                });
                let engine_inputs = observation.current_value().map(|metrics| {
                    metrics
                        .iter()
                        .map(|gpu| DeviceMetricInput {
                            device_id: DeviceId::new(gpu.device_id.clone()),
                            generation: gpu.device_generation.get(),
                            value: GpuEngineMetricPoint::from_metrics(gpu),
                            measured_at_ms,
                            freshness: DeviceMeasurementFreshness::DomainTick,
                        })
                        .collect()
                });
                let usage_mean = observation.current_value().and_then(|metrics| {
                    let values = metrics
                        .iter()
                        .filter_map(|gpu| gpu.current_utilization_pct().and_then(finite))
                        .collect::<Vec<_>>();
                    (!values.is_empty()).then(|| values.iter().sum::<f32>() / values.len() as f32)
                });
                self.inner
                    .gpu_usage_mean
                    .push(stamp, measured_at_ms, usage_mean);
                let report = ingest_device_metrics(DeviceMetricIngest {
                    histories: &self.inner.gpu_metrics,
                    capacity: self.inner.capacity,
                    commit_gate: self.inner.system_commit_gate(SystemHistoryDomain::Gpu),
                    stamp,
                    measured_at_ms,
                    state: observation.state(),
                    lifecycles: observation.device_lifecycles(),
                    inputs: point_inputs.unwrap_or_default(),
                    // The composite point ring carries power/temperature/frequency for
                    // replay; utilization is persisted by the aggregate usage ring
                    // below, so the point fan-out deliberately omits it.
                    persist: &PersistFanout::maybe(
                        self.record_sink.as_deref(),
                        GPU_POINT_PERSISTED,
                    ),
                });
                // Keep the established aggregate utilization history for the sidebar
                // and older frontend adapters. Both histories use the same accepted
                // event and lifecycle map; the typed point history is the selector's
                // source of truth.
                let _ = ingest_device_metrics(DeviceMetricIngest {
                    histories: &self.inner.gpu_usage,
                    capacity: self.inner.capacity,
                    commit_gate: self.inner.system_commit_gate(SystemHistoryDomain::Gpu),
                    stamp,
                    measured_at_ms,
                    state: observation.state(),
                    lifecycles: observation.device_lifecycles(),
                    inputs: usage_inputs.unwrap_or_default(),
                    persist: &PersistFanout::maybe(
                        self.record_sink.as_deref(),
                        GPU_USAGE_PERSISTED,
                    ),
                });
                let _ = ingest_device_metrics(DeviceMetricIngest {
                    histories: &self.inner.gpu_engine_metrics,
                    capacity: self.inner.capacity,
                    commit_gate: self.inner.system_commit_gate(SystemHistoryDomain::Gpu),
                    stamp,
                    measured_at_ms,
                    state: observation.state(),
                    lifecycles: observation.device_lifecycles(),
                    inputs: engine_inputs.unwrap_or_default(),
                    persist: &PersistFanout::disabled(),
                });
                report
            },
        )
    }
}

fn f32_scalar(value: &f32) -> Option<f64> {
    Some(f64::from(*value))
}

fn u64_scalar(value: &u64) -> Option<f64> {
    Some(*value as f64)
}

/// Persisted projection of the composite GPU point ring: only the fields no
/// other ring carries. Utilization is persisted by the aggregate usage ring.
fn gpu_point_power_w(point: &GpuMetricPoint) -> Option<f64> {
    point.power_w.map(f64::from)
}

fn gpu_point_temperature_c(point: &GpuMetricPoint) -> Option<f64> {
    point.temperature_c.map(f64::from)
}

fn gpu_point_frequency_mhz(point: &GpuMetricPoint) -> Option<f64> {
    point.frequency_mhz.map(|frequency| frequency as f64)
}

const STORAGE_ACTIVITY_PERSISTED: PersistedScalars<f32> =
    &[(HistoryMetric::StorageActivityPct, f32_scalar)];
const NETWORK_RATE_PERSISTED: PersistedScalars<u64> =
    &[(HistoryMetric::NetworkRateBps, u64_scalar)];
const GPU_USAGE_PERSISTED: PersistedScalars<f32> = &[(HistoryMetric::GpuUsagePct, f32_scalar)];
const GPU_POINT_PERSISTED: PersistedScalars<GpuMetricPoint> = &[
    (HistoryMetric::GpuPowerW, gpu_point_power_w),
    (HistoryMetric::GpuTemperatureC, gpu_point_temperature_c),
    (HistoryMetric::GpuFrequencyMhz, gpu_point_frequency_mhz),
];

fn push_device_gaps<T>(
    histories: &Mutex<HashMap<DeviceId, DeviceMetricHistory<T>>>,
    stamp: CorrelatedTelemetryStamp,
    persist: &PersistFanout<'_, T>,
) {
    let histories = histories
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (device_id, history) in histories.iter() {
        history.metric.push(stamp, None, None);
        persist.emit(device_id.as_str(), stamp, None, None);
    }
}

pub(super) fn finite(value: f32) -> Option<f32> {
    value.is_finite().then_some(value)
}
