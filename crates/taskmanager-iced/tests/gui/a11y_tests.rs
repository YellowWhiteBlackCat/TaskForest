use super::*;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::process_semantic_key;
use taskmanager_ui_contract::{SemanticAction, SemanticNodeId, SemanticRole};

use crate::app::{AlertsMessage, Message};

fn process_row_id(pid: u32) -> SemanticNodeId {
    SemanticNodeId::owned(format!(
        "row:process:pid:{pid}:start:{}",
        taskmanager_test_support::fixture_start_token(pid)
    ))
}

fn process_cell_id(pid: u32, cell: &str) -> SemanticNodeId {
    SemanticNodeId::owned(format!("{}:cell:{cell}", process_row_id(pid).as_str()))
}

#[test]
fn alerts_route_publishes_rule_switches_while_open() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::IcedApp::demo();
    // Closed: the frontend-local group is absent from the tree.
    let closed = semantic_snapshot_with_local(&app).expect("closed tree must build");
    assert!(
        closed
            .get(&SemanticNodeId::borrowed("alerts-rules"))
            .is_none()
    );

    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let open = semantic_snapshot_with_local(&app).expect("open tree must build");

    let group = open
        .get(&SemanticNodeId::borrowed("alerts-rules"))
        .expect("the alerts group publishes while the route is open");
    assert_eq!(group.role(), SemanticRole::Group);
    assert_eq!(group.name(), Some("Alert rules"));
    assert_eq!(
        group.children().count(),
        app.alerts_rules().len(),
        "one switch per managed rule"
    );

    let first = open
        .get(&SemanticNodeId::owned("alert-rule:cpu-high"))
        .expect("first rule switch");
    assert_eq!(first.role(), SemanticRole::Switch);
    assert_eq!(first.name(), Some("CPU usage"));
    assert_eq!(first.state().checked, Some(true));
    assert!(first.supports_action(SemanticAction::Toggle));
    assert_eq!(
        first.description(),
        Some("Warning · 90.0% · 37.4%"),
        "severity · threshold · honest current value"
    );
}

#[test]
fn closing_the_alerts_route_removes_the_rule_switches() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let _ = app.update(Message::Alerts(AlertsMessage::ClosePage));

    let closed = semantic_snapshot_with_local(&app).expect("closed tree must build");
    assert!(
        closed
            .get(&SemanticNodeId::borrowed("alerts-rules"))
            .is_none(),
        "the group must leave the tree with the route"
    );
    assert!(
        closed
            .get(&SemanticNodeId::owned("alert-rule:cpu-high"))
            .is_none()
    );
}

#[test]
fn a_disabled_rule_publishes_as_unchecked_and_a_firing_rule_names_itself() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));

    // Disable the first rule: the switch must publish the honest state.
    let _ = app.update(Message::Alerts(AlertsMessage::ToggleRule {
        rule_id: "cpu-high".into(),
    }));
    // Make the second rule (memory) fire: mirror one active alert whose
    // rule_id matches, exactly like the shell's evaluation mirror.
    let memory_rule_id = app.alerts_rules()[1].rule.id.clone();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ActiveAlerts(vec![
            taskmanager_core::core::alerts::Alert {
                instance_id: format!("{memory_rule_id}:"),
                rule_id: memory_rule_id.clone(),
                target: String::new(),
                metric: taskmanager_core::core::alerts::AlertMetric::MemoryUsagePercent,
                severity: taskmanager_core::core::alerts::AlertSeverity::Warning,
                value: 91.0,
                threshold: 90.0,
                active_since_ms: 0,
            },
        ]),
    );

    let snapshot = semantic_snapshot_with_local(&app).expect("tree must build");
    let disabled = snapshot
        .get(&SemanticNodeId::owned("alert-rule:cpu-high"))
        .expect("disabled rule switch");
    assert_eq!(disabled.state().checked, Some(false));

    let firing = snapshot
        .get(&SemanticNodeId::owned(format!(
            "alert-rule:{memory_rule_id}"
        )))
        .expect("firing rule switch");
    let description = firing.description().unwrap_or_default();
    assert!(
        description.ends_with("triggered"),
        "the firing rule's detail names the triggering flag: {description}"
    );
}

