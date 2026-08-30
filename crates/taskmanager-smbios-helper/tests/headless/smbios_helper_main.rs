use super::*;

/// The helper reads exactly one path: the kernel's DMI entries root. Locking
/// the constant guards the "no file access beyond `/sys/firmware/dmi/entries`"
/// promise at the value level.
#[test]
fn the_only_read_root_is_the_dmi_entries_directory() {
    assert_eq!(DMI_ENTRIES_ROOT, "/sys/firmware/dmi/entries");
}

/// An `Outcome::Error` carries the kind through to a non-zero exit code via
/// `ErrorKindJson::exit_code` (asserted exhaustively in json.rs); here we
/// confirm the kind round-trips through the Outcome construction.
#[test]
fn outcome_error_carries_typed_kind() {
    let outcome = Outcome::Error(ErrorKindJson::NoDmi, "no entries".to_string());
    match outcome {
        Outcome::Error(kind, detail) => {
            assert_eq!(kind, ErrorKindJson::NoDmi);
            assert_eq!(detail, "no entries");
            assert_eq!(kind.exit_code(), 3);
        }
        Outcome::Success { .. } => panic!("expected error outcome"),
    }
}

/// The success envelope built by `emit` (constructed here the same way)
/// serializes with `modules` and without `status` — the consumer's success
/// key. Exercises the real serialization path of the emit payload.
#[test]
fn success_payload_serializes_with_modules_and_no_status() {
    let module = MemoryModuleJson {
        slot: 0,
        size_mb: Some(16384),
        speed_mts: Some(5600),
        configured_speed_mts: Some(4800),
        manufacturer: Some("Crucial".to_owned()),
        serial_number: None,
        part_number: None,
        form_factor: Some("SODIMM"),
        memory_type: Some("DDR4"),
        locator: None,
    };
    let envelope = SuccessEnvelope {
        schema: SCHEMA_VERSION,
        slots_total: 2,
        slots_used: 1,
        modules: vec![module],
        identity: Some(DmiIdentityJson {
            system_serial: Some("PF3XYZ42".to_owned()),
            ..DmiIdentityJson::default()
        }),
    };
    let json = serde_json::to_string(&envelope).expect("serializes");
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(
        value.get("modules").is_some(),
        "consumer keys SUCCESS off modules"
    );
    assert!(
        value.get("status").is_none(),
        "SUCCESS never carries status"
    );
    assert_eq!(value["slots_total"], 2);
    assert_eq!(value["modules"][0]["size_mb"], 16384);
    assert!(value["modules"][0]["serial_number"].is_null());
    assert_eq!(value["identity"]["system_serial"], "PF3XYZ42");
    assert!(value["identity"]["system_uuid"].is_null());
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
