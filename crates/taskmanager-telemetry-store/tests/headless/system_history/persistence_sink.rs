//! Roadmap #4 (R1) persistence-sink regressions: the optional
//! `HistoryRecordSink` mirror must agree with the rings sample-for-sample —
//! same acceptance order, same values, same explicit gaps — and a rejected
//! observation must never reach the sink.

use std::sync::{Arc, Mutex};

use taskmanager_core::{
    BatteryInfo, BatteryScalarObservations, CpuMetrics, CpuScalarObservations,
    CpuTelemetryObservation, DeviceGeneration, DeviceState, FailureKind, GpuMetrics,
    GpuScalarObservations, GpuTelemetryObservation, HistoricalSample, HistoryMetric,
    HistoryRecordSink, HistorySeriesKey, MemoryTelemetryObservation, PowerSupplySnapshot,
    ScalarObservation,
};

use super::*;
use crate::TelemetryStore;

/// Captures every mirrored record in acceptance order.
#[derive(Default)]
struct CapturingSink {
    records: Mutex<Vec<(HistorySeriesKey, HistoricalSample)>>,
}

impl HistoryRecordSink for CapturingSink {
    fn record_sample(&self, key: HistorySeriesKey, sample: HistoricalSample) {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((key, sample));
    }
}

fn captured(sink: &CapturingSink) -> Vec<(HistorySeriesKey, HistoricalSample)> {
    sink.records
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// The records the sink holds for one metric, in acceptance order.
fn series_records(sink: &CapturingSink, metric: HistoryMetric) -> Vec<HistoricalSample> {
    captured(sink)
        .into_iter()
        .filter(|(key, _)| key.metric() == metric)
        .map(|(_, sample)| sample)
        .collect()
}

/// 1000 B total with `used_pct`% used and a half-used 200 B swap, so the
/// observed percentages are exact and host-independent.
fn memory_observation(used_pct: f32) -> MemoryTelemetryObservation {
    MemoryTelemetryObservation::current(
        observed_memory(1_000, used_pct as u64 * 10, 200, 100, 10),
        10,
        Vec::new(),
    )
}

#[test]
fn attached_sink_mirrors_accepted_samples_in_ring_order() {
    let sink = Arc::new(CapturingSink::default());
    let (_store, ingestor) = {
        let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
        (store, ingestor.with_record_sink(sink.clone()))
    };

    let cpu = CpuTelemetryObservation::current(
        CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(33.0, 10),
            core_usage_group: available_group([10.0, 90.0], 10),
            ..Default::default()
        }),
        10,
        Vec::new(),
    );
    ingestor
        .ingest_correlated_cpu(stamp_at(1, 1_000), &cpu)
        .expect("accepted cpu observation");
    ingestor
        .ingest_correlated_memory(stamp_at(2, 2_000), &memory_observation(56.0))
        .expect("accepted memory observation");

    let usage = series_records(&sink, HistoryMetric::CpuUsagePct);
    assert_eq!(
        usage
            .iter()
            .map(|sample| (sample.revision, sample.value))
            .collect::<Vec<_>>(),
        vec![(1, Some(33.0))],
        "the ring sample and the mirrored record must agree exactly"
    );
    assert_eq!(usage[0].completed_at_ms, 1_000);

    let memory = series_records(&sink, HistoryMetric::MemoryUsedPct);
    assert_eq!(
        memory.iter().map(|sample| sample.value).collect::<Vec<_>>(),
        vec![Some(56.0)]
    );

    // The per-core fan-out mirrors one series per core index.
    let core_zero = captured(&sink)
        .into_iter()
        .find(|(key, _)| {
            key.metric() == HistoryMetric::CpuCoreUsagePct && key.core_index() == Some(0)
        })
        .map(|(_, sample)| sample.value);
    let core_one = captured(&sink)
        .into_iter()
        .find(|(key, _)| {
            key.metric() == HistoryMetric::CpuCoreUsagePct && key.core_index() == Some(1)
        })
        .map(|(_, sample)| sample.value);
    assert_eq!(core_zero, Some(Some(10.0)));
    assert_eq!(core_one, Some(Some(90.0)));
}

