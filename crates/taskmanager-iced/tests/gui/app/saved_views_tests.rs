//! test-intent: behavior
//! Normal saved-view messages, atomic transfer and coordinator persistence.

use super::*;
use crate::app::{InputScope, LocalSurfaceKind, Message};
use taskmanager_application::ConfigStore;
use taskmanager_shell::saved_views::{SavedViewTransferFeedback, export_saved_views_json};
use taskmanager_shell::{ProcessStatusFilter, SortCol, SortDir};

#[test]
fn normal_saved_view_review_imports_applies_and_persists_across_restart() {
    let root = crate::test_support::temp_dir("saved-views");
    let path = root.join("config.json");
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));
    let _ = app.update(Message::OpenSavedViews);
    assert_eq!(
        app.input_scope(),
        InputScope::LocalSurface(LocalSurfaceKind::SavedViews)
    );
    app.shell
        .set_process_status_filter(ProcessStatusFilter::Sleeping);
    app.shell.set_sort_column(SortCol::Memory);
    app.shell.process_sort.1 = SortDir::Desc;
    let _ = app.update(Message::SaveCurrentProcessView);
    let id = app.saved_views.last().expect("saved").id;
    app.wait_for_config_where(|config| config.saved_process_views.len() == 1);
    let json = export_saved_views_json(&app.saved_views).expect("export");
    let _ = app.update(Message::SavedViewsClipboardRead(Some(json)));
    app.wait_for_config_where(|config| config.saved_process_views.len() == 2);
    assert!(matches!(
        app.saved_view_feedback,
        Some(SavedViewTransferFeedback::Imported(_))
    ));
    let before = app.saved_views.clone();
    let _ = app.update(Message::SavedViewsClipboardRead(Some("broken".into())));
    assert_eq!(
        app.saved_view_feedback,
        Some(SavedViewTransferFeedback::ImportInvalid)
    );
    assert_eq!(app.saved_views, before);
    app.shell
        .set_process_status_filter(ProcessStatusFilter::All);
    let _ = app.update(Message::ApplySavedView(id));
    assert_eq!(
        app.shell.process_status_filter,
        ProcessStatusFilter::Sleeping
    );
    assert_eq!(app.shell.process_sort, (SortCol::Memory, SortDir::Desc));
    let _ = app.update(Message::DeleteSavedView(id));
    app.wait_for_config_where(|config| config.saved_process_views.len() == 1);
    drop(app);
    let restarted = IcedApp::with_config_store(None, ConfigStore::new(&path));
    assert_eq!(
        restarted
            .saved_views
            .iter()
            .filter(|preset| preset.is_user_saved())
            .count(),
        1
    );
    assert_eq!(
        restarted.saved_views.last().expect("rehydrated").filter,
        ProcessStatusFilter::Sleeping
    );
    drop(restarted);
    std::fs::remove_dir_all(root).expect("cleanup");
}
