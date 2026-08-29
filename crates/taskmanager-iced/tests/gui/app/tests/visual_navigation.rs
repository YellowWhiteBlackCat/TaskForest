//! Tests for visual navigation, keyboard shortcuts, and settings persistence in IcedApp.

use super::*;
use crate::app::SettingsChange;
use crate::test_support::temp_dir;
use taskmanager_application::{AppPage, ConfigStore, KeyCode, Modifiers};

use taskmanager_shell::ShellKeyEvent;

fn grouped_fixture(
    pid: u32,
    name: &str,
    cpu: f32,
    memory_bytes: u64,
) -> taskmanager_core::core::process::ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.into())
        .current_cpu_percentage(cpu)
        .current_memory_bytes(memory_bytes)
        .metadata_observations(
            taskmanager_core::core::process::ProcessMetadataObservations::current(
                taskmanager_core::core::process::ProcessOwner::opaque("devuser"),
                None,
                1,
            ),
        )
        .build()
}

#[test]
fn visual_navigation_walks_the_category_projection() {
    let mib = 1024 * 1024_u64;
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            grouped_fixture(100, "zed", 24.8, 2_640 * mib),
            grouped_fixture(101, "zed-worker", 11.2, 1_000 * mib),
            grouped_fixture(102, "gnome-shell", 9.6, 1_120 * mib),
        ])),
    );
    // Projection is [Uncategorized header, process 100, process 101,
    // process 102]. The structural header has no flat selection identity.
    let down = || {
        Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
            KeyCode::ArrowDown,
            Modifiers::NONE,
        )))
    };
    let up = || {
        Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
            KeyCode::ArrowUp,
            Modifiers::NONE,
        )))
    };

    assert_eq!(app.process_presentation.visual_cursor, 0);
    assert_eq!(app.shell.selected, 0);
    // Down: category header → first process.
    let _ = app.update(down());
    assert_eq!(app.process_presentation.visual_cursor, 1);
    assert_eq!(app.shell.selected, 0);
    // Down: second process.
    let _ = app.update(down());
    assert_eq!(app.process_presentation.visual_cursor, 2);
    assert_eq!(app.shell.selected, 1);
    // Down: third process.
    let _ = app.update(down());
    assert_eq!(app.process_presentation.visual_cursor, 3);
    assert_eq!(app.shell.selected, 2);
    // Down past the end clamps to the last row.
    let _ = app.update(down());
    assert_eq!(app.process_presentation.visual_cursor, 3);
    assert_eq!(app.shell.selected, 2);
    // Up walks back up through the member rows to the header.
    let _ = app.update(up());
    assert_eq!(app.process_presentation.visual_cursor, 2);
    assert_eq!(app.shell.selected, 1);
    let _ = app.update(up());
    assert_eq!(app.process_presentation.visual_cursor, 1);
    assert_eq!(app.shell.selected, 0);
    let _ = app.update(up());
    assert_eq!(app.process_presentation.visual_cursor, 0);
    assert_eq!(app.shell.selected, 0);
}

#[test]
fn visual_left_right_toggles_category_headers() {
    let mib = 1024 * 1024_u64;
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            grouped_fixture(100, "zed", 24.8, 2_640 * mib),
            grouped_fixture(101, "zed-worker", 11.2, 1_000 * mib),
            grouped_fixture(102, "gnome-shell", 9.6, 1_120 * mib),
        ])),
    );
    app.process_presentation.expanded_groups.clear();
    let key = |code| Message::Key(IcedKey::Fixed(ShellKeyEvent::new(code, Modifiers::NONE)));
    let category = "category:uncategorized";
    assert!(!app.is_group_expanded(category));
    let _ = app.update(key(KeyCode::ArrowRight));
    assert!(app.is_group_expanded(category), "Right expands the header");
    // Left collapses it again (the cursor stays on the header).
    let _ = app.update(key(KeyCode::ArrowLeft));
    assert!(
        !app.is_group_expanded(category),
        "Left collapses the header"
    );
    assert_eq!(app.process_presentation.visual_cursor, 0);
}

#[test]
fn visual_left_right_toggles_category_tree_subtrees_and_left_goes_up_to_parent() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let mut root = grouped_fixture(100, "root-app", 24.8, 100);
    root.parent_pid = None;
    let mut child = grouped_fixture(101, "child-app", 11.2, 50);
    child.parent_pid = Some(100);
    let mut grandchild = grouped_fixture(102, "grandchild-app", 5.0, 30);
    grandchild.parent_pid = Some(101);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            root, child, grandchild,
        ])),
    );
    let key = |code| Message::Key(IcedKey::Fixed(ShellKeyEvent::new(code, Modifiers::NONE)));

    // Visual 0 is the category header; move to the root at visual 1.
    let _ = app.update(key(KeyCode::ArrowDown));
    assert!(!app.process_presentation.expanded_tree.contains(&100));
    let _ = app.update(key(KeyCode::ArrowRight));
    assert!(
        !app.process_presentation.expanded_tree.contains(&100),
        "root is already expanded"
    );
    let _ = app.update(key(KeyCode::ArrowDown));
    assert_eq!(app.process_presentation.visual_cursor, 2);
    assert_eq!(app.shell.selected, 1);
    // Left on the grandchild's parent (101) collapses it: select 101 first.
    assert_eq!(app.process_presentation.visual_cursor, 2);
    let _ = app.update(key(KeyCode::ArrowLeft));
    assert!(
        app.process_presentation.expanded_tree.contains(&101),
        "Left collapses the expanded subtree"
    );
    // Left again: the node is already collapsed, so the cursor moves to its
    // parent (100).
    let _ = app.update(key(KeyCode::ArrowLeft));
    assert_eq!(
        app.shell.selected, 0,
        "Left on a collapsed node goes to parent"
    );
    // Right re-expands the subtree.
    let _ = app.update(key(KeyCode::ArrowRight));
    assert!(!app.process_presentation.expanded_tree.contains(&101));
}

