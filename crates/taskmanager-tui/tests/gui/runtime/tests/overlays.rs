//! Modal overlay key handling: the keyboard-help (`?`) and threshold-
//! suggestions (`T`) overlays, and the mutually-exclusive local overlays
//! (settings / health / containers / about).

use super::super::*;

#[test]
fn help_toggle_is_modal_and_sort_keys_change_the_active_column() {
    use taskmanager_shell::{SortCol, SortDir};

    let mut app = crate::demo_app();

    // F1 opens the plain keyboard-help overlay (`?` owns the command palette).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::F(1), KeyModifiers::NONE),
    );
    assert!(app.help_open());

    // The overlay is modal: while open, a sort key is swallowed unchanged.
    let before = app.process_sort;
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.process_sort, before);

    // Esc dismisses the overlay.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(!app.help_open());

    // `s` cycles the sort column (default CPU -> Memory), keeping direction.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.process_sort, (SortCol::Memory, SortDir::Desc));

    // `S` reverses the direction without changing the column.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('S'),
            KeyModifiers::SHIFT,
        ),
    );
    assert_eq!(app.process_sort, (SortCol::Memory, SortDir::Asc));
}

#[test]
fn threshold_suggestions_toggle_is_modal_and_esc_dismisses() {
    let mut app = crate::demo_app();
    assert!(!app.suggestions_open());

    // `T` (Shift+t) opens the suggestions overlay. The help overlay must be
    // closed first because it is modal and swallows `T` while it is open.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('T'),
            KeyModifiers::SHIFT,
        ),
    );
    assert!(app.suggestions_open());

    // The overlay is modal: while open, a sort key is swallowed unchanged.
    let before = app.process_sort;
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.process_sort, before);

    // Esc dismisses the overlay without touching the active page.
    let page_before = app.page();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(!app.suggestions_open());
    assert_eq!(app.page(), page_before);

    // `T` toggles it back open, and a second `T` closes it from the modal.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('T'),
            KeyModifiers::SHIFT,
        ),
    );
    assert!(app.suggestions_open());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('T'),
            KeyModifiers::SHIFT,
        ),
    );
    assert!(!app.suggestions_open());
}

#[test]
fn local_overlay_toggles_are_mutually_exclusive_and_modal() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('p'),
            KeyModifiers::NONE,
        ),
    );
    assert!(app.settings_open());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('h'),
            KeyModifiers::NONE,
        ),
    );
    assert!(app.health_open());
    assert!(!app.settings_open(), "opening health closes settings");
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('c'),
            KeyModifiers::NONE,
        ),
    );
    assert!(app.containers_open());
    assert!(!app.health_open(), "opening containers closes health");

    // The overlay is modal: a page key is swallowed while open.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Performance);

    // Esc closes the open overlay without touching the page.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(!app.containers_open());
    assert_eq!(app.page(), AppPage::Performance);

    // `i` opens about; the toggle key closes it from the modal.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('i'),
            KeyModifiers::NONE,
        ),
    );
    assert!(app.about_open());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('i'),
            KeyModifiers::NONE,
        ),
    );
    assert!(!app.about_open());
}
