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
    assert!(
        app.hidden_columns
            .contains(&taskmanager_shell::SortCol::Cpu)
    );
    assert!(!app.column_visible(taskmanager_shell::SortCol::Cpu));

    // Enter again on the same row re-shows it.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.column_visible(taskmanager_shell::SortCol::Cpu));
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
    assert!(
        app.hidden_columns
            .contains(&taskmanager_shell::SortCol::Cpu)
    );
    // The sort must have moved to the first visible column (Memory).
    assert_eq!(
        app.effective_sort_col(),
        taskmanager_shell::SortCol::Memory,
        "the sort must relocate to the first visible column"
    );
}

#[test]
fn hidden_columns_disappear_from_the_header_and_rows() {
    let mut app = app_on_processes();
    // Hide CPU + DiskRead: the header must lose both labels and the rows must
    // stay aligned (the memory readout still renders, the disk columns do not).
    app.hidden_columns.insert(taskmanager_shell::SortCol::Cpu);
    app.hidden_columns
        .insert(taskmanager_shell::SortCol::DiskRead);
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
    assert_eq!(app.effective_sort_col(), taskmanager_shell::SortCol::Cpu);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.effective_sort_col(), taskmanager_shell::SortCol::Memory);

    // Hide Memory: `s` now skips it (CPU → PSS).
    app.hidden_columns
        .insert(taskmanager_shell::SortCol::Memory);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.effective_sort_col(), taskmanager_shell::SortCol::Pss);
}

/// Paint the frame through the backend buffer so a character index in a line
/// IS the cell x coordinate. The table aligns the header and every row to the
/// same columns, so a shared x names the same column in both.
fn painted_lines(app: &crate::TuiApp, width: u16, height: u16) -> Vec<String> {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, app, TuiTheme::default()))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let buffer_width = usize::from(buffer.area.width);
    (0..usize::from(buffer.area.height))
        .map(|row| {
            buffer.content[row * buffer_width..(row + 1) * buffer_width]
                .iter()
                .map(|cell| cell.symbol().to_owned())
                .collect()
        })
        .collect()
}

/// The column menu's ←/→ gesture is the keyboard equivalent of dragging a
/// column to a new position: it swaps the selected column with its neighbour
/// in the one display order the header, rows, and widths all paint. This test
/// proves the observable result — the header order AND the selected row's cell
/// order both follow the move, the moved column keeps its identity, the fixed
/// PID/Name/CPU prefix never moves, and the gesture clamps at both ends.
#[test]
fn moving_the_selected_column_left_reorders_the_painted_table() {
    let mut app = app_on_processes();
    let default_order = crate::TuiApp::reorderable_columns().to_vec();

    // Open the column menu; its cursor starts on CPU, a fixed prefix column.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('C'),
            KeyModifiers::SHIFT,
        ),
    );
    assert!(app.column_menu_open());

    // The identity/CPU prefix never moves: Left/Right on it are honest no-ops.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Left, KeyModifiers::NONE),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Right,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.column_order, default_order,
        "the fixed PID/Name/CPU prefix never reorders"
    );

    // Move the cursor to State (toggleable index 5) and drag it left past
    // Memory/Pss/Swap/User to the front of the reorderable tail.
    for _ in 0..5 {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
        );
    }
    for _ in 0..4 {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Left, KeyModifiers::NONE),
        );
    }
    assert_eq!(
        &app.column_order[..2],
        &[
            taskmanager_shell::SortCol::State,
            taskmanager_shell::SortCol::Memory,
        ],
        "State must swap left past Memory"
    );

    // Clamp: State is first, so one more Left is a no-op; the last column
    // cannot move right either.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Left, KeyModifiers::NONE),
    );
    assert_eq!(
        app.column_order[0],
        taskmanager_shell::SortCol::State,
        "the left edge clamps"
    );
    for _ in 0..13 {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
        );
    }
    for _ in 0..14 {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Right,
                KeyModifiers::NONE,
            ),
        );
    }
    assert_eq!(
        app.column_order.last().copied(),
        Some(taskmanager_shell::SortCol::Network),
        "the right edge clamps"
    );

    // The moved column keeps its identity: the order stays a permutation of
    // the same reorderable columns (no drop, no duplicate).
    let mut seen: Vec<&str> = app
        .column_order
        .iter()
        .map(|column| column.label())
        .collect();
    seen.sort_unstable();
    let mut expected: Vec<&str> = default_order.iter().map(|column| column.label()).collect();
    expected.sort_unstable();
    assert_eq!(seen, expected, "reordering only permutes the same columns");

    // Close the menu and read the painted table: header and selected row both
    // follow the new order.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(!app.column_menu_open());
    let lines = painted_lines(&app, 240, 40);
    let header = lines
        .iter()
        .find(|line| line.contains("PID") && line.contains("Name"))
        .expect("the table header must paint");
    let state_x = header.find("State").expect("State header");
    let memory_x = header.find("Memory").expect("Memory header");
    assert!(
        state_x < memory_x,
        "the header must follow the reorder:\n{header}"
    );

    // The selected row's own cells follow the same order: the state word now
    // paints before the memory byte string on the zed row.
    let row = lines
        .iter()
        .find(|line| line.contains("zed") && line.contains("Running") && line.contains("GiB"))
        .expect("the zed row must paint with its state and memory cells");
    let running = row.find("Running").expect("state cell");
    let memory = row.find("GiB").expect("memory cell");
    assert!(
        running < memory,
        "the row cells must follow the reorder:\n{row}"
    );
}
