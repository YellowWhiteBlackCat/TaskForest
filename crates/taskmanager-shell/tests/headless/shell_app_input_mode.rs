use super::*;

#[test]
fn input_mode_transitions_are_exclusive_and_branch_matched() {
    let mut app = ShellApp::new();
    assert_eq!(app.input_mode(), ShellInputMode::Content);

    app.open_search();
    assert_eq!(app.input_mode(), ShellInputMode::Search);
    app.toggle_help();
    assert_eq!(app.input_mode(), ShellInputMode::Help);
    app.close_search();
    assert_eq!(app.input_mode(), ShellInputMode::Help);
    app.toggle_suggestions();
    assert_eq!(app.input_mode(), ShellInputMode::Suggestions);
    app.dismiss_informational_overlay();
    assert_eq!(app.input_mode(), ShellInputMode::Content);

    app.toggle_search_focus();
    assert_eq!(app.input_mode(), ShellInputMode::Search);
    app.toggle_search_focus();
    assert_eq!(app.input_mode(), ShellInputMode::Content);
}
