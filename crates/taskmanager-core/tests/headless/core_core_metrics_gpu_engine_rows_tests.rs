use super::*;
use crate::core::GpuEngineKind;

fn sample_engines() -> Vec<GpuEngineMetric> {
    vec![
        GpuEngineMetric {
            name: "Render Ring".to_owned(),
            kind: GpuEngineKind::Unknown,
            utilization_pct: 41.5,
        },
        GpuEngineMetric {
            name: "Blitter".to_owned(),
            kind: GpuEngineKind::Copy,
            utilization_pct: 0.0,
        },
    ]
}

#[test]
fn success_carries_rows_and_never_a_failure_tag() {
    let snapshot = GpuEngineRowsSnapshot::success(DeviceId::new("gpu:0"), sample_engines());
    assert!(snapshot.is_success());
    assert_eq!(snapshot.engines.len(), 2);
    assert_eq!(snapshot.failure, None);
}

#[test]
fn failed_never_carries_a_fabricated_row() {
    let snapshot = GpuEngineRowsSnapshot::failed(
        DeviceId::new("gpu:0"),
        FailureKind::PermissionDenied,
        "user dismissed the prompt",
    );
    assert!(!snapshot.is_success());
    assert!(snapshot.engines.is_empty());
    let failure = snapshot
        .failure
        .as_ref()
        .expect("failure tag must be present");
    assert_eq!(failure.kind, FailureKind::PermissionDenied);
    assert_eq!(failure.detail, "user dismissed the prompt");
}

#[test]
fn snapshots_round_trip_through_serde() {
    let snapshot =
        GpuEngineRowsSnapshot::failed(DeviceId::new("gpu:1"), FailureKind::Unsupported, "off-host");
    let json = serde_json::to_string(&snapshot).expect("snapshot is serializable");
    let decoded: GpuEngineRowsSnapshot = serde_json::from_str(&json).expect("snapshot round-trips");
    assert_eq!(decoded, snapshot);
}
