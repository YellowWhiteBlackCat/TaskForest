use super::*;
use serde_json::Value;
use std::collections::BTreeSet;

fn success_value(
    modules: Vec<MemoryModuleJson>,
    slots_total: u32,
    slots_used: u32,
    identity: Option<DmiIdentityJson>,
) -> Value {
    serde_json::to_value(SuccessEnvelope {
        schema: SCHEMA_VERSION,
        slots_total,
        slots_used,
        modules,
        identity,
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
fn success_envelope_carries_modules_and_never_status() {
    let value = success_value(vec![], 2, 1, None);
    assert_eq!(
        keys(&value),
        ["identity", "modules", "schema", "slots_total", "slots_used"]
            .map(String::from)
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "SUCCESS keys exactly match the contract"
    );
    assert_eq!(value["schema"], 1);
    assert_eq!(value["slots_total"], 2);
    assert_eq!(value["slots_used"], 1);
    assert!(value["modules"].is_array());
    assert!(value["identity"].is_null(), "no 0/1/2 entries is null");
}

#[test]
fn error_envelope_carries_status_and_never_modules() {
    for kind in [
        ErrorKindJson::PermissionDenied,
        ErrorKindJson::NoDmi,
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
        (ErrorKindJson::NoDmi, "no_dmi"),
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
        ErrorKindJson::NoDmi,
        ErrorKindJson::OpenFailed,
        ErrorKindJson::ReadFailed,
    ]
    .map(|kind| kind.exit_code())
    .into_iter()
    .collect();
    assert_eq!(codes, [2, 3, 4, 5].into_iter().collect::<BTreeSet<_>>());
}

/// A module whose record stated nothing optional: every such field must be
/// JSON `null` (the consumer's absent marker), never a fabricated zero or
/// empty string.
#[test]
fn unstated_module_fields_serialize_as_null() {
    let module = MemoryModuleJson {
        slot: 1,
        size_mb: Some(8192),
        speed_mts: None,
        configured_speed_mts: None,
        manufacturer: None,
        serial_number: None,
        part_number: None,
        form_factor: None,
        memory_type: None,
        locator: None,
    };
    let value = success_value(vec![module], 1, 1, None);
    let module = &value["modules"][0];
    assert_eq!(module["slot"], 1);
    assert_eq!(module["size_mb"], 8192);
    for field in [
        "speed_mts",
        "configured_speed_mts",
        "manufacturer",
        "serial_number",
        "part_number",
        "form_factor",
        "memory_type",
        "locator",
    ] {
        assert!(module[field].is_null(), "{field} must serialize as null");
    }
}

#[test]
fn stated_module_fields_round_trip_typed() {
    let module = MemoryModuleJson {
        slot: 0,
        size_mb: Some(16384),
        speed_mts: Some(5600),
        configured_speed_mts: Some(4800),
        manufacturer: Some("Crucial".to_owned()),
        serial_number: Some("SER1".to_owned()),
        part_number: Some("CT8G4".to_owned()),
        form_factor: Some("SODIMM"),
        memory_type: Some("DDR4"),
        locator: Some("ChannelA-DIMM0".to_owned()),
    };
    let value = success_value(vec![module], 1, 1, None);
    let module = &value["modules"][0];
    assert_eq!(module["size_mb"], 16384);
    assert_eq!(module["speed_mts"], 5600);
    assert_eq!(module["configured_speed_mts"], 4800);
    assert_eq!(module["manufacturer"], "Crucial");
    assert_eq!(module["serial_number"], "SER1");
    assert_eq!(module["part_number"], "CT8G4");
    assert_eq!(module["form_factor"], "SODIMM");
    assert_eq!(module["memory_type"], "DDR4");
    assert_eq!(module["locator"], "ChannelA-DIMM0");
}

/// Every contract identity field, stated by the source records.
fn stated_identity() -> DmiIdentityJson {
    DmiIdentityJson {
        bios_vendor: Some("AMI".to_owned()),
        bios_version: Some("P1.27".to_owned()),
        bios_date: Some("04/17/2024".to_owned()),
        board_manufacturer: Some("ASUSTeK".to_owned()),
        board_product: Some("X670E".to_owned()),
        board_serial: Some("MB-SN-1".to_owned()),
        board_asset_tag: Some("ASSET-42".to_owned()),
        system_manufacturer: Some("LENOVO".to_owned()),
        system_product: Some("21JX".to_owned()),
        system_serial: Some("PF3XYZ42".to_owned()),
        system_uuid: Some("4c4c4544-0042-3510-8054-b7c04f4d3532".to_owned()),
        system_sku: Some("SKU-AB".to_owned()),
        system_family: Some("ThinkPad".to_owned()),
    }
}

/// The contract's exact identity field names, in the shared vocabulary.
const IDENTITY_FIELDS: [&str; 13] = [
    "bios_vendor",
    "bios_version",
    "bios_date",
    "board_manufacturer",
    "board_product",
    "board_serial",
    "board_asset_tag",
    "system_manufacturer",
    "system_product",
    "system_serial",
    "system_uuid",
    "system_sku",
    "system_family",
];

#[test]
fn stated_identity_fields_round_trip_typed() {
    let value = success_value(vec![], 0, 0, Some(stated_identity()));
    let identity = &value["identity"];
    assert_eq!(identity["bios_vendor"], "AMI");
    assert_eq!(identity["bios_date"], "04/17/2024");
    assert_eq!(identity["board_asset_tag"], "ASSET-42");
    assert_eq!(identity["system_serial"], "PF3XYZ42");
    assert_eq!(
        identity["system_uuid"],
        "4c4c4544-0042-3510-8054-b7c04f4d3532"
    );
    assert_eq!(identity["system_family"], "ThinkPad");
}

#[test]
fn unstated_identity_fields_serialize_as_null() {
    let value = success_value(vec![], 0, 0, Some(DmiIdentityJson::default()));
    let identity = &value["identity"];
    for field in IDENTITY_FIELDS {
        assert!(identity[field].is_null(), "{field} must serialize as null");
    }
}

#[test]
fn identity_object_key_set_is_exactly_the_contract_vocabulary() {
    let value = success_value(vec![], 0, 0, Some(DmiIdentityJson::default()));
    let identity = value["identity"].as_object().expect("identity object");
    assert_eq!(
        identity.keys().cloned().collect::<BTreeSet<_>>(),
        IDENTITY_FIELDS.map(String::from).into_iter().collect(),
        "the identity object carries exactly the 13 contract fields"
    );
}
