use super::*;

fn sample_packages() -> Vec<RaplPackageRow> {
    vec![
        RaplPackageRow {
            name: "package-0".to_owned(),
            power_w: 12.5,
            energy_delta_uj: 12_500_000,
        },
        RaplPackageRow {
            name: "package-1".to_owned(),
            power_w: 0.0,
            energy_delta_uj: 0,
        },
    ]
}

#[test]
fn success_carries_readings_and_never_a_failure_tag() {
    let snapshot = RaplPowerSnapshot::success(1000, sample_packages());
    assert!(snapshot.is_success());
    assert_eq!(snapshot.sample_ms, 1000);
    assert_eq!(snapshot.packages.len(), 2);
    assert_eq!(snapshot.failure, None);
}

#[test]
fn failed_never_carries_a_fabricated_watt_figure() {
    let snapshot = RaplPowerSnapshot::failed(FailureKind::PermissionDenied, "prompt refused");
    assert!(!snapshot.is_success());
    assert_eq!(snapshot.sample_ms, 0);
    assert!(snapshot.packages.is_empty());
    let failure = snapshot
        .failure
        .as_ref()
        .expect("failure tag must be present");
    assert_eq!(failure.kind, FailureKind::PermissionDenied);
    assert_eq!(failure.detail, "prompt refused");
}

#[test]
fn the_honest_empty_package_list_is_success() {
    // A host with the powercap tree but zero top-level packages reports an
    // empty success, not a failure — mirroring the helper contract.
    let snapshot = RaplPowerSnapshot::success(1000, Vec::new());
    assert!(snapshot.is_success());
    assert!(snapshot.packages.is_empty());
}

#[test]
fn snapshots_round_trip_through_serde() {
    let snapshot = RaplPowerSnapshot::success(1000, sample_packages());
    let json = serde_json::to_string(&snapshot).expect("snapshot is serializable");
    let decoded: RaplPowerSnapshot = serde_json::from_str(&json).expect("snapshot round-trips");
    assert_eq!(decoded, snapshot);

    let failed = RaplPowerSnapshot::failed(FailureKind::MissingDependency, "helper absent");
    let json = serde_json::to_string(&failed).expect("failed snapshot is serializable");
    let decoded: RaplPowerSnapshot =
        serde_json::from_str(&json).expect("failed snapshot round-trips");
    assert_eq!(decoded, failed);
}
