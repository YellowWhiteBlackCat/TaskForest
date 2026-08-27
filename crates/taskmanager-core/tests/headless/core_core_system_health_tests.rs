use super::*;

#[test]
fn self_test_provider_target_keeps_identity_generation_and_locator_together() {
    let intent = SmartSelfTestIntent {
        device_id: DeviceId::new("disk:wwid:fixture"),
        device_generation: DeviceGeneration::new(7),
        device_key: StorageDeviceKey::new("native-locator"),
        display_name: "Fixture disk".into(),
        kind: SmartSelfTestKind::Extended,
    };

    let target = intent.target();
    assert_eq!(target.device_id, intent.device_id);
    assert_eq!(target.device_generation, intent.device_generation);
    assert_eq!(target.locator, intent.device_key);

    let observation = intent.into_observation(SmartSelfTestReport::default());
    assert_eq!(observation.target(), target);
}

#[test]
fn self_test_intent_retains_legacy_flat_wire_shape() {
    let intent: SmartSelfTestIntent = serde_json::from_value(serde_json::json!({
        "device_id": "disk:wwid:fixture",
        "device_generation": 3,
        "device_key": "legacy-locator",
        "display_name": "Fixture",
        "kind": "short"
    }))
    .expect("decode legacy self-test intent");
    assert_eq!(intent.target().locator.as_str(), "legacy-locator");

    let encoded = serde_json::to_value(intent).expect("encode intent");
    assert_eq!(encoded["device_key"], "legacy-locator");
    assert!(encoded.get("target").is_none());
}
