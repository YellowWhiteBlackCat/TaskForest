use crate::app::{AppPage, FocusTarget, Message};
use std::rc::Rc;
use taskmanager_shell::StartupStatusFilter;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

fn seed_enabled_states(app: &mut crate::IcedApp) {
    let mut entries = app
        .shell
        .projection()
        .startup_entries
        .clone()
        .expect("demo startup inventory is present");
    if let Some(first) = entries.first_mut() {
        first.enabled = false;
    }
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::StartupEntries(Some(entries)),
    );
}

#[test]
fn startup_status_filter_uses_shared_predicate_and_preserves_canonical_rows() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Startup));
    seed_enabled_states(&mut app);
    let (canonical, all_indices, first_generation) = app.startup_projection();
    assert_eq!(canonical.len(), 2);
    assert_eq!(all_indices.len(), 2);

    let _ = app.update(Message::SelectStartupStatusFilter(
        StartupStatusFilter::Enabled,
    ));
    let (rows, enabled_indices, enabled_generation) = app.startup_projection();
    assert_eq!(app.startup_status_filter, StartupStatusFilter::Enabled);
    assert!(Rc::ptr_eq(&canonical, &rows));
    assert_ne!(enabled_generation, first_generation);
    assert_eq!(enabled_indices.len(), 1);
    assert!(rows[enabled_indices[0]].enabled);

    let _ = app.update(Message::SelectStartupStatusFilter(
        StartupStatusFilter::Disabled,
    ));
    let (_, disabled_indices, disabled_generation) = app.startup_projection();
    assert_ne!(disabled_generation, enabled_generation);
    assert_eq!(disabled_indices.len(), 1);
    assert!(!rows[disabled_indices[0]].enabled);
}

#[test]
fn startup_status_filter_composes_with_query_and_keeps_provider_identity() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Startup));
    seed_enabled_states(&mut app);
    let _ = app.update(Message::StartupSearchChanged("clipboard".to_owned()));
    let _ = app.update(Message::SelectStartupStatusFilter(
        StartupStatusFilter::Disabled,
    ));
    assert!(app.startup_visible_indices().is_empty());

    let _ = app.update(Message::SelectStartupStatusFilter(StartupStatusFilter::All));
    let canonical_index = app.startup_visible_indices()[0];
    let expected_id = app
        .shell
        .sorted_startup_entry_at(canonical_index)
        .expect("visible Startup row maps to a sorted source row")
        .id
        .clone();
    let expected_source = app.shell.sorted_startup_indices()[canonical_index];

    let _ = app.update(Message::RequestStartupControlFor {
        index: 0,
        enabled: false,
    });
    assert_eq!(
        app.shell
            .pending_startup()
            .map(|request| request.entry.id.clone()),
        Some(expected_id.clone())
    );
    let _ = app.update(Message::DismissOverlay);

    let _ = app.update(Message::OpenStartupRowMenu { visual_index: 0 });
    assert_eq!(app.startup_menu_index(), Some(expected_source));
    assert_eq!(
        app.startup_menu_entry().map(|entry| entry.id.clone()),
        Some(expected_id)
    );
}

#[test]
fn startup_status_catalog_and_focus_target_are_stable() {
    assert_eq!(
        StartupStatusFilter::ALL
            .into_iter()
            .map(|filter| filter.key())
            .collect::<Vec<_>>(),
        ["all", "enabled", "disabled"]
    );
    assert!(
        StartupStatusFilter::ALL
            .into_iter()
            .all(|filter| !filter.label().is_empty())
    );
    assert!(FocusTarget::ALL.contains(&FocusTarget::StartupStatusFilterSegment));
    assert_eq!(
        crate::focus::focus_id(FocusTarget::StartupStatusFilterSegment),
        "iced-startup-status-filter"
    );
    assert!(matches!(
        crate::app::startup_filter::startup_filter_message(2),
        Message::SelectStartupStatusFilter(StartupStatusFilter::Disabled)
    ));
}
