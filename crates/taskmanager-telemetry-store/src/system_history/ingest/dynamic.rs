//! Dynamic battery and sensor history fan-out.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use taskmanager_core::{
    DeviceId, HistoricalSample, HistoryMetric, HistoryRecordSink, HistorySeriesKey,
    PowerSupplySnapshot, SensorCenterSnapshot, SensorMagnitude, SensorQuantity,
};

use super::{CorrelatedSystemTelemetryIngestor, finite};
use crate::system_history::{
    CorrelatedTelemetryStamp, DeviceMetricHistory, DynamicHistoryDomain, DynamicIngestionError,
    DynamicIngestionReport, MAX_DYNAMIC_HISTORY_IDENTITIES,
};

impl CorrelatedSystemTelemetryIngestor {
    /// Append application-correlated battery observations to runtime-device history.
    pub fn ingest_correlated_power_supplies(
        &self,
        stamp: CorrelatedTelemetryStamp,
        snapshot: &PowerSupplySnapshot,
    ) -> Result<DynamicIngestionReport, DynamicIngestionError> {
        self.inner.transact_dynamic(
            DynamicHistoryDomain::Power,
            stamp,
            (snapshot.timestamp_ms > 0).then_some(snapshot.timestamp_ms),
            || {
                let commit_gate = self.inner.dynamic_commit_gate(DynamicHistoryDomain::Power);
                let histories = [
                    &self.inner.battery_capacity_pct,
                    &self.inner.battery_power_w,
                    &self.inner.battery_health_pct,
                ];
                if snapshot.state.status == taskmanager_core::DeviceStatus::Healthy {
                    let known = snapshot
                        .device_lifecycles
                        .keys()
                        .chain(snapshot.batteries.iter().map(|battery| &battery.id))
                        .map(|id| DeviceId::new(id.clone()))
                        .collect::<HashSet<_>>();
                    for family in histories {
                        prune_dynamic_histories(family, &known);
                    }
                }
                let admission = admit_dynamic_identities(
                    &histories,
                    snapshot
                        .batteries
                        .iter()
                        .map(|battery| DeviceId::new(battery.id.clone())),
                );
                let sink = self.record_sink.as_deref();
                push_missing_dynamic_gaps(
                    &self.inner.battery_capacity_pct,
                    &admission.allowed,
                    stamp,
                    sink,
                    HistoryMetric::BatteryCapacityPct,
                );
                push_missing_dynamic_gaps(
                    &self.inner.battery_power_w,
                    &admission.allowed,
                    stamp,
                    sink,
                    HistoryMetric::BatteryPowerW,
                );
                push_missing_dynamic_gaps(
                    &self.inner.battery_health_pct,
                    &admission.allowed,
                    stamp,
                    sink,
                    HistoryMetric::BatteryHealthPct,
                );
                for battery in &snapshot.batteries {
                    if !admission.allowed.contains(battery.id.as_str()) {
                        continue;
                    }
                    ingest_dynamic_metric(DynamicMetricIngest {
                        histories: &self.inner.battery_capacity_pct,
                        capacity: self.inner.capacity,
                        device_id: DeviceId::new(battery.id.clone()),
                        generation: battery.device_generation.get(),
                        stamp,
                        value: battery.current_capacity_pct().map(f32::from),
                        measured_at: battery
                            .scalar_observations()
                            .capacity_pct
                            .last_success_ms()
                            .or(battery.device_state.last_success_ms),
                        sink,
                        metric: HistoryMetric::BatteryCapacityPct,
                        commit_gate: &commit_gate,
                    });
                    ingest_dynamic_metric(DynamicMetricIngest {
                        histories: &self.inner.battery_power_w,
                        capacity: self.inner.capacity,
                        device_id: DeviceId::new(battery.id.clone()),
                        generation: battery.device_generation.get(),
                        stamp,
                        value: battery.current_power_w().and_then(finite),
                        measured_at: battery
                            .scalar_observations()
                            .power_w
                            .last_success_ms()
                            .or(battery.device_state.last_success_ms),
                        sink,
                        metric: HistoryMetric::BatteryPowerW,
                        commit_gate: &commit_gate,
                    });
                    ingest_dynamic_metric(DynamicMetricIngest {
                        histories: &self.inner.battery_health_pct,
                        capacity: self.inner.capacity,
                        device_id: DeviceId::new(battery.id.clone()),
                        generation: battery.device_generation.get(),
                        stamp,
                        value: battery
                            .current_health_pct()
                            .map(|health| health as f32)
                            .and_then(finite),
                        measured_at: battery
                            .scalar_observations()
                            .health_pct()
                            .last_success_ms()
                            .or(battery.device_state.last_success_ms),
                        sink,
                        metric: HistoryMetric::BatteryHealthPct,
                        commit_gate: &commit_gate,
                    });
                }
                DynamicIngestionReport {
                    rejected_identity_capacity: admission.rejected,
                }
            },
        )
    }

