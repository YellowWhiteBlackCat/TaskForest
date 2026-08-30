use super::{DashboardState, EventCenterState};
use taskmanager_core::core::AlertEventKind;
use taskmanager_shell::{ProcessStatusFilter, SortCol};

#[test]
fn event_center_reads_shared_history_and_tracks_local_read_state() {
    let mut center = EventCenterState::default();
    let events = EventCenterState::capture_event_fixture();
    let visible = center.visible_events(&events);
    assert_eq!(visible.len(), 2);
    assert_eq!(visible[0].kind, AlertEventKind::Cleared);
    assert_eq!(center.unread_count(&events), 2);
    center.mark_all_read(&events);
    assert_eq!(center.unread_count(&events), 0);
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
