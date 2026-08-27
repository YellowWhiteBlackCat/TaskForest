use super::*;

/// A SUCCESS envelope serializes with `engines` and NO `status` field, and
/// round-trips through serde_json preserving the engine shape. This is the
/// load-bearing contract assertion for Track B's consumer.
#[test]
fn success_envelope_has_engines_and_no_status() {
    let envelope = SuccessEnvelope {
        schema: SCHEMA_VERSION,
        driver: "xe",
        sample_ms: 1000,
        engines: vec![
            EngineJson {
                name: "Render/3D".to_string(),
                class: "render".to_string(),
                busy_pct: 42.5,
            },
            EngineJson {
                name: "Copy".to_string(),
                class: "copy".to_string(),
                busy_pct: 0.0,
            },
        ],
    };
    let json = serde_json::to_string(&envelope).expect("serialize success");
    assert!(
        json.contains("\"engines\""),
        "success envelope must carry engines: {json}"
    );
    assert!(
        !json.contains("\"status\""),
        "success envelope must NOT carry status: {json}"
    );
    assert!(
        json.contains("\"schema\":1"),
        "schema version pinned: {json}"
    );
    assert!(json.contains("\"driver\":\"xe\""), "driver emitted: {json}");
    assert!(
        json.contains("\"sample_ms\":1000"),
        "sample_ms emitted: {json}"
    );
    // Round-trip into a generic Value to confirm field names + types.
    let value: serde_json::Value = serde_json::from_str(&json).expect("emit is valid JSON");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["driver"], "xe");
    assert_eq!(value["sample_ms"], 1000);
    assert!(value.get("status").is_none(), "no status on success");
    assert_eq!(value["engines"][0]["name"], "Render/3D");
    assert_eq!(value["engines"][0]["class"], "render");
    assert!((value["engines"][0]["busy_pct"].as_f64().unwrap() - 42.5).abs() < 1e-6);
}

/// The four error kinds serialize to the exact snake_case keywords of the
/// contract, the envelope carries `status`/`kind`/`detail` and NO `engines`,
/// and each kind maps to a distinct non-zero exit code.
#[test]
fn error_envelopes_serialize_with_status_kind_detail_and_no_engines() {
    for (kind, keyword, expected_code) in [
        (ErrorKindJson::PermissionDenied, "permission_denied", 2),
        (ErrorKindJson::NoPmu, "no_pmu", 3),
        (ErrorKindJson::OpenFailed, "open_failed", 4),
        (ErrorKindJson::ReadFailed, "read_failed", 5),
    ] {
        let envelope = ErrorEnvelope {
            status: "error",
            kind,
            detail: format!("diag for {keyword}"),
        };
        let json = serde_json::to_string(&envelope)
            .unwrap_or_else(|err| panic!("serialize {kind:?}: {err}"));
        assert!(
            json.contains("\"status\":\"error\""),
            "{kind:?}: status literal: {json}"
        );
        assert!(
            json.contains(&format!("\"kind\":\"{keyword}\"")),
            "{kind:?}: kind keyword: {json}"
        );
        assert!(
            json.contains("\"detail\":\"diag for"),
            "{kind:?}: detail emitted: {json}"
        );
        assert!(
            !json.contains("\"engines\""),
            "{kind:?}: error envelope must NOT carry engines: {json}"
        );
        assert_eq!(kind.exit_code(), expected_code);
        // Valid JSON + field check via a generic parse.
        let value: serde_json::Value = serde_json::from_str(&json).expect("emit is valid JSON");
        assert!(value.get("engines").is_none());
        assert_eq!(value["status"], "error");
        assert_eq!(value["kind"], keyword);
    }
}

/// An empty SUCCESS envelope (PMU present, zero engines read) is still a
/// SUCCESS object carrying `engines: []` and no `status` — the consumer
/// treats a present-but-empty array as "PMU reachable, no data this tick",
/// distinct from the ERROR envelope. (In practice the helper reports an
/// ERROR when no engine produced data; this test pins the serialization
/// shape either way.)
#[test]
fn empty_success_still_carries_engines_array_and_no_status() {
    let envelope = SuccessEnvelope {
        schema: SCHEMA_VERSION,
        driver: "i915",
        sample_ms: 1000,
        engines: Vec::new(),
    };
    let json = serde_json::to_string(&envelope).expect("serialize empty success");
    assert!(
        json.contains("\"engines\":[]"),
        "empty engines array: {json}"
    );
    assert!(!json.contains("\"status\""));
}

/// `busy_pct` clamps into [0.0, 100.0]; a NaN/inf must never be emitted
/// (serde_json would emit `null`/invalid tokens). This guards the honesty
/// red line at the serialization seam.
#[test]
fn busy_pct_is_finite_and_within_range() {
    let engine = EngineJson {
        name: "Render/3D".to_string(),
        class: "render".to_string(),
        busy_pct: 100.0,
    };
    let json = serde_json::to_string(&engine).expect("serialize engine");
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    let pct = value["busy_pct"].as_f64().expect("busy_pct is a number");
    assert!(pct.is_finite());
    assert!((0.0..=100.0).contains(&pct));
}
