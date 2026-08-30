use super::help_support::render_help_overlay;
use super::*;
use crate::demo_app;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

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
    // Shared commands explicitly absent from the TUI must not appear in the
    // honest overlay: confirmation is y/n/Esc, the sidebar has a terminal
    // selector, and Alerts management is not implemented yet.
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
    assert!(
        !rows.iter().any(|row| row.shortcut == "Alt+8"),
        "Alerts is not wired into the TUI yet"
    );
    // Terminal-only bindings are present and attributed honestly.
    assert!(shortcuts.contains(&"?"));
    assert!(shortcuts.contains(&"q"));
    assert!(shortcuts.contains(&"s"));
    assert!(shortcuts.contains(&"S"));
    // Every shared command except the explicitly-unbound commands is still
    // represented, plus the five terminal-only bindings and the TUI-local
    // overlay bindings.
    let shared_count = taskmanager_shell::presentation::command_help()
        .into_iter()
        .filter(|help| !crate::bindings::is_deliberately_unbound(help.command))
        .count();
    assert_eq!(
        rows.len(),
        shared_count + 5 + crate::command_palette::TUI_LOCAL_COMMANDS.len()
    );
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

// ── Modal host contract ──────────────────────────────────────────────────────
//
// Both help-surface overlays delegate their host to the shared `Modal`
// component. These tests pin what the host paints: the accent border frame
// and the iconified title row.

/// The rendered text of one buffer row.
fn text_row(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    let width = buffer.area.width as usize;
    let start = y as usize * width;
    buffer.content[start..start + width]
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

/// Assert the shared `Modal` host painted a complete accent border frame over
/// `popup` and that `needle` (icon + title) rides the frame's title row.
fn assert_modal_border_and_title(
    terminal: &Terminal<TestBackend>,
    popup: Rect,
    theme: crate::TuiTheme,
    needle: &str,
) {
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(popup.x, popup.y)].symbol(), "┌");
    assert_eq!(buffer[(popup.right() - 1, popup.y)].symbol(), "┐");
    assert_eq!(buffer[(popup.x, popup.bottom() - 1)].symbol(), "└");
    assert_eq!(
        buffer[(popup.right() - 1, popup.bottom() - 1)].symbol(),
        "┘"
    );
    assert_eq!(
        buffer[(popup.x, popup.y)].style().fg,
        Some(theme.accent),
        "the host border carries the accent tone"
    );
    let title_row = text_row(terminal, popup.y);
    assert!(
        title_row.contains(needle),
        "the title row must carry the iconified title, got: {title_row:?}"
    );
}

#[test]
fn help_overlay_modal_host_paints_border_and_title() {
    let mut app = demo_app();
    app.toggle_help();
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let theme = crate::TuiTheme::default();
    let popup = Rect::new(4, 2, 60, 14);
    let mut terminal = Terminal::new(TestBackend::new(70, 18)).expect("test terminal");
    terminal
        .draw(|frame| super::render_help_overlay_at(frame, &app, theme, popup))
        .expect("draw");
    assert_modal_border_and_title(
        &terminal,
        popup,
        theme,
        &format!(
            " {} {} ",
            theme.glyph(IconId::Settings),
            t("menu.keyboard_reference")
        ),
    );
}

#[test]
fn command_palette_modal_host_paints_border_and_title() {
    let mut app = demo_app();
    app.open_command_palette();
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let theme = crate::TuiTheme::default();
    let popup = Rect::new(4, 2, 60, 14);
    // A non-palette control paints no row highlight; the host border and
    // title are what this test pins.
    let focus = crate::ui::frame_plan::TuiFocusPlan {
        target: crate::ui::frame_plan::TuiFocusTarget::LocalSurface(
            crate::TuiSurfaceKind::CommandPalette,
        ),
        order: crate::ui::frame_plan::TuiFocusOrder::None,
        control: crate::ui::frame_plan::TuiFocusControl::Viewport,
    };
    let mut terminal = Terminal::new(TestBackend::new(70, 18)).expect("test terminal");
    terminal
        .draw(|frame| {
            super::render_command_palette_at(frame, &app, theme, focus, popup);
        })
        .expect("draw");
    assert_modal_border_and_title(
        &terminal,
        popup,
        theme,
        &format!(
            " {} {} ",
            theme.glyph(IconId::Search),
            t("menu.keyboard_reference")
        ),
    );
}
