use super::*;
use taskmanager_core::alerts::{AlertMetric, AlertSeverity};

fn alert(instance: &str, severity: AlertSeverity) -> Alert {
    Alert {
        instance_id: instance.to_owned(),
        rule_id: "r".to_owned(),
        target: "t".to_owned(),
        metric: AlertMetric::CpuUsagePercent,
        severity,
        value: 90.0,
        threshold: 80.0,
        active_since_ms: 0,
    }
}

#[test]
fn disabled_policy_never_dispatching() {
    let mut dispatcher = AlertDispatcher::default();
    let requests = dispatcher.dispatch_new(&[], &[alert("a", AlertSeverity::Critical)], 1_000);
    assert!(requests.is_empty(), "opt-out must suppress delivery");
}

#[test]
fn only_new_active_instances_dispatch() {
    let mut dispatcher = AlertDispatcher::new(NotificationPolicy {
        enabled: true,
        cooldown_ms: 0,
        ..NotificationPolicy::default()
    });
    let a = alert("a", AlertSeverity::Warning);
    let b = alert("b", AlertSeverity::Critical);
    let first = dispatcher.dispatch_new(&[], &[a.clone(), b.clone()], 1_000);
    assert_eq!(first.len(), 2);
    // Second evaluation with the SAME active set: nothing new fires.
    let repeat = dispatcher.dispatch_new(&[a.clone(), b.clone()], &[a, b], 1_050);
    assert!(repeat.is_empty(), "staying active must stay silent");
}

#[test]
fn cleared_alert_can_dispatch_again() {
    let mut dispatcher = AlertDispatcher::new(NotificationPolicy {
        enabled: true,
        cooldown_ms: 0,
        ..NotificationPolicy::default()
    });
    let a = alert("a", AlertSeverity::Info);
    assert_eq!(
        dispatcher
            .dispatch_new(&[], std::slice::from_ref(&a), 1_000)
            .len(),
        1
    );
    // Alert cleared (empty next) then re-fired: the transition back INTO
    // the active set is a new event and must dispatch again.
    assert!(
        dispatcher
            .dispatch_new(std::slice::from_ref(&a), &[], 1_100)
            .is_empty()
    );
    assert_eq!(dispatcher.dispatch_new(&[], &[a], 1_200).len(), 1);
}

#[test]
fn cooldown_gates_per_instance_inside_the_dispatcher() {
    let mut dispatcher = AlertDispatcher::new(NotificationPolicy {
        enabled: true,
        cooldown_ms: 10_000,
        ..NotificationPolicy::default()
    });
    let a = alert("a", AlertSeverity::Warning);
    assert_eq!(
        dispatcher
            .dispatch_new(&[], std::slice::from_ref(&a), 1_000)
            .len(),
        1
    );
    // Same instance clears and re-fires inside the cooldown: suppressed.
    let suppressed = dispatcher
        .dispatch_new(std::slice::from_ref(&a), &[], 2_000)
        .is_empty()
        && dispatcher
            .dispatch_new(&[], std::slice::from_ref(&a), 2_500)
            .is_empty();
    assert!(suppressed, "cooldown must suppress the re-fire");
    assert_eq!(dispatcher.dispatch_new(&[], &[a], 12_000).len(), 1);
}

#[test]
fn request_carries_typed_severity_and_instance() {
    let mut dispatcher = AlertDispatcher::new(NotificationPolicy {
        enabled: true,
        cooldown_ms: 0,
        ..NotificationPolicy::default()
    });
    let request = dispatcher
        .dispatch_new(&[], &[alert("cpu-0", AlertSeverity::Critical)], 1_000)
        .remove(0);
    assert_eq!(request.instance_id, "cpu-0");
    assert_eq!(request.severity, AlertSeverity::Critical);
    assert_eq!(request.target, "t");
}