    /// Append typed fan and temperature channels to runtime-device history.
    pub fn ingest_correlated_sensors(
        &self,
        stamp: CorrelatedTelemetryStamp,
        snapshot: &SensorCenterSnapshot,
    ) -> Result<DynamicIngestionReport, DynamicIngestionError> {
        self.inner.transact_dynamic(
            DynamicHistoryDomain::Sensor,
            stamp,
            (snapshot.timestamp_ms > 0).then_some(snapshot.timestamp_ms),
            || {
                let commit_gate = self.inner.dynamic_commit_gate(DynamicHistoryDomain::Sensor);
                let histories = [
                    &self.inner.fan_rpm,
                    &self.inner.fan_pwm_pct,
                    &self.inner.fan_temperature_c,
                ];
                if snapshot.state.status == taskmanager_core::DeviceStatus::Healthy {
                    let known = snapshot
                        .readings
                        .iter()
                        .map(|reading| DeviceId::new(reading.id()))
                        .collect::<HashSet<_>>();
                    for family in histories {
                        prune_dynamic_histories(family, &known);
                    }
                }
                let admission = admit_dynamic_identities(
                    &histories,
                    snapshot
                        .readings
                        .iter()
                        .filter(|reading| {
                            matches!(
                                reading.quantity(),
                                SensorQuantity::FanSpeed | SensorQuantity::Temperature
                            )
                        })
                        .map(|reading| DeviceId::new(reading.id())),
                );
                let sink = self.record_sink.as_deref();
                push_missing_dynamic_gaps(
                    &self.inner.fan_rpm,
                    &admission.allowed,
                    stamp,
                    sink,
                    HistoryMetric::FanRpm,
                );
                push_missing_dynamic_gaps(
                    &self.inner.fan_pwm_pct,
                    &admission.allowed,
                    stamp,
                    sink,
                    HistoryMetric::FanPwmPct,
                );
                push_missing_dynamic_gaps(
                    &self.inner.fan_temperature_c,
                    &admission.allowed,
                    stamp,
                    sink,
                    HistoryMetric::FanTemperatureC,
                );
                for reading in &snapshot.readings {
                    if !admission.allowed.contains(reading.id()) {
                        continue;
                    }
                    let measured_at = reading
                        .measurement_observation()
                        .last_success_ms()
                        .or(reading.state().last_success_ms);
                    match reading.quantity() {
                        SensorQuantity::FanSpeed => {
                            let rpm = reading.current_number().map(|value| value as f32);
                            ingest_dynamic_metric(DynamicMetricIngest {
                                histories: &self.inner.fan_rpm,
                                capacity: self.inner.capacity,
                                device_id: DeviceId::new(reading.id()),
                                generation: reading.device_generation().get(),
                                stamp,
                                value: rpm,
                                measured_at,
                                sink,
                                metric: HistoryMetric::FanRpm,
                                commit_gate: &commit_gate,
                            });
                            let pwm = match reading.measurement_observation().current_value() {
                                Some(SensorMagnitude::DutyCycle { value, maximum })
                                    if *maximum > 0 =>
                                {
                                    Some((*value as f32 * 100.0) / *maximum as f32)
                                }
                                _ => None,
                            };
                            ingest_dynamic_metric(DynamicMetricIngest {
                                histories: &self.inner.fan_pwm_pct,
                                capacity: self.inner.capacity,
                                device_id: DeviceId::new(reading.id()),
                                generation: reading.device_generation().get(),
                                stamp,
                                value: pwm,
                                measured_at,
                                sink,
                                metric: HistoryMetric::FanPwmPct,
                                commit_gate: &commit_gate,
                            });
                        }
                        SensorQuantity::Temperature => {
                            let temperature = reading
                                .current_number()
                                .map(|value| value as f32)
                                .and_then(finite);
                            ingest_dynamic_metric(DynamicMetricIngest {
                                histories: &self.inner.fan_temperature_c,
                                capacity: self.inner.capacity,
                                device_id: DeviceId::new(reading.id()),
                                generation: reading.device_generation().get(),
                                stamp,
                                value: temperature,
                                measured_at,
                                sink,
                                metric: HistoryMetric::FanTemperatureC,
                                commit_gate: &commit_gate,
                            });
                        }
                        SensorQuantity::Unknown
                        | SensorQuantity::Power
                        | SensorQuantity::Voltage
                        | SensorQuantity::Current
                        | SensorQuantity::Energy
                        | SensorQuantity::RelativeHumidity
                        | SensorQuantity::PwmDutyCycle
                        | SensorQuantity::Intrusion
                        | SensorQuantity::Opaque(_) => {}
                    }
                }
                DynamicIngestionReport {
                    rejected_identity_capacity: admission.rejected,
                }
            },
        )
    }
}