#[test]
fn visual_navigation_is_category_owned_on_applications_and_shell_owned_elsewhere() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let arrow_down = Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::ArrowDown,
        Modifiers::NONE,
    )));
    let _ = app.update(arrow_down.clone());
    assert_eq!(app.process_presentation.visual_cursor, 1);
    // Another page: same shared path.
    let _ = app.update(Message::SelectPage(AppPage::Services));
    let _ = app.update(arrow_down);
    assert_eq!(app.shell.selected, 1);
}

#[test]
fn f1_toggles_the_help_sheet_like_question_mark() {
    let mut app = IcedApp::demo();
    let f1 = || {
        Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
            KeyCode::F1,
            Modifiers::NONE,
        )))
    };
    assert!(!app.shell.help_open());
    let _ = app.update(f1());
    assert!(app.shell.help_open(), "F1 opens the help sheet");
    let _ = app.update(f1());
    assert!(!app.shell.help_open(), "F1 toggles it closed");
}

#[test]
fn f9_toggles_the_performance_sidebar() {
    let mut app = IcedApp::demo();
    let f9 = || {
        Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
            KeyCode::F9,
            Modifiers::NONE,
        )))
    };
    assert!(app.performance.sidebar_visible);
    let _ = app.update(f9());
    assert!(
        !app.performance.sidebar_visible,
        "F9 hides the device sidebar"
    );
    let _ = app.update(f9());
    assert!(app.performance.sidebar_visible, "F9 restores it");
}

#[test]
fn startup_page_preference_opens_the_configured_page_at_boot() {
    use taskmanager_application::AppPage;
    let dir = temp_dir("startup-page");
    let path = dir.join("config.json");
    // Remember-last with no recorded page → the Performance launch default.
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));
    app.load_config();
    assert_eq!(app.shell.page(), AppPage::Performance);
    // A persisted "apps" startup token opens Applications even when the
    // remember-last default would land elsewhere.
    let store = ConfigStore::new(&path);
    let mut config = store.load_or_default();
    config.startup_page = "apps".into();
    store.save(&config).unwrap();
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));
    app.load_config();
    assert_eq!(app.shell.page(), AppPage::Applications);
    // A persisted last-page token restores the remember-last page.
    let store = ConfigStore::new(&path);
    let mut config = store.load_or_default();
    config.startup_page = "".into();
    config.last_page = "apps".into();
    store.save(&config).unwrap();
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));
    app.load_config();
    assert_eq!(app.shell.page(), AppPage::Applications);
}

#[test]
fn page_navigation_persists_the_remember_last_token() {
    let dir = temp_dir("remember-last");
    let path = dir.join("config.json");
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    app.wait_for_config_where(|config| config.last_page == "apps");
    let config = ConfigStore::new(&path).load_or_default();
    assert_eq!(config.last_page, "apps");
    let _ = app.update(Message::SelectPage(AppPage::Performance));
    app.wait_for_config_where(|config| config.last_page == "performance");
    let config = ConfigStore::new(&path).load_or_default();
    assert_eq!(config.last_page, "performance");
}

#[test]
fn demo_instances_use_isolated_throwaway_config_paths() {
    let mut first = IcedApp::demo();
    let second = IcedApp::demo();
    let default = IcedApp::default();
    let _ = first.update(Message::SettingsChanged(SettingsChange::Language(
        crate::i18n::Language::Zh,
    )));
    assert_eq!(first.language(), crate::i18n::Language::Zh);
    assert_eq!(second.language(), crate::i18n::Language::En);
    assert_eq!(default.language(), crate::i18n::Language::En);
}

#[test]
fn notification_settings_reach_config_and_the_shared_alert_center() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SettingsChanged(
        SettingsChange::DesktopNotifications(true),
    ));
    assert!(
        app.shell.projection().alert_center.policy().enabled,
        "the opt-in must reach the shared alert center immediately"
    );
    let _ = app.update(Message::SettingsChanged(SettingsChange::QuietHoursStart(
        22,
    )));
    let _ = app.update(Message::SettingsChanged(SettingsChange::QuietHoursEnd(7)));
    let policy = app.shell.projection().alert_center.policy();
    let hours = policy.quiet_hours.expect("quiet hours applied");
    assert_eq!(hours.start_minutes, 22 * 60);
    assert_eq!(hours.end_minutes, 7 * 60);
    // The persisted mirror reflects the same values (round-trip through the
    // config store used by the settings modal).
    assert!(app.preferences().notify_enabled);
    assert_eq!(app.preferences().quiet_start, 22);
    assert_eq!(app.preferences().quiet_end, 7);
    // Equal hours mean no quiet hours (the gate treats them as never-
    // suppressing); setting start back to the end clears the window.
    let _ = app.update(Message::SettingsChanged(SettingsChange::QuietHoursStart(7)));
    assert_eq!(
        app.shell.projection().alert_center.policy().quiet_hours,
        None,
        "equal start/end must mean no quiet hours"
    );
}
