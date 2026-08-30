use super::*;

fn sample_modules() -> Vec<SmbiosModuleRow> {
    vec![SmbiosModuleRow {
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
    }]
}

#[test]
fn success_carries_inventory_and_never_a_failure_tag() {
    let identity = sample_identity();
    let snapshot = SmbiosMemorySnapshot::success(4, 1, sample_modules(), Some(identity));
    assert!(snapshot.is_success());
    assert_eq!(snapshot.slots_total, 4);
    assert_eq!(snapshot.slots_used, 1);
    assert_eq!(snapshot.modules.len(), 1);
    assert_eq!(snapshot.failure, None);
    let identity = snapshot.identity.as_ref().expect("identity carried");
    assert_eq!(identity.system_serial.as_deref(), Some("PF3XYZ42"));
    assert_eq!(
        identity.system_uuid.as_deref(),
        Some("4c4c4544-0042-3510-8054-b7c04f4d3532")
    );
    assert_eq!(identity.board_asset_tag.as_deref(), Some("ASSET-42"));
}

/// A fully stated identity payload (system serial/UUID/asset tag/SKU are the
/// facts the unprivileged path cannot read).
fn sample_identity() -> DmiIdentityFacts {
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
fn success_without_identity_tables_keeps_the_fact_absent() {
    let snapshot = SmbiosMemorySnapshot::success(2, 0, Vec::new(), None);
    assert!(snapshot.is_success());
    assert_eq!(snapshot.identity, None);
}

#[test]
fn failed_never_carries_a_fabricated_row() {
    let snapshot =
        SmbiosMemorySnapshot::failed(FailureKind::RequiresEscalation, "helper not installed");
    assert!(!snapshot.is_success());
    assert_eq!(snapshot.slots_total, 0);
    assert!(snapshot.modules.is_empty());
    let failure = snapshot
        .failure
        .as_ref()
        .expect("failure tag must be present");
    assert_eq!(failure.kind, FailureKind::RequiresEscalation);
    assert_eq!(failure.detail, "helper not installed");
}

#[test]
fn absent_module_facts_stay_none_never_zero() {
    let row = SmbiosModuleRow {
        slot: 2,
        size_mb: Some(16_384),
        ..SmbiosModuleRow::default()
    };
    let snapshot = SmbiosMemorySnapshot::success(2, 1, vec![row], None);
    let row = &snapshot.modules[0];
    assert_eq!(row.speed_mts, None);
    assert_eq!(row.configured_speed_mts, None);
    assert_eq!(row.manufacturer, None);
    assert_eq!(row.locator, None);
}

#[test]
fn snapshots_round_trip_through_serde() {
    let snapshot = SmbiosMemorySnapshot::success(2, 1, sample_modules(), Some(sample_identity()));
    let json = serde_json::to_string(&snapshot).expect("snapshot is serializable");
    let decoded: SmbiosMemorySnapshot = serde_json::from_str(&json).expect("snapshot round-trips");
    assert_eq!(decoded, snapshot);

    let failed = SmbiosMemorySnapshot::failed(FailureKind::Unsupported, "off-host");
    let json = serde_json::to_string(&failed).expect("failed snapshot is serializable");
    let decoded: SmbiosMemorySnapshot =
        serde_json::from_str(&json).expect("failed snapshot round-trips");
    assert_eq!(decoded, failed);
}

#[test]
fn identity_field_defaults_when_decoding_a_pre_identity_snapshot() {
    // A snapshot serialized before the additive field existed carries no
    // `identity` key; decoding must yield an honest None, not an error.
    let legacy = r#"{"slots_total":2,"slots_used":0,"modules":[],"failure":null}"#;
    let decoded: SmbiosMemorySnapshot =
        serde_json::from_str(legacy).expect("pre-identity snapshot decodes");
    assert!(decoded.is_success());
    assert_eq!(decoded.identity, None);
}
