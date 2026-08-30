use super::*;

/// The helper reads exactly one path root: the kernel's per-CPU MSR nodes.
/// Locking the constant guards the "no file access beyond `/dev/cpu/N/msr`"
/// promise at the value level.
#[test]
fn the_only_read_root_is_the_dev_cpu_directory() {
    assert_eq!(DEV_CPU_ROOT, "/dev/cpu");
}

/// An `Outcome::Error` carries the kind through to a non-zero exit code via
/// `ErrorKindJson::exit_code` (asserted exhaustively in json.rs); here we
/// confirm the kind round-trips through the Outcome construction.
#[test]
fn outcome_error_carries_typed_kind() {
    let outcome = Outcome::Error(ErrorKindJson::NoMsr, "no msr driver".to_string());
    match outcome {
        Outcome::Error(kind, detail) => {
            assert_eq!(kind, ErrorKindJson::NoMsr);
            assert_eq!(detail, "no msr driver");
            assert_eq!(kind.exit_code(), 3);
        }
        Outcome::Success { .. } => panic!("expected error outcome"),
    }
}

/// The success envelope built by `emit` (constructed here the same way)
/// serializes with `packages` and without `status` — the consumer's success
/// key. Exercises the real serialization path of the emit payload.
#[test]
fn success_payload_serializes_with_packages_and_no_status() {
    let reading = PackageReadingJson {
        cpu: 0,
        bclk_mhz: None,
        temperature_c: Some(58.0),
        multiplier: Some(45.0),
        multiplier_min: Some(8.0),
        multiplier_max: Some(55.0),
        vcore_v: None,
    };
    let envelope = SuccessEnvelope {
        schema: SCHEMA_VERSION,
        packages: vec![reading],
    };
    let json = serde_json::to_string(&envelope).expect("serializes");
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(
        value.get("packages").is_some(),
        "consumer keys SUCCESS off packages"
    );
    assert!(
        value.get("status").is_none(),
        "SUCCESS never carries status"
    );
    assert_eq!(value["packages"][0]["cpu"], 0);
    assert_eq!(value["packages"][0]["temperature_c"], 58.0);
    assert!(value["packages"][0]["bclk_mhz"].is_null());
}

/// The hand-rolled serializer-fallback envelope is valid JSON despite quotes
/// and backslashes in the detail, and stays an `open_failed` error.
#[test]
fn serialize_error_fallback_stays_valid_json_with_escapable_detail() {
    let (json, kind) = serialize_error_json("boom \"quoted\" \\ path");
    assert_eq!(kind, Some(ErrorKindJson::OpenFailed));
    let value: serde_json::Value = serde_json::from_str(&json)
        .unwrap_or_else(|error| panic!("fallback JSON invalid: {error}"));
    assert_eq!(value["status"], "error");
    assert_eq!(value["kind"], "open_failed");
    assert_eq!(value["detail"], "boom \"quoted\" \\ path");
}
