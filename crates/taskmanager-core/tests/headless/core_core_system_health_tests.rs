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

#[test]
fn health_score_is_bounded_and_keeps_an_explicit_deduction_bill() {
    let score = SystemHealthScore::calculate(HealthScoreInput {
        cpu_some_avg10: Some(55.0),
        memory_some_avg10: Some(0.0),
        memory_full_avg10: Some(8.0),
        io_some_avg10: Some(0.0),
        swap_used_pct: Some(92.0),
        thermal_throttled: Some(true),
    })
    .expect("observed health inputs produce a score");
    assert!(score.score < 100);
    assert!(score.score <= 100);
    assert!(score.is_degraded());
    assert!(score.deductions.iter().any(|item| {
        item.kind == HealthDeductionKind::MemoryFullStall
            && item.evidence.contains("pressure avg10 8.0%")
    }));
    assert!(
        score
            .deductions
            .iter()
            .any(|item| item.kind == HealthDeductionKind::ThermalThrottle)
    );
}

#[test]
fn health_score_does_not_turn_an_unobserved_snapshot_into_healthy() {
    assert_eq!(
        SystemHealthScore::calculate(HealthScoreInput::default()),
        None
    );
    let score = SystemHealthScore::calculate(HealthScoreInput {
        thermal_throttled: Some(false),
        ..HealthScoreInput::default()
    })
    .expect("a confirmed unthrottled observation is still an observation");
    assert_eq!(score.score, 100);
    assert!(score.deductions.is_empty());
}
