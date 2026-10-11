//! Versioned alert-event export contract tests.

use super::*;
use crate::core::alerts::{
    Alert, AlertEventId, AlertEventKind, AlertInstanceId, AlertMetric, AlertSeverity,
    AlertTargetIdentity,
};

fn alert(rule_id: &str, target: AlertTargetIdentity) -> Alert {
    Alert {
        instance_id: AlertInstanceId::new(rule_id.into(), target),
        rule_id: rule_id.into(),
        target: "System Disk".into(),
        metric: AlertMetric::SmartPercentUsed,
        severity: AlertSeverity::Warning,
        value: 95.0,
        threshold: 90.0,
        active_since_ms: 10,
    }
}

fn event(id: u64, kind: AlertEventKind, rule_id: &str) -> AlertEvent {
    AlertEvent {
        id: AlertEventId::new(id).expect("fixture event IDs are non-zero"),
        kind,
        alert: alert(
            rule_id,
            AlertTargetIdentity::Storage {
                device_id: "disk:one".into(),
                generation: crate::DeviceGeneration::new(4),
            },
        ),
        observed_at_ms: id * 10,
    }
}

#[test]
fn export_is_versioned_and_contains_structured_transition_identity() {
    let json = export_alert_events_json(&[
        event(1, AlertEventKind::Activated, "rule:with:delimiters"),
        event(2, AlertEventKind::Cleared, "rule:with:delimiters"),
    ])
    .expect("ordered events export");
    let document: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(document["schema"], ALERT_EVENT_FILE_SCHEMA);
    assert_eq!(document["version"], 2);
    assert_eq!(document["events"][0]["id"], 1);
    assert_eq!(
        document["events"][0]["instance_id"]["rule_id"],
        "rule:with:delimiters"
    );
    assert_eq!(
        document["events"][0]["instance_id"]["target"]["kind"],
        "storage"
    );
    assert_eq!(
        document["events"][0]["instance_id"]["target"]["device_id"],
        "disk:one"
    );
    assert_eq!(
        document["events"][0]["instance_id"]["target"]["generation"],
        4
    );
    assert_eq!(document["events"][0]["rule_id"], "rule:with:delimiters");
    assert_eq!(document["events"][0]["kind"], "activated");
    assert_eq!(document["events"][1]["kind"], "cleared");
}

#[test]
fn export_rejects_nonmonotonic_append_ids_and_deserialization_rejects_zero() {
    assert_eq!(
        export_alert_events_json(&[
            event(2, AlertEventKind::Activated, "a"),
            event(1, AlertEventKind::Cleared, "a"),
        ]),
        Err(AlertEventExportError::NonMonotonicIds)
    );
    assert!(serde_json::from_str::<AlertEventId>("0").is_err());
}

#[test]
fn export_rejects_histories_larger_than_the_domain_bound() {
    let events: Vec<_> = (1..=MAX_ALERT_EVENTS as u64 + 1)
        .map(|id| event(id, AlertEventKind::Activated, "a"))
        .collect();
    assert_eq!(
        export_alert_events_json(&events),
        Err(AlertEventExportError::TooManyEvents(MAX_ALERT_EVENTS + 1))
    );
}
