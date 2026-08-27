//! OSC 52 clipboard copy tests: the `y` key on the Applications page and the
//! `copy_selected_process` payload contract.

use super::super::*;

use taskmanager_application::AppAction;

#[test]
fn y_copies_the_selected_pid_and_name_via_osc52() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let selected = app.selected_detail_process().expect("a row is selected");

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('y'),
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.feedback_text().contains(&selected.pid.to_string()),
        "status names the copied pid: {}",
        app.feedback_text()
    );
}

#[test]
fn copy_selected_process_writes_pid_tab_name_to_the_sink() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let selected = app.selected_detail_process().expect("a row is selected");

    let mut sink = Vec::new();
    app.copy_selected_process(&mut sink);
    let bytes = String::from_utf8(sink).expect("utf8 escape");
    assert!(bytes.starts_with("\x1b]52;c;"), "OSC 52 clipboard write");
    let payload =
        crate::clipboard::base64_encode(format!("{}\t{}", selected.pid, selected.name).as_bytes());
    assert!(
        bytes.contains(payload.as_str()),
        "the encoded pid<TAB>name reaches the emulator"
    );
    assert!(bytes.ends_with('\u{7}'), "BEL terminates the sequence");
}

#[test]
fn copy_without_a_selected_row_reports_an_honest_status() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::demo_app();
    // An empty process list has no row to copy.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(Vec::new())),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));

    let mut sink = Vec::new();
    app.copy_selected_process(&mut sink);
    assert!(sink.is_empty(), "no row to copy writes nothing");
    assert!(
        app.feedback_text().contains("No process"),
        "{}",
        app.feedback_text()
    );
}
