//! Tab focus ring between the process table and the inline detail panel.
//!
//! These tests drive `handle_key` (the same path crossterm uses) and render the
//! real frame, asserting on behavior (focus state, panel scroll, cursor
//! immobility) rather than source text.

use super::super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyModifiers;
use taskmanager_application::AppPage;

use crate::TuiTheme;
use crate::render;

fn app_on_processes() -> crate::TuiApp {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    app.shell.selected = 0;
    app.reconcile_applications_cursor();
    app
}

#[test]
fn tab_cycles_the_focus_between_table_and_details_panel() {
    let mut app = app_on_processes();
    assert_eq!(app.focus_panel, crate::FocusPanel::Table);

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
    );
    assert_eq!(app.focus_panel, crate::FocusPanel::Details);

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
    );
    assert_eq!(app.focus_panel, crate::FocusPanel::Table);
}

#[test]
fn details_focus_scrolls_the_panel_without_moving_the_cursor() {
    let mut app = app_on_processes();
    let selected_before = app.selected;

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert!(app.detail_scroll > 0, "Down must scroll the focused panel");
    assert_eq!(
        app.selected, selected_before,
        "the table cursor must not move while the panel is focused"
    );

    // Up scrolls back.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Up, KeyModifiers::NONE),
    );
    assert_eq!(app.detail_scroll, 0);
    assert_eq!(app.selected, selected_before);

    // Esc returns focus to the table; Down then moves the cursor again.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert_eq!(app.focus_panel, crate::FocusPanel::Table);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert_ne!(app.selected, selected_before);
}

#[test]
fn tab_with_an_active_search_closes_the_field_instead_of_cycling() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        ),
    );
    assert!(app.search_active());
    assert_eq!(app.focus_panel, crate::FocusPanel::Table);

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
    );
    assert!(
        !app.search_active(),
        "Tab must close the search field while it is active"
    );
    assert_eq!(
        app.focus_panel,
        crate::FocusPanel::Table,
        "Tab must not cycle focus while the search field owns the keyboard"
    );
}

#[test]
fn focused_panel_renders_the_visual_focus_marker() {
    let mut app = app_on_processes();
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);

    let backend = TestBackend::new(140, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, &app, TuiTheme::default()))
        .expect("draw");
    let unfocused = terminal.backend().to_string();
    assert!(
        !unfocused.contains("▸ Process details"),
        "unfocused panel must not show the focus marker, got:\n{unfocused}"
    );

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
    );
    terminal
        .draw(|frame| render(frame, &app, TuiTheme::default()))
        .expect("draw");
    let focused = terminal.backend().to_string();
    assert!(
        focused.contains("▸ Process details"),
        "focused panel must render the focus marker, got:\n{focused}"
    );
}
