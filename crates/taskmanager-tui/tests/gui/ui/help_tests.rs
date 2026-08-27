use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskmanager_application::CommandId;

use crate::demo_app;

fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    // Pin English and serialize against the language-flipping i18n test
    // (see ui::LANG_TEST_GUARD). The overlay title resolves through the
    // process-global t(), which otherwise auto-seeds from the host locale.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_help_overlay(frame, app, crate::TuiTheme::default(), frame.area()))
        .expect("draw");
    terminal.backend().to_string()
}

#[test]
fn help_rows_drop_unwired_dialog_confirm_and_sidebar_and_add_terminal_only_bindings() {
    let rows = help_rows();
    let shortcuts: Vec<&str> = rows.iter().map(|row| row.shortcut).collect();
    // The shared Confirm(Enter) is NOT wired into the TUI dialog, and the
    // TUI has no sidebar surface, so neither may appear in the honest
    // overlay.
    assert!(
        !rows
            .iter()
            .any(|row| row.shortcut == "Enter" && row.label == "Confirm"),
        "Enter-as-confirm is not wired into the TUI dialog"
    );
    assert!(
        !rows.iter().any(|row| row.shortcut == "F9"),
        "the sidebar toggle is not wired into the TUI"
    );
    // Terminal-only bindings are present and attributed honestly.
    assert!(shortcuts.contains(&"?"));
    assert!(shortcuts.contains(&"q"));
    assert!(shortcuts.contains(&"s"));
    assert!(shortcuts.contains(&"S"));
    // Every shared command except Confirm and ToggleSidebar is still
    // represented, plus the five terminal-only bindings and the TUI-local
    // overlay bindings.
    let shared_count = crate::command_help()
        .into_iter()
        .filter(|help| {
            help.command != CommandId::Confirm && help.command != CommandId::ToggleSidebar
        })
        .count();
    assert_eq!(rows.len(), shared_count + 5 + TUI_LOCAL_BINDINGS.len());
}

#[test]
fn overlay_renders_title_shortcuts_and_close_hint() {
    let mut app = demo_app();
    app.toggle_help();
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("Keyboard reference"));
    // A real shared shortcut is listed.
    assert!(text.contains("Ctrl+F"));
    // Terminal-only binding is listed.
    assert!(text.contains("Cycle sort column"));
    // TUI-local overlay bindings are listed too.
    assert!(text.contains("Settings"));
    assert!(text.contains("Containers"));
    assert!(text.contains("Export snapshot"));
    // Close hint is shown (F1 is the `?` help alias, so it is advertised
    // too).
    assert!(text.contains("? / Esc"));
    assert!(text.contains("F1 / ? / Esc"));
}

#[test]
fn help_overlay_scrolls_its_two_column_listing_on_a_short_terminal() {
    let mut app = demo_app();
    app.toggle_help();
    // A short frame: the overlay shrinks to the terminal and the listing
    // would clip without the scroll slice. At offset 0 the top rows of
    // both columns are visible.
    let text = frame_text(&app, 80, 20);
    assert!(text.contains("Keyboard reference"));
    assert!(
        text.contains("Quit TaskForest"),
        "first terminal row visible"
    );

    // Scrolling past the listing bottom keeps a bounded, non-panicking
    // frame (the renderer clamps the intent).
    app.help_scroll_by(200);
    let text = frame_text(&app, 80, 20);
    assert!(text.contains("Keyboard reference"), "overlay still renders");
    assert!(text.contains("F1 / ? / Esc"), "footer hint still visible");
}

#[test]
fn help_overlay_advertises_the_performance_and_service_page_keys() {
    let rows = help_rows();
    let shortcuts: Vec<&str> = rows.iter().map(|row| row.shortcut).collect();
    // The genuinely wired device-page chords are advertised.
    assert!(shortcuts.contains(&"o"), "service logs");
    assert!(shortcuts.contains(&"e"), "GPU engines / escalation");
    assert!(shortcuts.contains(&"d"), "directory usage scan");
}