#[test]
fn demo_shell_projects_process_rows_graph_and_selection() {
    let mut shell = taskmanager_shell::demo_app();
    shell.selected = 1;
    let snapshot = semantic_snapshot(&shell).expect("demo semantic tree must build");

    assert_eq!(snapshot.root().as_str(), "app");
    assert!(
        snapshot
            .get(&SemanticNodeId::borrowed("cpu-graph"))
            .is_some()
    );
    assert_eq!(
        snapshot
            .nodes()
            .filter(|node| node.role() == SemanticRole::Row)
            .count(),
        shell.visible_process_count()
    );
    let selected_process = shell
        .visible_processes()
        .into_iter()
        .nth(shell.selected)
        .expect("selected demo process");
    let selected_id =
        SemanticNodeId::owned(format!("row:{}", process_semantic_key(selected_process)));
    assert_eq!(
        snapshot.get(&selected_id).map(|node| node.state().selected),
        Some(Some(true))
    );
}

#[test]
fn application_aggregate_never_fabricates_a_selected_process_semantic() {
    let mut shell = taskmanager_shell::demo_app();
    shell.selected = 1;
    let root = shell.visible_processes()[1];
    shell.selected_row = root
        .current_start_token()
        .and_then(|token| ProcessLiveKey::from_parts(root.pid, token))
        .map(taskmanager_shell::ProcessRowId::Application);
    shell.selected_rows.clear();

    let snapshot = semantic_snapshot(&shell).expect("semantic tree must build");
    assert!(
        snapshot
            .nodes()
            .filter(|node| node.role() == SemanticRole::Row)
            .all(|node| node.state().selected != Some(true))
    );
}

#[test]
fn first_loading_frame_omits_unobserved_graph_and_keeps_row_scalars_honest() {
    let mut shell = taskmanager_shell::ShellApp::new();
    let mut process = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(77)
        .name(String::from("unobserved"))
        .build();
    let mut observations = *process.scalar_observations();
    observations.cpu_percentage = ScalarObservation::unavailable(
        taskmanager_core::core::failure::FailureKind::PermissionDenied,
    );
    process.apply_scalar_observations(observations);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![process])),
    );
    // The direct slot swap follows the simulate-a-batch convention so the
    // shell's watermarked memos see it.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRefresh,
    );

    let snapshot = semantic_snapshot(&shell).expect("loading semantic tree must build");
    assert!(
        snapshot
            .get(&SemanticNodeId::borrowed("cpu-graph"))
            .is_none()
    );
    assert_eq!(
        snapshot
            .get(&process_cell_id(77, "cpu"))
            .and_then(|node| node.value_text()),
        Some("Unavailable")
    );
    assert_eq!(
        snapshot
            .get(&process_cell_id(77, "memory"))
            .and_then(|node| node.value_text()),
        Some("Unavailable")
    );
}

#[test]
fn active_iced_modal_is_exposed_as_dismissible_dialog_semantics() {
    let mut shell = taskmanager_shell::demo_app();
    shell.toggle_suggestions();
    let snapshot = semantic_snapshot(&shell).expect("modal semantic tree must build");
    let modal = snapshot
        .get(&SemanticNodeId::owned("modal:threshold-suggestions"))
        .expect("threshold modal semantic node");

    assert_eq!(modal.role(), SemanticRole::Dialog);
    assert!(modal.state().modal);
    assert!(modal.supports_action(SemanticAction::Dismiss));
}

#[test]
fn service_control_confirmation_is_a_dismissible_dialog_in_semantics() {
    let mut shell = taskmanager_shell::demo_app();
    let service = shell.projection().services.as_ref().expect("demo services")[0].clone();
    assert!(shell.select_service_control(
        &service,
        taskmanager_core::core::services::ServiceAction::Stop
    ));
    let _ = shell.apply_action(taskmanager_application::AppAction::RequestServiceControl);
    let snapshot = semantic_snapshot(&shell).expect("modal semantic tree must build");
    let modal = snapshot
        .get(&SemanticNodeId::owned("modal:service-control-confirmation"))
        .expect("service-control modal semantic node");

    assert_eq!(modal.role(), SemanticRole::Dialog);
    assert!(modal.state().modal);
    assert!(modal.supports_action(SemanticAction::Dismiss));
}

