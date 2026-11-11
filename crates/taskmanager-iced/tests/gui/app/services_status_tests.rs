use crate::app::{AppPage, FocusTarget, Message, ServiceStatusFilter};

#[test]
fn services_status_filter_narrows_visible_rows_without_rebuilding_facts() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Services));
    let (canonical, all_indices, first_generation) = app.services_projection("");
    assert_eq!(all_indices.len(), 5);

    let _ = app.update(Message::SelectServiceStatusFilter(
        ServiceStatusFilter::Active,
    ));
    let (rows, active_indices, active_generation) = app.services_projection("");
    assert_eq!(app.services_status_filter, ServiceStatusFilter::Active);
    assert!(std::rc::Rc::ptr_eq(&canonical, &rows));
    assert_ne!(active_generation, first_generation);
    assert_eq!(active_indices.len(), 3);
    assert!(
        rows.iter()
            .enumerate()
            .filter(|(index, _)| active_indices.contains(index))
            .all(|(_, row)| row.status == taskforest_core::core::services::ServiceStatus::Active)
    );

    let _ = app.update(Message::SelectServiceStatusFilter(
        ServiceStatusFilter::Failed,
    ));
    let (_, failed_indices, _) = app.services_projection("");
    assert_eq!(failed_indices.len(), 1);
    assert_eq!(canonical[failed_indices[0]].name, "demo-failed.service");
}

#[test]
fn services_status_filter_composes_with_query_and_clamps_selection() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Services));
    let _ = app.update(Message::ServicesSearchChanged("network".into()));
    let (_, query_indices, _) = app.services_projection(app.services_query());
    assert_eq!(query_indices.len(), 2);
    app.shell.move_selection_to(1);

    let _ = app.update(Message::SelectServiceStatusFilter(
        ServiceStatusFilter::Inactive,
    ));
    let (_, inactive_indices, _) = app.services_projection(app.services_query());
    assert!(inactive_indices.is_empty());
    assert_eq!(app.shell.selected, 0);

    let _ = app.update(Message::SelectServiceStatusFilter(ServiceStatusFilter::All));
    let (_, restored_indices, _) = app.services_projection(app.services_query());
    assert_eq!(restored_indices.len(), 2);
}

#[test]
fn services_status_segment_values_and_focus_target_are_stable() {
    let labels = ServiceStatusFilter::ALL
        .iter()
        .map(|filter| filter.label())
        .collect::<Vec<_>>();
    assert_eq!(labels.len(), 4);
    assert!(labels.iter().all(|label| !label.is_empty()));
    assert_eq!(
        ServiceStatusFilter::from_index(2),
        Some(ServiceStatusFilter::Inactive)
    );
    assert_eq!(ServiceStatusFilter::from_index(4), None);
    assert!(FocusTarget::ALL.contains(&FocusTarget::ServiceStatusFilterSegment));
    assert_eq!(
        crate::focus::focus_id(FocusTarget::ServiceStatusFilterSegment),
        "iced-services-status-filter"
    );
}
