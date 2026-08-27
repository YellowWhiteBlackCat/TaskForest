//! Correlated storage and network device-domain ingestion.

use taskmanager_core::{
    DeviceId, DeviceStatus, DiskMetrics, NetworkTelemetryObservation, SmartAvailability,
    StorageTelemetryObservation,
};

use super::super::device::{
    DeviceMeasurementFreshness, DeviceMetricIngest, DeviceMetricInput, PersistFanout,
    ingest_device_metrics,
};
use super::super::{
    CorrelatedIngestionError, CorrelatedIngestionReport, CorrelatedTelemetryStamp,
    SystemHistoryDomain, SystemObservationState,
};
use super::{
    CorrelatedSystemTelemetryIngestor, NETWORK_RATE_PERSISTED, STORAGE_ACTIVITY_PERSISTED, finite,
};

impl CorrelatedSystemTelemetryIngestor {
    pub fn ingest_correlated_storage(
        &self,
        stamp: CorrelatedTelemetryStamp,
        observation: &StorageTelemetryObservation,
    ) -> Result<CorrelatedIngestionReport, CorrelatedIngestionError> {
        self.inner.transact(
            SystemHistoryDomain::Storage,
            stamp,
            observation.state(),
            |measured_at_ms| {
                let activity_inputs = observation.current_value().map(|metrics| {
                    metrics
                        .iter()
                        .map(|disk| DeviceMetricInput {
                            device_id: DeviceId::new(disk.device_id.clone()),
                            generation: disk.device_generation.get(),
                            value: disk.current_active_time_pct().and_then(finite),
                            measured_at_ms,
                            freshness: DeviceMeasurementFreshness::DomainTick,
                        })
                        .collect()
                });
                let rate_inputs = observation.current_value().map(|metrics| {
                    metrics
                        .iter()
                        .map(|disk| DeviceMetricInput {
                            device_id: DeviceId::new(disk.device_id.clone()),
                            generation: disk.device_generation.get(),
                            value: disk.current_read_bytes_per_sec().and_then(|read| {
                                disk.current_write_bytes_per_sec()
                                    .and_then(|write| read.checked_add(write))
                            }),
                            measured_at_ms,
                            freshness: DeviceMeasurementFreshness::DomainTick,
                        })
                        .collect()
                });
                // Split-direction companions of the summed rate lane, fed from
                // the SAME accepted observation fields. Each direction keeps
                // its own availability: a disk reporting only read traffic
                // appends a measured read sample and a write gap, never a
                // fabricated zero. Deliberately not persisted — the sink
                // vocabulary carries the summed series (the GPU engine ring
                // precedent for live-graph-only lanes).
                let read_inputs = observation.current_value().map(|metrics| {
                    metrics
                        .iter()
                        .map(|disk| DeviceMetricInput {
                            device_id: DeviceId::new(disk.device_id.clone()),
                            generation: disk.device_generation.get(),
                            value: disk.current_read_bytes_per_sec(),
                            measured_at_ms,
                            freshness: DeviceMeasurementFreshness::DomainTick,
                        })
                        .collect()
                });
                let write_inputs = observation.current_value().map(|metrics| {
                    metrics
                        .iter()
                        .map(|disk| DeviceMetricInput {
                            device_id: DeviceId::new(disk.device_id.clone()),
                            generation: disk.device_generation.get(),
                            value: disk.current_write_bytes_per_sec(),
                            measured_at_ms,
                            freshness: DeviceMeasurementFreshness::DomainTick,
                        })
                        .collect()
                });
                let temperature_inputs = observation.current_value().map(|metrics| {
                    metrics
                        .iter()
                        .map(|disk| {
                            let (smart_measured_at_ms, value) =
                                fresh_smart_temperature(disk, stamp.completed_at_ms());
                            DeviceMetricInput {
                                device_id: DeviceId::new(disk.device_id.clone()),
                                generation: disk.device_generation.get(),
                                value,
                                measured_at_ms: smart_measured_at_ms,
                                freshness: DeviceMeasurementFreshness::DistinctTimestamp,
                            }
                        })
                        .collect()
                });
                // The activity lane is per-device only: the store observes
                // per-disk facts and fabricates no host aggregate for them,
                // so there is deliberately no host mean computation here.
                let rate_total = observation.current_value().and_then(|metrics| {
                    if metrics.is_empty() {
                        return (matches!(
                            observation.state(),
                            SystemObservationState::Current { .. }
                        ) && observation.device_lifecycles().is_empty())
                        .then_some(0);
                    }
                    metrics.iter().try_fold(0_u64, |total, disk| {
                        total
                            .checked_add(
                                disk.scalar_observations()
                                    .read_bytes_per_sec
                                    .current_value()
                                    .copied()?,
                            )?
                            .checked_add(
                                disk.scalar_observations()
                                    .write_bytes_per_sec
                                    .current_value()
                                    .copied()?,
                            )
                    })
                });
                self.inner
                    .storage_rate_total
                    .push(stamp, measured_at_ms, rate_total);
                let report = ingest_device_metrics(DeviceMetricIngest {
                    histories: &self.inner.storage_activity,
                    capacity: self.inner.capacity,
                    commit_gate: self.inner.system_commit_gate(SystemHistoryDomain::Storage),
                    stamp,
                    measured_at_ms,
                    state: observation.state(),
                    lifecycles: observation.device_lifecycles(),
                    inputs: activity_inputs.unwrap_or_default(),
                    persist: &PersistFanout::maybe(
                        self.record_sink.as_deref(),
                        STORAGE_ACTIVITY_PERSISTED,
                    ),
                });
                let _ = ingest_device_metrics(DeviceMetricIngest {
                    histories: &self.inner.storage_rate,
                    capacity: self.inner.capacity,
                    commit_gate: self.inner.system_commit_gate(SystemHistoryDomain::Storage),
                    stamp,
                    measured_at_ms,
                    state: observation.state(),
                    lifecycles: observation.device_lifecycles(),
                    inputs: rate_inputs.unwrap_or_default(),
                    persist: &PersistFanout::disabled(),
                });
                let _ = ingest_device_metrics(DeviceMetricIngest {
                    histories: &self.inner.storage_read_rate,
                    capacity: self.inner.capacity,
                    commit_gate: self.inner.system_commit_gate(SystemHistoryDomain::Storage),
                    stamp,
                    measured_at_ms,
                    state: observation.state(),
                    lifecycles: observation.device_lifecycles(),
                    inputs: read_inputs.unwrap_or_default(),
                    persist: &PersistFanout::disabled(),
                });
                let _ = ingest_device_metrics(DeviceMetricIngest {
                    histories: &self.inner.storage_write_rate,
                    capacity: self.inner.capacity,
                    commit_gate: self.inner.system_commit_gate(SystemHistoryDomain::Storage),
                    stamp,
                    measured_at_ms,
                    state: observation.state(),
                    lifecycles: observation.device_lifecycles(),
                    inputs: write_inputs.unwrap_or_default(),
                    persist: &PersistFanout::disabled(),
                });
                let _ = ingest_device_metrics(DeviceMetricIngest {
                    histories: &self.inner.storage_temperature_c,
                    capacity: self.inner.capacity,
                    commit_gate: self.inner.system_commit_gate(SystemHistoryDomain::Storage),
                    stamp,
                    measured_at_ms,
                    state: observation.state(),
                    lifecycles: observation.device_lifecycles(),
                    inputs: temperature_inputs.unwrap_or_default(),
                    persist: &PersistFanout::disabled(),
                });
                report
            },
        )
    }

