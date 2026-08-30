//! Tests for the `--memory-smbios` CLI mode's JSON rendering of the polkit
//! memory-helper outcome: success inventories, contract error round-trips, and
//! the no-fabricated-rows honesty rule.

use super::*;
use serde_json::Value;
use taskmanager_escalation::polkit::{
    DmiIdentityFacts, SmbiosHelperError, SmbiosHelperErrorKind, SmbiosMemorySuccess,
    SmbiosModuleReading,
};

fn all_contract_error_kinds() -> [SmbiosHelperErrorKind; 4] {
    [
        SmbiosHelperErrorKind::PermissionDenied,
        SmbiosHelperErrorKind::NoDmi,
        SmbiosHelperErrorKind::OpenFailed,
        SmbiosHelperErrorKind::ReadFailed,
    ]
}

fn populated_reading() -> SmbiosModuleReading {
    SmbiosModuleReading {
        slot: 0,
        size_mb: Some(16_384),
        speed_mts: Some(5_600),
        configured_speed_mts: Some(5_200),
        manufacturer: Some("Samsung".to_owned()),
        serial_number: Some("37A31B2C".to_owned()),
        part_number: Some("M425R4GA3PB0".to_owned()),
        form_factor: Some("SODIMM".to_owned()),
        memory_type: Some("DDR5".to_owned()),
        locator: Some("ChannelA-DIMM0".to_owned()),
    }
}

/// A fully stated identity payload (the root-only /sys/class/dmi/id facts).
fn stated_identity() -> DmiIdentityFacts {
    DmiIdentityFacts {
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

#[test]
fn render_success_lists_modules() {
    let outcome = SmbiosHelperOutcome::Success(Box::new(SmbiosMemorySuccess {
        schema: 1,
        slots_total: 4,
        slots_used: 1,
        modules: vec![populated_reading()],
        identity: Some(stated_identity()),
    }));
    let value: Value = serde_json::from_str(&render_outcome(&outcome))
        .expect("rendered success document must parse");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["slots_total"], 4);
    assert_eq!(value["slots_used"], 1);
    assert_eq!(value["modules"][0]["slot"], 0);
    assert_eq!(value["modules"][0]["size_mb"], 16_384);
    assert_eq!(value["modules"][0]["configured_speed_mts"], 5_200);
    assert_eq!(value["modules"][0]["manufacturer"], "Samsung");
    assert_eq!(value["modules"][0]["memory_type"], "DDR5");
    assert_eq!(value["identity"]["system_serial"], "PF3XYZ42");
    assert_eq!(
        value["identity"]["system_uuid"],
        "4c4c4544-0042-3510-8054-b7c04f4d3532"
    );
    assert_eq!(value["identity"]["board_asset_tag"], "ASSET-42");
}

#[test]
fn render_success_identity_object_is_null_or_per_fact_null() {
    // No 0/1/2 entries on the host: the whole object is an honest null.
    let outcome = SmbiosHelperOutcome::Success(Box::new(SmbiosMemorySuccess {
        schema: 1,
        slots_total: 0,
        slots_used: 0,
        modules: Vec::new(),
        identity: None,
    }));
    let value: Value = serde_json::from_str(&render_outcome(&outcome))
        .expect("rendered success document must parse");
    assert!(value["identity"].is_null());

    // An object whose records stated almost nothing: every absent fact is
    // null, never a fabricated value.
    let mut identity = stated_identity();
    identity.system_serial = None;
    identity.system_uuid = None;
    let outcome = SmbiosHelperOutcome::Success(Box::new(SmbiosMemorySuccess {
        schema: 1,
        slots_total: 1,
        slots_used: 0,
        modules: Vec::new(),
        identity: Some(identity),
    }));
    let value: Value = serde_json::from_str(&render_outcome(&outcome))
        .expect("rendered success document must parse");
    assert!(value["identity"]["system_serial"].is_null());
    assert!(value["identity"]["system_uuid"].is_null());
    assert_eq!(value["identity"]["system_sku"], "SKU-AB");
}

#[test]
fn render_success_keeps_absent_facts_as_json_null() {
    let mut reading = populated_reading();
    reading.configured_speed_mts = None;
    reading.serial_number = None;
    let outcome = SmbiosHelperOutcome::Success(Box::new(SmbiosMemorySuccess {
        schema: 1,
        slots_total: 1,
        slots_used: 1,
        modules: vec![reading],
        identity: None,
    }));
    let value: Value = serde_json::from_str(&render_outcome(&outcome))
        .expect("rendered success document must parse");
    assert_eq!(value["status"], "ok");
    assert!(value["modules"][0]["configured_speed_mts"].is_null());
    assert!(value["modules"][0]["serial_number"].is_null());
    assert_eq!(value["modules"][0]["speed_mts"], 5_600);
}

#[test]
fn render_success_with_no_modules_is_honest_empty_array() {
    let outcome = SmbiosHelperOutcome::Success(Box::new(SmbiosMemorySuccess {
        schema: 1,
        slots_total: 2,
        slots_used: 0,
        modules: Vec::new(),
        identity: None,
    }));
    let value: Value =
        serde_json::from_str(&render_outcome(&outcome)).expect("rendered document must parse");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["modules"].as_array().map(Vec::len), Some(0));
}

