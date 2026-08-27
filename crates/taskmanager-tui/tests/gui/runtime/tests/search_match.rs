//! Search enhancements: the query now also matches the command line, and
//! Enter while the search field is active jumps to the next matching row
//! (wrapping), mirroring the graphical frontends.
//!
//! The cmdline matching lives in the shared shell projection (so every
//! frontend benefits); the Enter-to-next-match is TUI-local navigation.

use super::super::*;
use ratatui::crossterm::event::KeyModifiers;
use taskmanager_application::AppPage;

fn app_on_processes() -> crate::TuiApp {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    app.shell.selected = 0;
    app.reconcile_applications_cursor();
    app
}

#[test]
fn search_matches_the_command_line() {
    // Seed a demo process with a distinct command line the name/user do not
    // carry, then confirm the shared projection keeps it visible.
    let mut app = app_on_processes();
    let processes = app.projection().processes.clone().expect("demo processes");
    let mut first = processes.first().expect("demo has processes").clone();
    first.cmdline = "/usr/bin/servicemanager --daemonize --config run.conf".to_owned();
    first.name = "smd".to_owned();
    let mut updated = processes.clone();
    updated[0] = first;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(updated)),
    );

    // A query that only matches the command line must still surface the row.
    app.query = "daemonize".to_owned();
    app.open_search();
    assert!(
        app.visible_processes().iter().any(|p| p.name == "smd"),
        "the cmdline match must keep the row visible"
    );
}

#[test]
fn enter_with_an_active_search_jumps_to_the_next_matching_row() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        ),
    );
    // Type "s": the shared projection re-filters to rows whose name (or user,
    // or cmdline) contains 's' — the cursor lands on the first filtered row.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert!(app.search_active());
    let filtered = app.visible_processes();
    assert!(
        filtered.len() >= 2,
        "the filtered demo list must have matches"
    );
    let names: Vec<String> = filtered.iter().map(|p| p.name.clone()).collect();

    // The cursor sits on the first filtered row; Enter advances to the NEXT
    // match, then keeps walking until it wraps back to the first.
    assert_eq!(
        app.selected_detail_process()
            .as_ref()
            .map(|process| process.name.as_str()),
        names.first().map(String::as_str),
        "the cursor lands on the first filtered process row"
    );
    for (expected, expected_name) in names.iter().enumerate().skip(1) {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Enter,
                KeyModifiers::NONE,
            ),
        );
        assert_eq!(
            app.selected_detail_process()
                .as_ref()
                .map(|process| process.name.as_str()),
            Some(expected_name.as_str()),
            "Enter must advance to filtered process {expected}"
        );
        assert!(app.search_active(), "the search field stays active");
    }
    // One more Enter wraps to the first match.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.selected_detail_process()
            .as_ref()
            .map(|process| process.name.as_str()),
        names.first().map(String::as_str),
        "Enter must wrap past the end"
    );
}

#[test]
fn enter_with_an_empty_search_keeps_the_cursor_put() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        ),
    );
    let selected_before = app.selected;
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(effect, None, "an empty query must not jump");
    assert_eq!(
        app.selected, selected_before,
        "an empty query must not move the cursor"
    );
}

#[test]
fn esc_clears_the_query_first_then_closes_the_field() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert!(app.search_active());
    assert_eq!(app.query, "s");

    // First Esc clears the query but keeps the field active (the shared
    // close path would leave the stale filter visible).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert_eq!(app.query, "", "Esc must clear the query first");
    assert!(app.search_active(), "the field stays active after clearing");

    // Second Esc closes the field through the shared path.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(!app.search_active(), "a second Esc closes the field");
}

#[test]
fn the_search_box_renders_the_match_counter() {
    use crate::TuiTheme;
    use crate::render;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut app = app_on_processes();
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    let count = app.visible_processes().len();
    assert!(count >= 1, "the 's' filter must keep matches");

    let backend = TestBackend::new(140, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, &app, TuiTheme::default()))
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(
        text.contains(&format!("{count} matches")),
        "the search box must show the match count, got:\n{text}"
    );
}