    pub fn ingest_correlated_network(
        &self,
        stamp: CorrelatedTelemetryStamp,
        observation: &NetworkTelemetryObservation,
    ) -> Result<CorrelatedIngestionReport, CorrelatedIngestionError> {
        self.inner.transact(
            SystemHistoryDomain::Network,
            stamp,
            observation.state(),
            |measured_at_ms| {
                let inputs = observation.current_value().map(|metrics| {
                    metrics
                        .iter()
                        .map(|network| DeviceMetricInput {
                            device_id: DeviceId::new(network.device_id.as_ref().to_owned()),
                            generation: network.device_generation.get(),
                            value: network.current_rx_bytes_per_sec().and_then(|rx| {
                                network
                                    .current_tx_bytes_per_sec()
                                    .and_then(|tx| rx.checked_add(tx))
                            }),
                            measured_at_ms,
                            freshness: DeviceMeasurementFreshness::DomainTick,
                        })
                        .collect()
                });
                // Split-direction companions of the summed rate lane, fed from
                // the SAME accepted observation fields; each direction keeps
                // its own availability (a missing counter is a gap, never
                // zero). Deliberately not persisted — the sink vocabulary
                // carries the summed `network-rate-bps` series (the GPU engine
                // ring precedent for live-graph-only lanes).
                let rx_inputs = observation.current_value().map(|metrics| {
                    metrics
                        .iter()
                        .map(|network| DeviceMetricInput {
                            device_id: DeviceId::new(network.device_id.as_ref().to_owned()),
                            generation: network.device_generation.get(),
                            value: network.current_rx_bytes_per_sec(),
                            measured_at_ms,
                            freshness: DeviceMeasurementFreshness::DomainTick,
                        })
                        .collect()
                });
                let tx_inputs = observation.current_value().map(|metrics| {
                    metrics
                        .iter()
                        .map(|network| DeviceMetricInput {
                            device_id: DeviceId::new(network.device_id.as_ref().to_owned()),
                            generation: network.device_generation.get(),
                            value: network.current_tx_bytes_per_sec(),
                            measured_at_ms,
                            freshness: DeviceMeasurementFreshness::DomainTick,
                        })
                        .collect()
                });
                let rate_total = observation.current_value().and_then(|metrics| {
                    if metrics.is_empty() {
                        return (matches!(
                            observation.state(),
                            SystemObservationState::Current { .. }
                        ) && observation.device_lifecycles().is_empty())
                        .then_some(0);
                    }
                    metrics.iter().try_fold(0_u64, |total, network| {
                        total
                            .checked_add(
                                network
                                    .scalar_observations()
                                    .rx_bytes_per_sec
                                    .current_value()
                                    .copied()?,
                            )?
                            .checked_add(
                                network
                                    .scalar_observations()
                                    .tx_bytes_per_sec
                                    .current_value()
                                    .copied()?,
                            )
                    })
                });
                self.inner
                    .network_rate_total
                    .push(stamp, measured_at_ms, rate_total);
                let _ = ingest_device_metrics(DeviceMetricIngest {
                    histories: &self.inner.network_rx_rate,
                    capacity: self.inner.capacity,
                    commit_gate: self.inner.system_commit_gate(SystemHistoryDomain::Network),
                    stamp,
                    measured_at_ms,
                    state: observation.state(),
                    lifecycles: observation.device_lifecycles(),
                    inputs: rx_inputs.unwrap_or_default(),
                    persist: &PersistFanout::disabled(),
                });
                let _ = ingest_device_metrics(DeviceMetricIngest {
                    histories: &self.inner.network_tx_rate,
                    capacity: self.inner.capacity,
                    commit_gate: self.inner.system_commit_gate(SystemHistoryDomain::Network),
                    stamp,
                    measured_at_ms,
                    state: observation.state(),
                    lifecycles: observation.device_lifecycles(),
                    inputs: tx_inputs.unwrap_or_default(),
                    persist: &PersistFanout::disabled(),
                });
                ingest_device_metrics(DeviceMetricIngest {
                    histories: &self.inner.network_rate,
                    capacity: self.inner.capacity,
                    commit_gate: self.inner.system_commit_gate(SystemHistoryDomain::Network),
                    stamp,
                    measured_at_ms,
                    state: observation.state(),
                    lifecycles: observation.device_lifecycles(),
                    inputs: inputs.unwrap_or_default(),
                    persist: &PersistFanout::maybe(
                        self.record_sink.as_deref(),
                        NETWORK_RATE_PERSISTED,
                    ),
                })
            },
        )
    }
}

fn fresh_smart_temperature(disk: &DiskMetrics, completed_at_ms: u64) -> (Option<u64>, Option<f32>) {
    let measured_at_ms = disk.smart_state.last_success_ms;
    if disk.smart_availability != SmartAvailability::Available
        || disk.smart_state.status != DeviceStatus::Healthy
        || measured_at_ms.is_none_or(|measured| measured > completed_at_ms)
    {
        return (None, None);
    }
    (measured_at_ms, disk.smart_temperature_c.and_then(finite))
}
