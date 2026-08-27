use iced::advanced::widget::operation::Focusable;

use super::*;

#[test]
fn adapter_state_obeys_iced_focusable_contract() {
    let mut state = widget::State::default();
    assert!(!state.is_focused());

    state.focus();
    assert!(state.is_focused());

    state.unfocus();
    assert!(!state.is_focused());
}

#[test]
fn every_declared_focus_target_has_a_unique_operation_id() {
    let ids: Vec<_> = FocusTarget::ALL.into_iter().map(focus_id).collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len());
    assert!(ids.iter().all(|id| id.starts_with("iced-")));
}

#[test]
fn table_row_operation_ids_are_stable_and_page_bound() {
    let first = focus_id(FocusTarget::TableRow {
        page: taskmanager_application::AppPage::Applications,
        index: 0,
    });
    let second = focus_id(FocusTarget::TableRow {
        page: taskmanager_application::AppPage::Applications,
        index: 1,
    });
    let other_page = focus_id(FocusTarget::TableRow {
        page: taskmanager_application::AppPage::Services,
        index: 0,
    });
    assert_eq!(first, "iced-table-row-applications-0");
    assert_ne!(first, second);
    assert_ne!(first, other_page);
}
