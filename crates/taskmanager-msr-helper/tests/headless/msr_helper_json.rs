use super::*;
use serde_json::Value;
use std::collections::BTreeSet;

fn success_value(packages: Vec<PackageReadingJson>) -> Value {
    serde_json::to_value(SuccessEnvelope {
        schema: SCHEMA_VERSION,
        packages,
    })
    .expect("success envelope serializes")
}

fn error_value(kind: ErrorKindJson, detail: &str) -> Value {
    serde_json::to_value(ErrorEnvelope {
        status: "error",
        kind,
        detail: detail.to_owned(),
    })
    .expect("error envelope serializes")
}

fn keys(value: &Value) -> BTreeSet<String> {
    value.as_object().expect("object").keys().cloned().collect()
}

#[test]
fn success_envelope_carries_packages_and_never_status() {
    let value = success_value(Vec::new());
    assert_eq!(
        keys(&value),
        ["packages", "schema"]
            .map(String::from)
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "SUCCESS keys exactly match the contract",
    );
    assert_eq!(value["schema"], 1);
    assert_eq!(
        value["packages"].as_array().map(Vec::len),
        Some(0),
        "an empty node list is an honest empty array",
    );
}

#[test]
fn success_row_serializes_nulls_for_absent_readouts_never_zeros() {
    let reading = PackageReadingJson {
        cpu: 7,
        bclk_mhz: None,
        temperature_c: Some(58.0),
        multiplier: Some(45.0),
        multiplier_min: None,
        multiplier_max: Some(55.0),
        vcore_v: None,
    };
    let value = success_value(vec![reading]);
    let row = &value["packages"][0];
    assert_eq!(row["cpu"], 7);
    assert_eq!(row["temperature_c"], 58.0);
    assert_eq!(row["multiplier"], 45.0);
    assert_eq!(row["multiplier_max"], 55.0);
    for absent in ["bclk_mhz", "multiplier_min", "vcore_v"] {
        assert!(
            row[absent].is_null(),
            "{absent} must serialize as null, never a fabricated zero",
        );
    }
}

#[test]
fn error_envelope_carries_status_and_never_packages() {
    for kind in [
        ErrorKindJson::PermissionDenied,
        ErrorKindJson::NoMsr,
        ErrorKindJson::OpenFailed,
        ErrorKindJson::ReadFailed,
    ] {
        let value = error_value(kind, "diagnostic");
        assert_eq!(value["status"], "error");
        assert_eq!(value["detail"], "diagnostic");
        assert!(
            value.get("packages").is_none(),
            "an ERROR object never carries packages",
        );
    }
}

#[test]
fn error_kinds_serialize_to_the_contract_keywords() {
    for (kind, keyword) in [
        (ErrorKindJson::PermissionDenied, "permission_denied"),
        (ErrorKindJson::NoMsr, "no_msr"),
        (ErrorKindJson::OpenFailed, "open_failed"),
        (ErrorKindJson::ReadFailed, "read_failed"),
    ] {
        assert_eq!(serde_json::to_value(kind).expect("serializes"), keyword);
    }
}

#[test]
fn exit_codes_are_distinct_nonzero_and_ordered_by_kind() {
    let codes: [i32; 4] = [
        ErrorKindJson::PermissionDenied.exit_code(),
        ErrorKindJson::NoMsr.exit_code(),
        ErrorKindJson::OpenFailed.exit_code(),
        ErrorKindJson::ReadFailed.exit_code(),
    ];
    assert_eq!(codes, [2, 3, 4, 5]);
}
