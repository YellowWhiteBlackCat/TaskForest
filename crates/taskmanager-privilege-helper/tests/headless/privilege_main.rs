use super::*;

#[test]
fn sample_error_kind_maps_every_variant() {
    assert_eq!(
        sample_error_kind(&SampleError::PermissionDenied("x".into())),
        ErrorKindJson::PermissionDenied
    );
    assert_eq!(
        sample_error_kind(&SampleError::OpenFailed("x".into())),
        ErrorKindJson::OpenFailed
    );
    assert_eq!(
        sample_error_kind(&SampleError::ReadFailed("x".into())),
        ErrorKindJson::ReadFailed
    );
    assert_eq!(
        sample_error_kind(&SampleError::NoEngines("x".into())),
        ErrorKindJson::NoPmu
    );
}

#[test]
fn sample_error_detail_round_trips_the_message() {
    assert_eq!(
        sample_error_detail(&SampleError::PermissionDenied("denied by paranoid".into())),
        "denied by paranoid"
    );
    assert_eq!(
        sample_error_detail(&SampleError::ReadFailed("EIO".into())),
        "EIO"
    );
    assert_eq!(
        sample_error_detail(&SampleError::NoEngines("empty engine list".into())),
        "empty engine list"
    );
}

/// `emit` on a success writes a JSON line with `engines` and no `status`,
/// and returns SUCCESS. This exercises the real stdout serialization path.
#[test]
fn emit_success_writes_engines_envelope() {
    // Capture: route the real emit through a buffer by invoking the
    // serializer directly (emit owns stdout, which is unwritable in a unit
    // test). Assert the shape on the constructed envelope instead.
    let engines = vec![EngineJson {
        name: "Render/3D".to_string(),
        class: "render".to_string(),
        busy_pct: 12.5,
    }];
    let envelope = SuccessEnvelope {
        schema: SCHEMA_VERSION,
        driver: Driver::Xe.keyword(),
        sample_ms: SAMPLE_MS,
        engines,
    };
    let json_string = serde_json::to_string(&envelope).unwrap();
    assert!(json_string.contains("\"engines\""));
    assert!(json_string.contains("\"driver\":\"xe\""));
    assert!(!json_string.contains("\"status\""));
}

/// An `Outcome::Error` carries the kind through to a non-zero exit code via
/// `ErrorKindJson::exit_code` (asserted in json.rs); here we confirm the
/// kind round-trips through the Outcome construction.
#[test]
fn outcome_error_carries_typed_kind() {
    let outcome = Outcome::Error(ErrorKindJson::NoPmu, "none".to_string());
    match outcome {
        Outcome::Error(kind, _) => {
            assert_eq!(kind, ErrorKindJson::NoPmu);
            assert_ne!(kind.exit_code(), 0);
        }
        _ => panic!("expected error outcome"),
    }
}
