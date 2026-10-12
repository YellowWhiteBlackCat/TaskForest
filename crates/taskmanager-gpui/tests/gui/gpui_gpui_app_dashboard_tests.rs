use super::{DashboardState, EventCenterState};
use taskmanager_core::core::AlertEventKind;
use taskmanager_core::core::SystemSnapshot;
use taskmanager_core::core::metrics::{CpuMetrics, CpuScalarObservations, ScalarObservation};
use taskmanager_shell::{DirectTrackState, ProcessStatusFilter, SortCol};

#[test]
fn event_center_reads_shared_history_and_tracks_local_read_state() {
    let mut center = EventCenterState::default();
    let mut shell = DirectTrackState::default();
    shell.replace_alert_event_history(EventCenterState::capture_event_fixture());
    let events = shell.projection().alert_center.event_history();
    let visible = center.visible_events(events);
    assert_eq!(visible.len(), 2);
    assert_eq!(visible[0].kind, AlertEventKind::Cleared);
    assert_eq!(center.unread_count(events), 2);
    center.mark_all_read(events);
    assert_eq!(center.unread_count(events), 0);

    let first_tick = SystemSnapshot {
        timestamp_ms: 1_000,
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(95.0, 1_000),
            ..Default::default()
        }),
        ..SystemSnapshot::default()
    };
    let second_tick = SystemSnapshot {
        timestamp_ms: 11_000,
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(95.0, 11_000),
            ..Default::default()
        }),
        ..SystemSnapshot::default()
    };
    let _ = shell.evaluate_alerts(&first_tick, 1_000);
    let _ = shell.evaluate_alerts(&second_tick, 11_000);
    let appended = shell.projection().alert_center.event_history();
    assert_eq!(appended.len(), 3);
    assert_eq!(center.unread_count(appended), 1);

    shell.clear_alert_event_history();
    center.clear();
    assert!(shell.projection().alert_center.event_history().is_empty());
    shell.replace_alert_event_history(EventCenterState::capture_event_fixture());
    assert_eq!(
        center.unread_count(shell.projection().alert_center.event_history()),
        2,
        "events installed after clear receive newer IDs and are unread"
    );
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
