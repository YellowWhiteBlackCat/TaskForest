//! Versioned alert-event export contract tests.

use super::*;
use crate::{Alert, AlertMetric, AlertSeverity};

fn alert(id: &str) -> Alert {
    Alert {
        instance_id: id.to_owned(),
        rule_id: "cpu-high".to_owned(),
        target: "system".to_owned(),
        metric: AlertMetric::CpuUsagePercent,
        severity: AlertSeverity::Warning,
        value: 95.0,
        threshold: 90.0,
        active_since_ms: 10,
    }
}

fn event(id: u64, kind: AlertEventKind) -> AlertEvent {
    AlertEvent {
        id,
        kind,
        alert: alert(&format!("cpu-high:{id}")),
        observed_at_ms: id * 10,
    }
}

#[test]
fn export_is_versioned_and_contains_transition_facts() {
    let json = export_alert_events_json(&[
        event(1, AlertEventKind::Activated),
        event(2, AlertEventKind::Cleared),
    ])
    .expect("ordered events export");
    assert!(json.contains(ALERT_EVENT_FILE_SCHEMA));
    assert!(json.contains("\"version\": 1"));
    assert!(json.contains("\"activated\""));
    assert!(json.contains("\"cleared\""));
    assert!(json.contains("cpu-high:1"));
}

#[test]
fn export_rejects_invalid_event_order_and_ids() {
    assert_eq!(
        export_alert_events_json(&[
            event(2, AlertEventKind::Activated),
            event(1, AlertEventKind::Cleared),
        ]),
        Err(AlertEventExportError::NonMonotonicIds)
    );
    assert_eq!(
        export_alert_events_json(&[event(0, AlertEventKind::Activated)]),
        Err(AlertEventExportError::ZeroEventId)
    );
}

#[test]
fn export_rejects_histories_larger_than_the_domain_bound() {
    let events: Vec<_> = (1..=MAX_ALERT_EVENTS as u64 + 1)
        .map(|id| event(id, AlertEventKind::Activated))
        .collect();
    assert_eq!(
        export_alert_events_json(&events),
        Err(AlertEventExportError::TooManyEvents(MAX_ALERT_EVENTS + 1))
    );
}
