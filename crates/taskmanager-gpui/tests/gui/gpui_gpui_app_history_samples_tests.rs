use super::*;
use taskmanager_telemetry_store::CorrelatedTelemetryStamp;

fn stamp(revision: u64) -> CorrelatedTelemetryStamp {
    CorrelatedTelemetryStamp::from_accepted_event(revision, revision * 10)
        .expect("test revisions are non-zero")
}

fn sample<T>(
    revision: u64,
    measured_at_ms: Option<u64>,
    value: Option<T>,
) -> CorrelatedMetricSample<T> {
    CorrelatedMetricSample {
        stamp: stamp(revision),
        measured_at_ms,
        value,
    }
}

#[test]
fn measured_zero_survives_while_every_missing_state_becomes_a_gap() {
    let values = f32_samples(&[
        sample(1, Some(1), Some(0.0)),
        sample(2, None, Some(7.0)),
        sample(3, Some(3), None),
        sample(4, Some(4), Some(f32::INFINITY)),
    ]);

    assert_eq!(values[0], 0.0);
    assert!(values[1..].iter().all(|value| value.is_nan()));
}

#[test]
fn u64_conversion_is_finite_at_boundaries_and_preserves_network_units() {
    let raw = u64_samples(
        &[
            sample(1, Some(1), Some(0)),
            sample(2, Some(2), Some(u64::MAX)),
        ],
        1.0,
    );
    assert_eq!(raw[0], 0.0);
    assert!(raw[1].is_finite());
    assert!(raw[1] >= 1.8e19_f32);

    let network = u64_samples(
        &[
            sample(1, Some(1), Some(0)),
            sample(2, Some(2), Some(2_500_000)),
            sample(3, Some(3), Some(u64::MAX)),
            sample(4, None, Some(1_000_000)),
        ],
        DECIMAL_BYTES_PER_MEGABYTE,
    );
    assert_eq!(network[0], 0.0);
    assert_eq!(network[1], 2.5);
    assert!(network[2].is_finite());
    assert!(network[2] > 18_000_000_000_000.0);
    assert!(network[3].is_nan());
}

#[test]
fn device_reinsert_generation_cannot_reuse_previous_samples() {
    let previous = vec![sample(1, Some(1), Some(73.0))];

    assert!(generation_scoped_samples(1, 2, previous).is_none());
    assert!(generation_scoped_samples(2, 2, vec![sample(2, Some(2), Some(0.0))]).is_some());
}

#[test]
fn storage_temperature_projection_is_identity_and_generation_scoped() {
    use std::collections::BTreeMap;

    use crate::core::{DeviceLifecycle, DevicePresence, DeviceState, StorageTelemetryObservation};
    use taskmanager_telemetry_store::TelemetryStore;

    let lifecycle = |device_id: &str| {
        (
            DeviceId::new(device_id),
            DeviceLifecycle {
                presence: DevicePresence::Present,
                state: DeviceState::healthy(10),
                generation: 1,
                first_seen_ms: Some(10),
                last_seen_ms: Some(10),
                absent_since_ms: None,
            },
        )
    };
    let disk = |device_id: &str, temperature_c| {
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .device_id(device_id.to_owned())
            .device_generation(DeviceGeneration::new(1))
            .device_state(DeviceState::healthy(10))
            .smart_availability(crate::core::SmartAvailability::Available)
            .smart_state(DeviceState::healthy(10))
            .smart_temperature_c(Some(temperature_c))
            .build()
    };
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    ingestor
        .ingest_correlated_storage(
            stamp(1),
            &StorageTelemetryObservation::current(
                vec![disk("disk:a", 32.0), disk("disk:b", 62.0)],
                10,
                Vec::new(),
                Vec::new(),
                BTreeMap::from([lifecycle("disk:a"), lifecycle("disk:b")]),
            ),
        )
        .expect("two-device storage history");

    assert_eq!(
        storage_temperature_samples(&store.system_history, "disk:a", DeviceGeneration::new(1),)
            .as_ref(),
        [32.0]
    );
    assert!(
        storage_temperature_samples(&store.system_history, "disk:a", DeviceGeneration::new(2),)
            .is_empty(),
        "a reattached generation cannot inherit disk A's old curve"
    );
}

#[test]
fn engine_projection_keeps_missing_engine_samples_as_gaps() {
    use std::collections::BTreeMap;

    use crate::core::{
        DeviceLifecycle, DevicePresence, DeviceState, GpuEngine, GpuEngineKind,
        GpuTelemetryObservation,
    };
    use taskmanager_telemetry_store::{CorrelatedTelemetryStamp, TelemetryStore};

    let device_id = "gpu:fixture:engine-history";
    let lifecycle = || {
        BTreeMap::from([(
            DeviceId::new(device_id),
            DeviceLifecycle {
                presence: DevicePresence::Present,
                state: DeviceState::healthy(1),
                generation: 1,
                first_seen_ms: Some(1),
                last_seen_ms: Some(1),
                absent_since_ms: None,
            },
        )])
    };
    let observation = |observed_at_ms, engines| {
        let mut gpu = GpuMetrics::new(device_id, "Fixture GPU");
        gpu.device_generation = DeviceGeneration::new(1);
        gpu.device_state = DeviceState::healthy(observed_at_ms);
        gpu.engines = engines;
        GpuTelemetryObservation::current(
            vec![gpu],
            observed_at_ms,
            Vec::new(),
            Vec::new(),
            lifecycle(),
        )
    };
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    for (revision, observed_at_ms, engines) in [
        (
            1,
            10,
            vec![
                GpuEngine {
                    name: "Render/3D".into(),
                    kind: GpuEngineKind::Render,
                    usage_pct: 42.0,
                },
                GpuEngine {
                    name: "Video Decode".into(),
                    kind: GpuEngineKind::VideoDecode,
                    usage_pct: 7.0,
                },
            ],
        ),
        (
            2,
            20,
            vec![GpuEngine {
                name: "Render/3D".into(),
                kind: GpuEngineKind::Render,
                usage_pct: 0.0,
            }],
        ),
        (3, 30, Vec::new()),
    ] {
        ingestor
            .ingest_correlated_gpu(
                CorrelatedTelemetryStamp::from_accepted_event(revision, observed_at_ms + 1)
                    .unwrap(),
                &observation(observed_at_ms, engines),
            )
            .unwrap();
    }

    let render = gpu_engine_samples(
        &store.system_history,
        device_id,
        DeviceGeneration::new(1),
        "Render/3D",
    );
    let decode = gpu_engine_samples(
        &store.system_history,
        device_id,
        DeviceGeneration::new(1),
        "Video Decode",
    );
    assert_eq!(render[..2], [42.0, 0.0]);
    assert!(render[2].is_nan());
    assert_eq!(decode[0], 7.0);
    assert!(decode[1].is_nan() && decode[2].is_nan());
    let engine_history = store
        .system_history
        .gpu_engine_metrics(&DeviceId::new(device_id))
        .expect("typed engine history exists");
    let engine_samples = engine_history.samples();
    let engine_point = engine_samples[0]
        .value
        .as_ref()
        .expect("current engine point exists");
    assert_eq!(engine_point.engines[1].kind, GpuEngineKind::VideoDecode);
    assert_eq!(
        gpu_engine_series_names(
            &store.system_history,
            &observation(31, Vec::new()).current_value().unwrap()[0],
        ),
        vec!["Render/3D".to_string(), "Video Decode".to_string()]
    );
}
