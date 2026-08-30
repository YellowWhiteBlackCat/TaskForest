use super::*;

/// The helper reads exactly one path: the kernel's powercap class tree.
/// Locking the constant guards the "no file access beyond
/// /sys/class/powercap" promise at the value level.
#[test]
fn the_only_read_root_is_the_powercap_class_directory() {
    assert_eq!(POWERCAP_ROOT, "/sys/class/powercap");
}

/// The contract reports the window the rates were computed over; the main
/// path pins it to the product's standard rate window.
#[test]
fn the_sample_window_is_one_second() {
    assert_eq!(SAMPLE_MS, 1000);
}

/// An `Outcome::Error` carries the kind through to a non-zero exit code via
/// `ErrorKindJson::exit_code` (asserted exhaustively in json.rs); here we
/// confirm the kind round-trips through the Outcome construction.
#[test]
fn outcome_error_carries_typed_kind() {
    let outcome = Outcome::Error(ErrorKindJson::NoRapl, "no packages".to_string());
    match outcome {
        Outcome::Error(kind, detail) => {
            assert_eq!(kind, ErrorKindJson::NoRapl);
            assert_eq!(detail, "no packages");
            assert_eq!(kind.exit_code(), 3);
        }
        Outcome::Success { .. } => panic!("expected error outcome"),
    }
}

/// The success envelope built by `emit` (constructed here the same way)
/// serializes with `packages` and without `status` — the consumer's success
/// key — and carries finite, non-negative watt figures.
#[test]
fn success_payload_serializes_with_packages_and_no_status() {
    let envelope = SuccessEnvelope {
        schema: SCHEMA_VERSION,
        sample_ms: SAMPLE_MS,
        packages: vec![PackageJson {
            name: "Core".to_owned(),
            power_w: 1.5,
            energy_delta_uj: 1_500_000,
        }],
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
    assert_eq!(value["sample_ms"], 1000);
    let package = &value["packages"][0];
    assert_eq!(package["power_w"], 1.5);
    assert!(package["power_w"].as_f64().expect("finite") >= 0.0);
    assert_eq!(package["energy_delta_uj"], 1_500_000);
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
