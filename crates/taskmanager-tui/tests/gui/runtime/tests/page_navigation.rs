//! Applications category-tree paging and inert TUI-only chords.

use super::super::*;
use taskmanager_application::{AppAction, AppPage, ProcessItem, ScalarObservation};

fn process(pid: u32) -> ProcessItem {
    let mut process = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(format!("proc-{pid}"))
        .build();
    process.apply_scalar_observations(taskmanager_application::ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid), 1),
        ..Default::default()
    });
    process
}

#[test]
fn page_keys_move_over_the_category_tree_and_reset_detail_scroll() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(
            (1..=25).map(process).collect(),
        )),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:uncategorized".to_string()].into_iter().collect();
    app.detail_scroll = 4;

    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::PageDown,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.selected, taskmanager_shell::PAGE_STEP);
    assert_eq!(app.detail_scroll, 0);
    assert!(matches!(effect, Some(PlatformEffect::ProcessInsights(_))));

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::End, KeyModifiers::NONE),
    );
    assert_eq!(app.selected, app.visual_row_count() - 1);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Home, KeyModifiers::NONE),
    );
    assert_eq!(app.selected, 0);
}

#[test]
fn non_application_table_pages_keep_their_own_flat_bounds() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::End, KeyModifiers::NONE),
    );
    assert_eq!(app.selected, app.sorted_services().len().saturating_sub(1));
}

#[test]
fn f9_is_consumed_without_creating_a_terminal_sidebar() {
    let mut app = crate::demo_app();
    let before = app.input_scope();
    let effect = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::F(9), KeyModifiers::NONE),
    );
    assert!(effect.is_none());
    assert_eq!(app.input_scope(), before);
}
