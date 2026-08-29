use super::*;
use taskmanager_core::core::session::SessionItem;

#[test]
fn user_projection_preserves_session_identity_and_remote_facts() {
    // `remote_text` now resolves Yes/No through the shared catalog, which
    // auto-detects the host locale on first use; pin English so the
    // assertion is deterministic and independent of the host language.
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);

    let shell = taskmanager_shell::demo_app();
    assert_eq!(user_list_state(&shell), ListState::Ready);

    let rows = user_rows(&shell);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "2");
    assert_eq!(rows[0].uid, 1000);
    assert_eq!(rows[0].user, "devuser");
    assert_eq!(rows[0].seat.as_deref(), Some("seat0"));
    assert_eq!(rows[0].tty.as_deref(), Some("tty2"));
    assert_eq!(remote_text(rows[0].remote), "No");
    assert_eq!(remote_text(rows[1].remote), "Yes");
    assert_eq!(rows[1].timestamp.as_deref(), Some("2026-07-29 11:20"));
}

#[test]
fn user_projection_distinguishes_loading_empty_and_missing_fields() {
    // Localized copy: pin English so the assertion is identical on
    // every runner regardless of the host locale.
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let shell = ShellApp::new();
    assert_eq!(user_list_state(&shell), ListState::Loading);
    assert_eq!(
        user_heading(ListState::Loading, 0),
        "Users · waiting for inventory…"
    );

    let mut shell = ShellApp::new();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sessions(Some(Vec::new())),
    );
    assert_eq!(user_list_state(&shell), ListState::Empty);
    assert_eq!(user_heading(ListState::Empty, 0), "Users · 0 reported");

    taskmanager_shell::fixture::seed_projection_fact(
        &mut shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sessions(Some(vec![SessionItem {
            id: "local".into(),
            uid: 0,
            user: "unknown".into(),
            seat: Some(String::new()),
            tty: None,
            remote: false,
            timestamp: None,
        }])),
    );
    let rows = user_rows(&shell);
    assert_eq!(user_list_state(&shell), ListState::Ready);
    assert_eq!(optional_text(rows[0].seat.as_deref()), "—");
    assert_eq!(optional_text(rows[0].tty.as_deref()), "—");
    assert_eq!(optional_text(rows[0].timestamp.as_deref()), "—");
}

/// The Users page owns no filter of its own: a Processes-page query never
/// shrinks the session list, so the name cells render plain — highlighting
/// by `shell.query` (the old wiring) would tint rows that search never
/// kept on this page.
#[test]
fn user_rows_have_no_page_filter_so_names_stay_plain_under_a_shared_search() {
    let mut shell = taskmanager_shell::demo_app();
    shell.query = "root".into();
    shell.open_search();

    let rows = user_rows(&shell);
    assert_eq!(
        rows.len(),
        2,
        "every demo session stays visible: no page filter consumes shell.query"
    );
    assert!(
        rows.iter().all(|row| row.user != "root"),
        "the shared query matches no session user, yet nothing is dropped"
    );

    // The page composes with the leftover shared search active.
    let mut app = crate::IcedApp::demo();
    app.shell.query = "root".into();
    app.shell.open_search();
    let _ = render(&app);
}
