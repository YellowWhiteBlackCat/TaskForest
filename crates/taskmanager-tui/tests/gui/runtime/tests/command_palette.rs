//! Searchable command palette: `?` opens it, typing narrows the keybindings,
//! Enter runs the selected shared action, Esc closes.

use super::super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyModifiers;
use taskmanager_application::{AppAction, AppPage};

use crate::TuiTheme;
use crate::render;

fn app_on_processes() -> crate::TuiApp {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    app.shell.selected = 0;
    app
}

#[test]
fn question_mark_opens_the_palette_and_esc_closes_it() {
    let mut app = app_on_processes();
    assert!(app.command_palette().is_none());

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('?'),
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.command_palette().is_some(),
        "? must open the command palette"
    );
    assert_eq!(
        app.input_scope(),
        crate::TuiInputScope::LocalSurface(crate::TuiSurfaceKind::CommandPalette),
        "the palette owns input directly instead of borrowing help_open"
    );
    assert!(!app.shell.help_open());

    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(120, 36);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, &app, TuiTheme::default()))
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(
        text.contains("filter · Enter run"),
        "the palette filter hint must render, got:\n{text}"
    );

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(app.command_palette().is_none());
    assert!(!app.shell.help_open());
}

#[test]
fn typing_narrows_the_rows_and_enter_runs_the_selected_action() {
    let mut app = app_on_processes();
    // The shared command labels resolve through the i18n catalog, so pin
    // English (like every other label-asserting test) before filtering.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('?'),
            KeyModifiers::NONE,
        ),
    );
    // Type "serv": only the Services row (label "Services", shortcut "Alt+3")
    // remains.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('e'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('r'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('v'),
            KeyModifiers::NONE,
        ),
    );
    let rows = app.filtered_palette_rows();
    assert!(!rows.is_empty(), "the filter must keep at least one row");
    assert!(
        rows.iter()
            .all(|row| row.label.contains("Services") || row.label.contains("Service")),
        "the filter must narrow to Services rows, got {:?}",
        rows.iter().map(|r| r.label).collect::<Vec<_>>()
    );

    // The first row is the executable ShowServices action; Enter runs it and
    // closes the palette.
    assert_eq!(app.page(), AppPage::Applications);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.page(), AppPage::Services, "Enter must run the action");
    assert!(
        app.command_palette().is_none(),
        "Enter must close the palette"
    );
}

#[test]
fn backspace_edits_the_palette_filter() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('?'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('v'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.command_palette().unwrap().filter, "sv");
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Backspace,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.command_palette().unwrap().filter,
        "s",
        "Backspace must pop the last filter character"
    );
}

#[test]
fn palette_runs_tui_local_actions_from_the_selected_row() {
    use crate::PaletteLocalAction;
    // The palette rows map terminal-only and TUI-local bindings onto
    // executable local actions.
    let rows = crate::TuiApp::palette_rows();
    let settings = rows
        .iter()
        .find(|row| row.shortcut == "p")
        .expect("settings row");
    assert_eq!(
        settings.local_action,
        Some(PaletteLocalAction::ToggleSettings),
        "a local row is executable from the palette"
    );
    let quit = rows
        .iter()
        .find(|row| row.shortcut == "q")
        .expect("quit row");
    assert_eq!(quit.local_action, Some(PaletteLocalAction::Quit));
    let copy = rows
        .iter()
        .find(|row| row.shortcut == "y")
        .expect("copy row");
    assert_eq!(copy.local_action, Some(PaletteLocalAction::CopyClipboard));

    // Running the local action through the palette state executes the same
    // TUI binding the direct key would.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('?'),
            KeyModifiers::NONE,
        ),
    );
    assert!(app.command_palette().is_some());
    // Type "sett" to narrow to the Settings row, then Enter runs it.
    for character in ['s', 'e', 't', 't'] {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Char(character),
                KeyModifiers::NONE,
            ),
        );
    }
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.settings_open(),
        "palette Enter runs the local settings action"
    );
    assert!(
        app.command_palette().is_none(),
        "palette closes after running"
    );
}

#[test]
fn palette_quit_sets_the_run_loop_flag() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('?'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('q'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.should_quit(),
        "palette Quit sets the same flag the q key does"
    );
    assert_eq!(
        app.quit_reason(),
        Some(taskmanager_shell::QuitReason::CommandPalette)
    );
}

#[test]
fn palette_rows_are_still_discoverable_when_not_executable() {
    // The prefix-jump row is a navigation gesture with no palette action.
    let rows = crate::TuiApp::palette_rows();
    let jump = rows
        .iter()
        .find(|row| row.label.starts_with("Jump by name prefix"))
        .expect("prefix-jump row");
    assert_eq!(jump.action, None);
    assert_eq!(jump.local_action, None);
}

#[test]
fn palette_maps_the_device_page_keys_and_scopes_their_actions() {
    use crate::PaletteLocalAction;
    let rows = crate::TuiApp::palette_rows();
    let scan = rows
        .iter()
        .find(|row| row.shortcut == "d")
        .expect("disk row");
    assert_eq!(
        scan.local_action,
        Some(PaletteLocalAction::ToggleDirectoryScan)
    );
    // The context-sensitive escalation key stays discoverable-only.
    let escalate = rows
        .iter()
        .find(|row| row.shortcut == "e")
        .expect("escalate row");
    assert_eq!(escalate.local_action, None);
}
