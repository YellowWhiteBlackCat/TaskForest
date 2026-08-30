use super::*;
use serde_json::Value;
use std::collections::BTreeSet;

fn success_value(packages: Vec<PackageJson>) -> Value {
    serde_json::to_value(SuccessEnvelope {
        schema: SCHEMA_VERSION,
        sample_ms: 1000,
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
    let value = success_value(vec![]);
    assert_eq!(
        keys(&value),
        ["packages", "sample_ms", "schema"]
            .map(String::from)
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "SUCCESS keys exactly match the contract"
    );
    assert_eq!(value["schema"], 1);
    assert_eq!(value["sample_ms"], 1000);
    assert!(value["packages"].is_array());
}

#[test]
fn error_envelope_carries_status_and_never_packages() {
    for kind in [
        ErrorKindJson::PermissionDenied,
        ErrorKindJson::NoRapl,
        ErrorKindJson::OpenFailed,
        ErrorKindJson::ReadFailed,
    ] {
        let value = error_value(kind, "diagnostic");
        assert_eq!(
            keys(&value),
            ["detail", "kind", "status"]
                .map(String::from)
                .into_iter()
                .collect::<BTreeSet<_>>(),
            "ERROR keys exactly match the contract for {kind:?}"
        );
        assert_eq!(
            value["status"], "error",
            "status is the literal error marker"
        );
        assert_eq!(value["detail"], "diagnostic");
    }
}

#[test]
fn kinds_serialize_to_the_contract_snake_case_keywords() {
    let cases = [
        (ErrorKindJson::PermissionDenied, "permission_denied"),
        (ErrorKindJson::NoRapl, "no_rapl"),
        (ErrorKindJson::OpenFailed, "open_failed"),
        (ErrorKindJson::ReadFailed, "read_failed"),
    ];
    for (kind, keyword) in cases {
        assert_eq!(error_value(kind, "d")["kind"], keyword);
    }
}

#[test]
fn exit_codes_are_distinct_and_nonzero() {
    let codes: BTreeSet<i32> = [
        ErrorKindJson::PermissionDenied,
        ErrorKindJson::NoRapl,
        ErrorKindJson::OpenFailed,
        ErrorKindJson::ReadFailed,
    ]
    .map(|kind| kind.exit_code())
    .into_iter()
    .collect();
    assert_eq!(codes, [2, 3, 4, 5].into_iter().collect::<BTreeSet<_>>());
}

#[test]
fn package_fields_serialize_typed() {
    let package = PackageJson {
        name: "Core".to_owned(),
        power_w: 1.5,
        energy_delta_uj: 1_500_000,
    };
    let value = success_value(vec![package]);
    let package = &value["packages"][0];
    assert_eq!(package["name"], "Core");
    assert_eq!(package["power_w"], 1.5);
    assert_eq!(package["energy_delta_uj"], 1_500_000);
}
