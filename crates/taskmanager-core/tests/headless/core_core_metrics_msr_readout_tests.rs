use super::*;

fn sample_packages() -> Vec<MsrPackageReadout> {
    vec![
        MsrPackageReadout {
            cpu: 0,
            bclk_mhz: None,
            temperature_c: Some(54.5),
            multiplier: Some(42.0),
            multiplier_min: Some(8.0),
            multiplier_max: Some(58.0),
            vcore_v: Some(1.219),
        },
        MsrPackageReadout {
            cpu: 1,
            bclk_mhz: None,
            // A node whose CPU does not implement a register keeps it typed
            // absent — never a fabricated zero.
            temperature_c: None,
            multiplier: None,
            multiplier_min: None,
            multiplier_max: None,
            vcore_v: None,
        },
    ]
}

#[test]
fn success_carries_readings_and_never_a_failure_tag() {
    let snapshot = MsrReadoutSnapshot::success(sample_packages());
    assert!(snapshot.is_success());
    assert_eq!(snapshot.packages.len(), 2);
    assert_eq!(snapshot.failure, None);
    assert_eq!(snapshot.packages[0].temperature_c, Some(54.5));
    assert_eq!(snapshot.packages[0].vcore_v, Some(1.219));
    assert_eq!(snapshot.packages[1].temperature_c, None);
}

#[test]
fn failed_never_carries_a_fabricated_register_value() {
    let snapshot = MsrReadoutSnapshot::failed(FailureKind::PermissionDenied, "prompt refused");
    assert!(!snapshot.is_success());
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
    // A host with the msr driver loaded but zero enumerated nodes reports an
    // empty success, not a failure — mirroring the helper contract.
    let snapshot = MsrReadoutSnapshot::success(Vec::new());
    assert!(snapshot.is_success());
    assert!(snapshot.packages.is_empty());
}

#[test]
fn snapshots_round_trip_through_serde() {
    let snapshot = MsrReadoutSnapshot::success(sample_packages());
    let json = serde_json::to_string(&snapshot).expect("snapshot is serializable");
    let decoded: MsrReadoutSnapshot = serde_json::from_str(&json).expect("snapshot round-trips");
    assert_eq!(decoded, snapshot);

    let failed = MsrReadoutSnapshot::failed(FailureKind::MissingDependency, "helper absent");
    let json = serde_json::to_string(&failed).expect("failed snapshot is serializable");
    let decoded: MsrReadoutSnapshot =
        serde_json::from_str(&json).expect("failed snapshot round-trips");
    assert_eq!(decoded, failed);
}