#[test]
fn process_properties_modal_is_a_dismissible_dialog_in_semantics() {
    let mut shell = taskmanager_shell::demo_app();
    let target = taskmanager_core::core::process::FrozenProcessIdentity::from_authoritative_parts(
        4242,
        "worker.exe",
        7_500,
        9_000,
    )
    .expect("valid identity");
    let _ = shell
        .application
        .interaction
        .reduce(taskmanager_application::InteractionEvent::OpenProcessProperties(target));
    let snapshot = semantic_snapshot(&shell).expect("properties modal semantic tree must build");
    let modal = snapshot
        .get(&SemanticNodeId::owned("modal:process-properties-modal"))
        .expect("properties modal semantic node");

    assert_eq!(modal.role(), SemanticRole::Dialog);
    assert!(modal.state().modal);
    assert!(modal.supports_action(SemanticAction::Dismiss));
}

#[test]
fn service_log_modal_is_a_dismissible_dialog_in_semantics() {
    let mut shell = taskmanager_shell::demo_app();
    let _ = shell.open_service_log_for(taskmanager_core::core::target::ServiceId::from(
        "systemd-journald",
    ));
    let snapshot = semantic_snapshot(&shell).expect("service log modal semantic tree must build");
    let modal = snapshot
        .get(&SemanticNodeId::owned("modal:service-log-modal"))
        .expect("service log modal semantic node");

    assert_eq!(modal.role(), SemanticRole::Dialog);
    assert!(modal.state().modal);
    assert!(modal.supports_action(SemanticAction::Dismiss));
}

#[test]
fn mapped_tree_is_well_formed_under_accesskit_consumer_oracle() {
    let app = crate::IcedApp::demo();
    let snapshot = semantic_snapshot_with_local(&app).expect("snapshot must build");
    let update = taskmanager_accessibility_linux::snapshot_to_tree_update(&snapshot);
    let tree = accesskit_consumer::Tree::new(update, false);

    let root = tree.state().root();
    assert_eq!(root.role(), accesskit::Role::Application);
    assert_eq!(root.label().as_deref(), Some(product::ICED_NAME));
}

#[test]
fn assistive_technology_actions_drive_iced_selection_and_modal() {
    let mut app = crate::IcedApp::demo();
    let snapshot = semantic_snapshot_with_local(&app).expect("snapshot must build");

    let process = app.shell.visible_processes()[1].clone();
    let row_node_id = format!("row:{}", process_semantic_key(&process));
    let request = taskmanager_ui_contract::AccessibilityActionRequest {
        snapshot_revision: snapshot.revision(),
        node: SemanticNodeId::owned(row_node_id),
        action: SemanticAction::Select,
        value: None,
    };
    apply_accessibility_action(&mut app, &request, &snapshot).expect("matching AT action");
    assert_eq!(
        app.shell.selected_row,
        process
            .current_start_token()
            .and_then(|t| ProcessLiveKey::from_parts(process.pid, t))
            .map(ProcessRowId::Process)
    );

    // Modal dismiss
    app.shell.toggle_suggestions();
    let modal_snapshot = semantic_snapshot_with_local(&app).expect("modal snapshot");
    assert!(app.shell.suggestions_open());
    let dismiss_request = taskmanager_ui_contract::AccessibilityActionRequest {
        snapshot_revision: modal_snapshot.revision(),
        node: SemanticNodeId::borrowed("modal:threshold-suggestions"),
        action: SemanticAction::Dismiss,
        value: None,
    };
    apply_accessibility_action(&mut app, &dismiss_request, &modal_snapshot)
        .expect("dismiss action");
    assert!(!app.shell.suggestions_open());
}
