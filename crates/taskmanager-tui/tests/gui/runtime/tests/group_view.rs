//! Canonical Applications category-tree interaction tests.

use super::super::*;
use taskmanager_application::{AppAction, ProcessItem, ScalarObservation};

fn trustworthy_process(pid: u32, name: &str, parent_pid: Option<u32>) -> ProcessItem {
    let mut process = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.into())
        .parent_pid(parent_pid)
        .build();
    process.apply_scalar_observations(taskmanager_application::ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid), 42),
        ..Default::default()
    });
    process
}

#[test]
fn enter_and_right_toggle_the_category_header() {
    let mut app = crate::demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![trustworthy_process(
            11, "demo", None,
        )])),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.expanded_groups.clear();
    app.selected = 0;
    assert_eq!(app.visual_row_count(), 1);

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.expanded_groups.contains("category:uncategorized"));
    assert_eq!(app.visual_row_count(), 2);

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Right,
            KeyModifiers::NONE,
        ),
    );
    assert!(!app.expanded_groups.contains("category:uncategorized"));
}

#[test]
fn left_collapses_a_recursive_process_node() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            trustworthy_process(1, "root", None),
            trustworthy_process(2, "child", Some(1)),
        ])),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:uncategorized".to_string()].into_iter().collect();
    app.selected = 1;

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Left, KeyModifiers::NONE),
    );
    assert!(app.collapsed_tree.contains(&1));
    assert_eq!(app.visual_row_count(), 2, "header plus collapsed root");
}

#[test]
fn selection_motion_emits_and_then_dedupes_process_insights() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![trustworthy_process(
            1, "root", None,
        )])),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:uncategorized".to_string()].into_iter().collect();

    let first = app.move_nonflat_selection_oneshot(1);
    assert!(matches!(first, Some(PlatformEffect::ProcessInsights(_))));
    let second = app.refresh_selected_process_insights();
    assert!(second.is_none());
}

#[test]
fn application_aggregate_selection_is_pidless() {
    use taskmanager_application::{ProcessApplicationIdentity, ProcessMetadataObservation};
    let identity = ProcessApplicationIdentity::new("org.example.Editor", "Editor", None)
        .expect("identity fixture");
    let mut process = trustworthy_process(11, "editor", None);
    process.apply_application_identity(ProcessMetadataObservation::available(identity, 10));
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![process])),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:application".to_string()].into_iter().collect();

    let _ = app.move_nonflat_selection_oneshot(1);
    assert_eq!(
        app.shell.selected_process_row,
        Some(taskmanager_shell::ProcessRowKey::Application(11))
    );
    assert!(app.shell.selected_pids().is_empty());
    assert!(app.shell.selected_process_identity().is_none());
}

/// A live-style process snapshot that changes the process domain must retire
/// the TUI-local per-pid tree state for exited pids (the same timing the
/// shell prunes its stale selections): a reused pid cannot inherit a stale
/// collapse, and a dead application root's expansion key cannot linger.
#[test]
fn a_process_domain_change_prunes_stale_per_pid_tree_state() {
    use taskmanager_application::{
        CapabilityId, CorrelatedEvent, EventSequence, PlatformEventBatch, PlatformEventContext,
        ProcessEvent, RequestId,
    };

    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            trustworthy_process(1, "root", None),
            trustworthy_process(2, "child", Some(1)),
            trustworthy_process(3, "other", None),
        ])),
    );
    app.application.active_page = AppPage::Applications;
    // Local tree state: the live root's aggregate stays expanded, its child
    // and an unrelated pid are collapsed, a dead root keeps a stale
    // expansion key, and one category key carries no pid at all.
    app.expanded_groups.insert("app-tree:1".to_string());
    app.expanded_groups.insert("app-tree:9".to_string());
    app.expanded_groups
        .insert("category:application".to_string());
    app.collapsed_tree.extend([2u32, 3]);

    // The next live batch reports only pid 1: pids 2 and 3 exited (and 9
    // never existed). The fold goes through the TUI's batch entry, exactly
    // like the runtime event loop does.
    let mut batch = PlatformEventBatch::default();
    batch.process_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(1).expect("fixture request id"),
            capability: CapabilityId::PROCESS_LIST,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
        },
        ProcessEvent::Snapshot(vec![trustworthy_process(1, "root", None)]),
    ));
    app.apply_platform_batch(batch);

    assert!(
        app.expanded_groups.contains("app-tree:1"),
        "the still-live root keeps its expansion"
    );
    assert!(
        app.expanded_groups.contains("category:application"),
        "pid-less category keys are never pruned"
    );
    assert!(
        !app.expanded_groups.contains("app-tree:9"),
        "the dead root's expansion key must be retired"
    );
    assert!(
        !app.collapsed_tree.contains(&2) && !app.collapsed_tree.contains(&3),
        "exited pids must not stay collapsed (pid reuse would show them pre-collapsed)"
    );
}
