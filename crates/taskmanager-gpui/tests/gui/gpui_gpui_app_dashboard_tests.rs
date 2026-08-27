use super::{DashboardState, EventCenterState, EventKind};
use crate::core::{Alert, AlertMetric, AlertSeverity};
use crate::gpui_app::processes_view::{ProcessStatusFilter, SortCol};

fn alert(id: &str) -> Alert {
    Alert {
        instance_id: id.into(),
        rule_id: "rule".into(),
        target: "system".into(),
        metric: AlertMetric::CpuUsagePercent,
        severity: AlertSeverity::Warning,
        value: 95.0,
        threshold: 90.0,
        active_since_ms: 1,
    }
}

#[test]
fn event_center_records_only_transitions_and_bounds_storage() {
    let mut center = EventCenterState::default();
    let active = alert("cpu:system");
    center.observe(&[], std::slice::from_ref(&active), 10);
    center.observe(
        std::slice::from_ref(&active),
        std::slice::from_ref(&active),
        11,
    );
    center.observe(std::slice::from_ref(&active), &[], 12);
    let events = center.visible_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, EventKind::Cleared);
    assert_eq!(center.unread_count(), 2);
    center.mark_all_read();
    assert_eq!(center.unread_count(), 0);
}

#[test]
fn saved_view_names_and_capture_fixture_are_stable() {
    let mut state = DashboardState::new();
    assert_eq!(state.saved_views.len(), 3);
    state.add_capture_saved_view();
    state.add_capture_saved_view();
    assert_eq!(state.saved_views.len(), 4);
    assert_eq!(state.saved_views.last().map(|view| view.id), Some(90_000));

    // The built-in preset carries running-process sort/filter policy. The
    // hierarchy is canonical and therefore absent from preset state.
    let preset = &state.saved_views[1];
    assert_eq!(preset.filter, ProcessStatusFilter::Running);
    assert_eq!(preset.sort_col, SortCol::Cpu);
    assert!(!preset.sort_asc);
}
