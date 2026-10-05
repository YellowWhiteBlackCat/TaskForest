//! test-intent: behavior
//! Native modal keys, bracketed paste, terminal output and persisted restart.
use super::*;
use crate::ui::test_support::{install_config_store, repo_temp_dir};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use taskmanager_application::AppPage;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_shell::saved_views::{SavedViewTransferFeedback, export_saved_views_json};
use taskmanager_shell::{ProcessStatusFilter, SortCol, SortDir};

fn key(app: &mut TuiApp, code: KeyCode) {
    assert!(crate::runtime::handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).is_none());
}
fn settle(app: &mut TuiApp, expected: usize) {
    app.config_client
        .as_ref()
        .expect("client")
        .synchronize(std::time::Duration::from_secs(2))
        .expect("save finished");
    app.drain_config_publications();
    assert_eq!(app.config_draft.saved_process_views.len(), expected);
}
#[test]
fn saved_views_native_keys_and_paste_persist_apply_delete_and_restart() {
    let root = repo_temp_dir().join("tui-saved-views");
    std::fs::create_dir_all(&root).expect("directory");
    let path = root.join("config.json");
    let mut app = crate::demo_app();
    install_config_store(&mut app, &path);
    app.shell
        .set_process_status_filter(ProcessStatusFilter::Sleeping);
    app.shell.set_sort_column(SortCol::Memory);
    app.shell.process_sort.1 = SortDir::Desc;
    app.hidden_columns = std::collections::HashSet::from([SortCol::Pss]);
    app.toggle_settings();
    key(&mut app, KeyCode::F(4));
    assert_eq!(app.local_surface_kind(), Some(TuiSurfaceKind::SavedViews));
    key(&mut app, KeyCode::F(2));
    settle(&mut app, 1);
    let json = export_saved_views_json(&app.saved_views.rows).expect("JSON");
    let before = app.saved_views.rows.clone();
    assert!(
        !app.paste_saved_views(&json),
        "unarmed paste cannot mutate views"
    );
    key(&mut app, KeyCode::F(6));
    assert!(app.paste_saved_views(&json));
    settle(&mut app, 2);
    key(&mut app, KeyCode::F(6));
    assert!(app.paste_saved_views("invalid"));
    assert_eq!(
        app.saved_views.feedback,
        Some(SavedViewTransferFeedback::ImportInvalid)
    );
    assert_eq!(app.saved_views.rows.len(), before.len() + 1);
    let payload = app
        .handle_saved_views_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE))
        .expect("export intent");
    let mut output = Vec::new();
    app.export_saved_views_to(&mut output, &payload);
    assert_eq!(
        output,
        format!(
            "\x1b]52;c;{}\x07",
            crate::clipboard::base64_encode(payload.as_bytes())
        )
        .into_bytes()
    );
    assert_eq!(
        app.saved_views.feedback,
        Some(SavedViewTransferFeedback::ExportCopied)
    );
    let mut broken = std::io::Cursor::new(&mut [] as &mut [u8]);
    app.export_saved_views_to(&mut broken, &payload);
    assert_eq!(
        app.saved_views.feedback,
        Some(SavedViewTransferFeedback::ExportFailed)
    );
    app.shell
        .set_process_status_filter(ProcessStatusFilter::All);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.page(), AppPage::Applications);
    assert_eq!(
        app.shell.process_status_filter,
        ProcessStatusFilter::Sleeping
    );
    assert_eq!(app.shell.process_sort, (SortCol::Memory, SortDir::Desc));
    assert_eq!(
        app.hidden_columns,
        std::collections::HashSet::from([SortCol::Pss])
    );
    app.open_saved_views();
    key(&mut app, KeyCode::Home);
    key(&mut app, KeyCode::Delete);
    settle(&mut app, 1);
    drop(app);
    let mut restarted = crate::demo_app();
    install_config_store(&mut restarted, &path);
    assert_eq!(
        restarted
            .saved_views
            .rows
            .iter()
            .filter(|row| row.is_user_saved())
            .count(),
        1
    );
    assert_eq!(
        restarted
            .saved_views
            .rows
            .last()
            .expect("restored")
            .hidden_cols,
        std::collections::HashSet::from([SortCol::Pss])
    );
    drop(restarted);
    std::fs::remove_dir_all(root).expect("cleanup");
}
#[test]
fn saved_views_whole_groups_and_all_actions_fit_compact_viewports_after_import_feedback() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("language");
    set_language(Language::En);
    let mut app = crate::demo_app();
    app.open_saved_views();
    key(&mut app, KeyCode::F(2));
    key(&mut app, KeyCode::F(6));
    for (width, height) in [
        (54, 16),
        (80, 24),
        (120, 36),
        (180, 20),
        (54, 50),
        (200, 60),
    ] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| crate::ui::render(frame, &app, crate::TuiTheme::default()))
            .expect("draw");
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        for label in [
            "Saved views",
            "Hidden columns: 0",
            "Enter Apply",
            "Esc Close",
            "F2 Save current Apps view",
            "Del Remove",
            "F5 Export",
            "F6 Import",
            "Paste JSON to import",
        ] {
            assert!(
                text.contains(label),
                "{width}x{height} missing {label}: {text}"
            );
        }
    }
}