#[test]
fn render_helper_error_each_kind_round_trips_contract_string() {
    for kind in all_contract_error_kinds() {
        let outcome = SmbiosHelperOutcome::HelperError(SmbiosHelperError {
            kind,
            detail: "detail text".to_owned(),
        });
        let value: Value = serde_json::from_str(&render_outcome(&outcome))
            .expect("rendered error document must parse");
        assert_eq!(value["status"], "error");
        assert_eq!(value["kind"], kind.as_contract_str());
        assert_eq!(value["detail"], "detail text");
    }
}

#[test]
fn render_unavailable_carries_typed_reason_and_feature() {
    for (reason, expected) in [
        (EscalationDenialReason::Unsupported, "unsupported"),
        (
            EscalationDenialReason::PermissionDenied,
            "permission_denied",
        ),
        (
            EscalationDenialReason::AuthorizationUnavailable,
            "authorization_unavailable",
        ),
        (
            EscalationDenialReason::HelperUnavailable,
            "helper_unavailable",
        ),
        (
            EscalationDenialReason::HelperProtocolViolation,
            "helper_protocol_violation",
        ),
    ] {
        let outcome = SmbiosHelperOutcome::Unavailable {
            reason,
            detail: "why".to_owned(),
        };
        let value: Value = serde_json::from_str(&render_outcome(&outcome))
            .expect("rendered unavailable document must parse");
        assert_eq!(value["status"], "unavailable");
        assert_eq!(value["reason"], expected);
        assert_eq!(value["feature"], "memory_smbios");
        assert_eq!(value["detail"], "why");
    }
}

#[test]
fn render_never_emits_a_fabricated_module_row() {
    // Every non-success outcome MUST omit "modules" entirely; a fabricated
    // zero-valued row here would violate the honesty red line.
    let denied = SmbiosHelperOutcome::Unavailable {
        reason: EscalationDenialReason::PermissionDenied,
        detail: "declined".to_owned(),
    };
    let value: Value =
        serde_json::from_str(&render_outcome(&denied)).expect("rendered document must parse");
    assert!(
        value.get("modules").is_none(),
        "a denial must never carry a fabricated modules array",
    );

    let helper_err = SmbiosHelperOutcome::HelperError(SmbiosHelperError {
        kind: SmbiosHelperErrorKind::NoDmi,
        detail: "none".to_owned(),
    });
    let value: Value =
        serde_json::from_str(&render_outcome(&helper_err)).expect("rendered document must parse");
    assert!(
        value.get("modules").is_none(),
        "a helper error must never carry a fabricated modules array",
    );
}
