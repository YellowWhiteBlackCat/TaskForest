//! Column-visibility menu: open/close/toggle behavior, the visible-sort
//! relocation, and the frame dropping hidden columns from the header and rows.
//!
//! These tests drive `handle_key` (the same path crossterm uses) and render the
//! real frame through `render`, asserting on the drawn frame text — not source
//! `.contains()`.

use super::super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyModifiers;
use taskmanager_application::AppPage;

use crate::TuiTheme;
use crate::render;

/// Render the live frame through the same TestBackend path the render tests
/// use.
fn frame_text(app: &crate::TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, app, TuiTheme::default()))
        .expect("draw");
    terminal.backend().to_string()
}

/// A demo app parked on the Applications page with a selected process.
fn app_on_processes() -> crate::TuiApp {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    app.shell.selected = 0;
    app
}

#[test]
fn c_key_opens_the_column_menu_and_esc_closes_it() {
    let mut app = app_on_processes();
    assert!(!app.column_menu_open());

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('C'),
            KeyModifiers::SHIFT,
        ),
    );
    assert!(app.column_menu_open(), "C must open the column menu");

    // The menu renders the column list with a visible toggle.
    let text = frame_text(&app, 100, 40);
    assert!(
        text.contains("Columns"),
        "menu title must render, got:\n{text}"
    );
    assert!(
        text.contains("CPU"),
        "a toggleable column label must render"
    );

    // Esc closes; the table regains its keys.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(!app.column_menu_open(), "Esc must close the column menu");
}

#[test]
fn column_menu_toggle_hides_and_reshows_a_column() {
    let mut app = app_on_processes();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('C'),
            KeyModifiers::SHIFT,
        ),
    );
    // Default cursor is on CPU (index 0). Enter hides it.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.hidden_columns.contains(&crate::SortCol::Cpu));
    assert!(!app.column_visible(crate::SortCol::Cpu));

    // Enter again on the same row re-shows it.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.column_visible(crate::SortCol::Cpu));
}

#[test]
fn hiding_the_active_sort_column_relocates_the_sort_to_a_visible_column() {
    let mut app = app_on_processes();
    // The demo sorts by CPU by default; hide CPU through the menu.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('C'),
            KeyModifiers::SHIFT,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.hidden_columns.contains(&crate::SortCol::Cpu));
    // The sort must have moved to the first visible column (Memory).
    assert_eq!(
        app.effective_sort_col(),
        crate::SortCol::Memory,
        "the sort must relocate to the first visible column"
    );
}

#[test]
fn hidden_columns_disappear_from_the_header_and_rows() {
    let mut app = app_on_processes();
    // Hide CPU + DiskRead: the header must lose both labels and the rows must
    // stay aligned (the memory readout still renders, the disk columns do not).
    app.hidden_columns.insert(crate::SortCol::Cpu);
    app.hidden_columns.insert(crate::SortCol::DiskRead);
    let text = frame_text(&app, 140, 40);
    assert!(
        !text.contains("CPU%"),
        "the hidden CPU column must not render its header, got:\n{text}"
    );
    assert!(
        !text.contains("Disk R/s"),
        "the hidden disk column must not render its header, got:\n{text}"
    );
    assert!(
        text.contains("Memory"),
        "a visible column header must still render, got:\n{text}"
    );
    // The demo's selected process (zed) still renders its memory readout.
    assert!(
        text.contains("MiB"),
        "row cells must still render, got:\n{text}"
    );
}

#[test]
fn sort_cycle_walks_only_the_visible_columns() {
    let mut app = app_on_processes();
    // Default sort is CPU; the visible cycle is PID → Name → CPU → Memory →
    // PSS → Swap → User → State (advanced columns hidden). One `s` from CPU
    // lands on Memory.
    assert_eq!(app.effective_sort_col(), crate::SortCol::Cpu);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.effective_sort_col(), crate::SortCol::Memory);

    // Hide Memory: `s` now skips it (CPU → PSS).
    app.hidden_columns.insert(crate::SortCol::Memory);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.effective_sort_col(), crate::SortCol::Pss);
}
