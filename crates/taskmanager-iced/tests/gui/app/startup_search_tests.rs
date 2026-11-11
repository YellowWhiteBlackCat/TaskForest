use crate::app::{AppPage, Message};
use crate::keys::IcedKey;
use std::rc::Rc;
use taskmanager_application::{KeyCode, Modifiers};
use taskmanager_shell::ShellKeyEvent;

#[test]
fn startup_search_matches_name_and_exec_without_filtering_canonical_rows() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Startup));
    let canonical = app.startup_projection().0;
    assert_eq!(canonical.len(), 2);

    let _ = app.update(Message::StartupSearchChanged("clipboard".to_owned()));
    let visible = app.startup_visible_indices();
    assert_eq!(visible.len(), 1);
    assert_eq!(canonical[visible[0]].name, "Clipboard Sync");
    assert!(
        Rc::ptr_eq(&visible, &app.startup_visible_indices()),
        "unchanged queries reuse their cached visible-index allocation"
    );
    assert!(
        Rc::ptr_eq(&canonical, &app.startup_projection().0),
        "the query changes only the Startup page's visible index projection"
    );

    let _ = app.update(Message::StartupSearchChanged("WL-PASTE".to_owned()));
    let visible = app.startup_visible_indices();
    assert_eq!(visible.len(), 1);
    assert_eq!(canonical[visible[0]].exec, "wl-paste --watch");

    let _ = app.update(Message::StartupSearchChanged("no-such-entry".to_owned()));
    assert!(app.startup_visible_indices().is_empty());
    assert_eq!(app.startup_projection().0.len(), 2);
}

#[test]
fn filtered_startup_rows_keep_provider_identity_for_switches_and_menus() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Startup));
    let _ = app.update(Message::StartupSearchChanged("wl-paste".to_owned()));

    let canonical_index = app.startup_visible_indices()[0];
    let expected_id = app
        .shell
        .sorted_startup_entry_at(canonical_index)
        .expect("the visible Startup row maps to a sorted source row")
        .id
        .clone();
    let expected_source = app.shell.sorted_startup_indices()[canonical_index];
    assert_eq!(app.startup_source_index(0), Some(expected_source));

    let _ = app.update(Message::RequestStartupControlFor {
        index: 0,
        enabled: false,
    });
    assert_eq!(
        app.shell
            .pending_startup()
            .map(|request| request.entry.id.clone()),
        Some(expected_id.clone()),
        "the filtered row switch targets its provider entry"
    );
    let _ = app.update(Message::DismissOverlay);

    let _ = app.update(Message::OpenStartupRowMenu { visual_index: 0 });
    assert_eq!(app.startup_menu_index(), Some(expected_source));
    assert_eq!(
        app.startup_menu_entry().map(|entry| entry.id.clone()),
        Some(expected_id),
        "the filtered row menu freezes the same provider entry"
    );
}

#[test]
fn startup_keyboard_navigation_stays_inside_filtered_rows() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Startup));
    let _ = app.update(Message::StartupSearchChanged("s".to_owned()));
    assert_eq!(app.startup_visible_indices().len(), 2);

    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::ArrowDown,
        Modifiers::NONE,
    ))));
    assert_eq!(app.shell.selected, 1);

    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::End,
        Modifiers::NONE,
    ))));
    assert_eq!(app.shell.selected, 1);
}