#[test]
fn gpu_device_series_mirror_usage_and_point_derived_scalars() {
    let sink = Arc::new(CapturingSink::default());
    let device_id = "gpu:pci:0000:01:00.0";
    let (_store, ingestor) = {
        let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
        (store, ingestor.with_record_sink(sink.clone()))
    };

    let mut gpu = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(77.0, 10),
        temperature_c: ScalarObservation::available(61.0, 10),
        power_w: ScalarObservation::available(28.5, 10),
        frequency_mhz: ScalarObservation::available(2_100, 10),
        ..Default::default()
    });
    gpu.device_id = device_id.to_owned();
    gpu.device_generation = DeviceGeneration::new(1);
    gpu.device_state = DeviceState::healthy(10);
    let observation = GpuTelemetryObservation::current(
        vec![gpu],
        10,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Present, 1),
    );
    ingestor
        .ingest_correlated_gpu(stamp(1), &observation)
        .expect("accepted gpu observation");

    let expected_device = DeviceId::new(device_id);
    for (metric, value) in [
        (HistoryMetric::GpuUsagePct, Some(77.0)),
        (HistoryMetric::GpuTemperatureC, Some(61.0)),
        (HistoryMetric::GpuPowerW, Some(28.5)),
        (HistoryMetric::GpuFrequencyMhz, Some(2_100.0)),
    ] {
        let records = captured(&sink)
            .into_iter()
            .filter(|(key, _)| key.metric() == metric && key.device() == Some(&expected_device))
            .map(|(_, sample)| sample.value)
            .collect::<Vec<_>>();
        assert_eq!(
            records,
            vec![value],
            "{metric:?} must be mirrored exactly once per accepted tick"
        );
    }
}

#[test]
fn domain_unavailable_mirrors_explicit_gaps_for_seen_series() {
    let sink = Arc::new(CapturingSink::default());
    let (_store, ingestor) = {
        let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
        (store, ingestor.with_record_sink(sink.clone()))
    };

    ingestor
        .ingest_correlated_memory(stamp_at(1, 1_000), &memory_observation(50.0))
        .expect("accepted live observation");
    ingestor
        .ingest_correlated_unavailable(
            stamp_at(2, 2_000),
            SystemHistoryDomain::Memory,
            FailureKind::PermissionDenied,
        )
        .expect("the unavailable domain advances with a gap");

    let memory = series_records(&sink, HistoryMetric::MemoryUsedPct);
    assert_eq!(
        memory
            .iter()
            .map(|sample| (sample.revision, sample.value, sample.measured_at_ms))
            .collect::<Vec<_>>(),
        vec![(1, Some(50.0), Some(10)), (2, None, None)],
        "the failed tick must mirror as an explicit gap, not a zero or a skip"
    );
}

#[test]
fn rejected_revisions_never_reach_the_sink() {
    let sink = Arc::new(CapturingSink::default());
    let (_store, ingestor) = {
        let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
        (store, ingestor.with_record_sink(sink.clone()))
    };

    ingestor
        .ingest_correlated_memory(stamp_at(5, 5_000), &memory_observation(12.0))
        .expect("first acceptance");
    let rejected = ingestor.ingest_correlated_memory(stamp_at(5, 5_000), &memory_observation(99.0));
    assert!(rejected.is_err(), "a repeated revision must be rejected");

    let memory = series_records(&sink, HistoryMetric::MemoryUsedPct);
    assert_eq!(
        memory.iter().map(|sample| sample.value).collect::<Vec<_>>(),
        vec![Some(12.0)],
        "the rejected observation's value must never reach the mirror"
    );
}

#[test]
fn battery_dynamic_history_mirrors_to_the_sink() {
    let sink = Arc::new(CapturingSink::default());
    let (_store, ingestor) = {
        let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
        (store, ingestor.with_record_sink(sink.clone()))
    };

    let battery_id = "power-supply:BAT0";
    let mut battery = BatteryInfo::new(battery_id, DeviceState::healthy(100));
    battery.device_generation = DeviceGeneration::new(1);
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(73, 100),
        power_w: ScalarObservation::available(12.5, 100),
        energy_full_uwh: ScalarObservation::available(49_000_000.0, 100),
        energy_full_design_uwh: ScalarObservation::available(56_000_000.0, 100),
        ..Default::default()
    });
    ingestor
        .ingest_correlated_power_supplies(
            stamp_at(1, 110),
            &PowerSupplySnapshot {
                timestamp_ms: 100,
                batteries: vec![battery],
                ..Default::default()
            },
        )
        .expect("first dynamic power event should be accepted");

    let expected_device = DeviceId::new(battery_id);
    let capacity = captured(&sink)
        .into_iter()
        .find(|(key, _)| {
            key.metric() == HistoryMetric::BatteryCapacityPct
                && key.device() == Some(&expected_device)
        })
        .map(|(_, sample)| sample.value);
    let power = captured(&sink)
        .into_iter()
        .find(|(key, _)| {
            key.metric() == HistoryMetric::BatteryPowerW && key.device() == Some(&expected_device)
        })
        .map(|(_, sample)| sample.value);
    let health = captured(&sink)
        .into_iter()
        .find(|(key, _)| {
            key.metric() == HistoryMetric::BatteryHealthPct
                && key.device() == Some(&expected_device)
        })
        .map(|(_, sample)| sample.value);
    assert_eq!(capacity, Some(Some(73.0)));
    assert_eq!(power, Some(Some(12.5)));
    // The core full/design rule (49/56 Wh × 100) reaches the persisted
    // mirror as one derived health sample.
    assert_eq!(health, Some(Some(87.5)));
}
