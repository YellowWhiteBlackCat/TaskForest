use super::*;
use crate::core::alerts::{AlertMetric, AlertSeverity};

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
fn disabled_policy_never_notifies() {
    let mut gate = NotificationGate::new(NotificationPolicy {
        enabled: false,
        ..Default::default()
    });
    assert!(!gate.consider(&alert("a", AlertSeverity::Critical), 1_000));
}

#[test]
fn cooldown_suppresses_repeat_within_window() {
    let mut gate = NotificationGate::new(NotificationPolicy {
        enabled: true,
        cooldown_ms: 5_000,
        ..Default::default()
    });
    assert!(gate.consider(&alert("a", AlertSeverity::Warning), 1_000));
    assert!(
        !gate.consider(&alert("a", AlertSeverity::Critical), 5_999),
        "same instance inside cooldown must be suppressed"
    );
    assert!(
        gate.consider(&alert("a", AlertSeverity::Warning), 6_000),
        "cooldown elapsed boundary must notify again"
    );
}

#[test]
fn cooldown_is_per_instance() {
    let mut gate = NotificationGate::new(NotificationPolicy {
        enabled: true,
        cooldown_ms: 10_000,
        ..Default::default()
    });
    assert!(gate.consider(&alert("a", AlertSeverity::Info), 1_000));
    assert!(
        gate.consider(&alert("b", AlertSeverity::Info), 1_500),
        "different instance must not share cooldown state"
    );
}

#[test]
fn quiet_hours_suppresses_and_boundaries_work() {
    let hours = QuietHours {
        start_minutes: 22 * 60,
        end_minutes: 7 * 60,
    };
    let mut gate = NotificationGate::new(NotificationPolicy {
        enabled: true,
        cooldown_ms: 0,
        quiet_hours: Some(hours),
    });
    let at = |hour: u16, minute: u16| u64::from(hour) * 3_600_000 + u64::from(minute) * 60_000;
    assert!(!gate.consider(&alert("a", AlertSeverity::Critical), at(23, 30)));
    assert!(gate.consider(&alert("a", AlertSeverity::Critical), at(7, 0)));
    assert!(gate.consider(&alert("a", AlertSeverity::Critical), at(21, 59)));
    assert!(!gate.consider(&alert("a", AlertSeverity::Critical), at(6, 59)));
}

#[test]
fn quiet_hours_equal_bounds_never_suppress() {
    let hours = QuietHours {
        start_minutes: 600,
        end_minutes: 600,
    };
    let mut gate = NotificationGate::new(NotificationPolicy {
        enabled: true,
        cooldown_ms: 0,
        quiet_hours: Some(hours),
    });
    assert!(gate.consider(&alert("a", AlertSeverity::Critical), 600 * 60_000));
}

#[test]
fn day_span_quiet_hours() {
    let hours = QuietHours {
        start_minutes: 60,
        end_minutes: 120,
    };
    let mut gate = NotificationGate::new(NotificationPolicy {
        enabled: true,
        cooldown_ms: 0,
        quiet_hours: Some(hours),
    });
    let at = |hour: u16, minute: u16| u64::from(hour) * 3_600_000 + u64::from(minute) * 60_000;
    assert!(gate.consider(&alert("a", AlertSeverity::Warning), at(0, 59)));
    assert!(!gate.consider(&alert("a", AlertSeverity::Warning), at(1, 0)));
    assert!(!gate.consider(&alert("a", AlertSeverity::Warning), at(1, 30)));
    assert!(!gate.consider(&alert("a", AlertSeverity::Warning), at(1, 59)));
    assert!(gate.consider(&alert("a", AlertSeverity::Warning), at(2, 0)));
}

#[test]
fn quiet_bound_semantics_are_single_source() {
    // Setting one bound creates a window against the 00:00 default.
    let window = apply_quiet_hour_bound(None, QuietBound::End, 7);
    assert_eq!(
        window,
        Some(QuietHours {
            start_minutes: 0,
            end_minutes: 7 * 60,
        })
    );
    let window = apply_quiet_hour_bound(window, QuietBound::Start, 22);
    assert_eq!(
        window,
        Some(QuietHours {
            start_minutes: 22 * 60,
            end_minutes: 7 * 60,
        })
    );
    // Equal bounds clear the window (never-suppressing semantics).
    assert_eq!(apply_quiet_hour_bound(window, QuietBound::End, 22), None);
    assert_eq!(apply_quiet_hour_bound(window, QuietBound::Start, 7), None);
}

#[test]
fn clear_forgets_delivery_history() {
    let mut gate = NotificationGate::new(NotificationPolicy {
        enabled: true,
        cooldown_ms: 10_000,
        ..Default::default()
    });
    assert!(gate.consider(&alert("a", AlertSeverity::Info), 1_000));
    gate.clear();
    assert!(
        gate.consider(&alert("a", AlertSeverity::Info), 1_000),
        "clear must reset cooldown state"
    );
}

#[test]
fn cooldown_history_retires_instances_that_can_no_longer_affect_a_verdict() {
    let mut gate = NotificationGate::new(NotificationPolicy {
        enabled: true,
        cooldown_ms: 10,
        ..Default::default()
    });
    for now_ms in 1..=100 {
        assert!(gate.consider(
            &alert(&format!("device-{now_ms}"), AlertSeverity::Info),
            now_ms,
        ));
    }
    assert!(
        gate.last_notified_ms.len() <= 11,
        "only identities inside the cooldown window may remain tracked"
    );
    assert!(!gate.last_notified_ms.contains_key("device-1"));
    assert!(gate.last_notified_ms.contains_key("device-100"));
}