struct DynamicIdentityAdmission {
    allowed: HashSet<DeviceId>,
    rejected: usize,
}

/// Preserve every retained identity during a partial refresh, then admit new
/// identities in stable order only while the shared domain ceiling has room.
/// The same decision is reused by every scalar family so battery capacity,
/// power and health (or fan RPM/PWM/temperature) cannot drift into different
/// key sets.
fn admit_dynamic_identities(
    histories: &[&Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>],
    incoming: impl IntoIterator<Item = DeviceId>,
) -> DynamicIdentityAdmission {
    let mut retained = HashSet::new();
    for histories in histories {
        retained.extend(
            histories
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .keys()
                .cloned(),
        );
    }
    let incoming = incoming.into_iter().collect::<BTreeSet<_>>();
    let mut allowed = HashSet::with_capacity(incoming.len().min(MAX_DYNAMIC_HISTORY_IDENTITIES));
    let mut new_identities = 0_usize;
    let mut rejected = 0_usize;
    for device_id in incoming {
        if retained.contains(&device_id) {
            allowed.insert(device_id);
        } else if retained.len().saturating_add(new_identities) < MAX_DYNAMIC_HISTORY_IDENTITIES {
            new_identities = new_identities.saturating_add(1);
            allowed.insert(device_id);
        } else {
            rejected = rejected.saturating_add(1);
        }
    }
    DynamicIdentityAdmission { allowed, rejected }
}

fn push_missing_dynamic_gaps(
    histories: &Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>,
    current: &HashSet<DeviceId>,
    stamp: CorrelatedTelemetryStamp,
    sink: Option<&dyn HistoryRecordSink>,
    metric: HistoryMetric,
) {
    let histories = histories
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (device_id, history) in histories.iter() {
        if current.contains(device_id) {
            continue;
        }
        history.metric.push(stamp, None, None);
        if let Some(sink) = sink {
            sink.record_sample(
                HistorySeriesKey::for_device(metric, device_id.clone()),
                HistoricalSample {
                    revision: stamp.revision(),
                    completed_at_ms: stamp.completed_at_ms(),
                    measured_at_ms: None,
                    value: None,
                },
            );
        }
    }
}

struct DynamicMetricIngest<'a> {
    histories: &'a Mutex<HashMap<DeviceId, DeviceMetricHistory<f32>>>,
    capacity: usize,
    device_id: DeviceId,
    generation: u64,
    stamp: CorrelatedTelemetryStamp,
    value: Option<f32>,
    measured_at: Option<u64>,
    sink: Option<&'a dyn HistoryRecordSink>,
    metric: HistoryMetric,
    commit_gate: &'a Arc<Mutex<()>>,
}

fn ingest_dynamic_metric(transaction: DynamicMetricIngest<'_>) {
    let DynamicMetricIngest {
        histories,
        capacity,
        device_id,
        generation,
        stamp,
        value,
        measured_at,
        sink,
        metric,
        commit_gate,
    } = transaction;
    if generation == 0 || device_id.as_str().is_empty() {
        return;
    }
    let value = value.filter(|value| value.is_finite());
    let persisted_key = sink.map(|_| HistorySeriesKey::for_device(metric, device_id.clone()));
    let mut histories = histories
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !histories.contains_key(&device_id) && histories.len() >= MAX_DYNAMIC_HISTORY_IDENTITIES {
        return;
    }
    let history = histories
        .entry(device_id)
        .or_insert_with(|| DeviceMetricHistory::new(generation, capacity, commit_gate.clone()));
    if history.generation != generation {
        *history = DeviceMetricHistory::new(generation, capacity, commit_gate.clone());
    }
    history.metric.push(stamp, measured_at, value);
    if let (Some(sink), Some(key)) = (sink, persisted_key) {
        sink.record_sample(
            key,
            HistoricalSample {
                revision: stamp.revision(),
                completed_at_ms: stamp.completed_at_ms(),
                measured_at_ms: measured_at,
                value: value.map(f64::from),
            },
        );
    }
}

fn prune_dynamic_histories<T>(
    histories: &Mutex<HashMap<DeviceId, DeviceMetricHistory<T>>>,
    known: &HashSet<DeviceId>,
) {
    histories
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(|device_id, _| known.contains(device_id));
}
