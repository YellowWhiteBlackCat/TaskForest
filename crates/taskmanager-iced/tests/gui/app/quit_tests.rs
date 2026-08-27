//! Quit-path regression tests for the Iced application update loop.

use super::*;
use taskmanager_application::Modifiers;

struct TestTray;

impl taskmanager_app_host::TrayController for TestTray {
    fn set_visible(&self, _visible: bool) -> Result<(), taskmanager_app_host::TrayFailure> {
        Ok(())
    }

    fn set_tooltip(
        &self,
        _tooltip: Option<String>,
    ) -> Result<(), taskmanager_app_host::TrayFailure> {
        Ok(())
    }

    fn set_title(&self, _title: Option<String>) -> Result<(), taskmanager_app_host::TrayFailure> {
        Ok(())
    }

    fn set_item_checked(
        &self,
        _id: taskmanager_application::tray::TrayActionId,
        _checked: bool,
    ) -> Result<(), taskmanager_app_host::TrayFailure> {
        Ok(())
    }
}

/// The bare-`q` exit must actually route a window close; the shell flag alone
/// left a dead-end "quitting…" footer.
#[test]
fn bare_q_routes_a_window_close_task_and_sets_the_quit_flag() {
    let mut app = IcedApp::demo();
    assert!(!app.shell.should_quit());

    let _close_task = app.update(Message::Key(IcedKey::Character('q', Modifiers::NONE)));

    assert!(app.shell.should_quit());
    assert_eq!(
        app.shell.quit_reason(),
        Some(taskmanager_shell::QuitReason::Keyboard)
    );
}

#[test]
fn native_close_request_closes_when_no_tray_is_available() {
    let mut app = IcedApp::demo();
    assert!(!app.tray_available());

    let _close_task = app.update(Message::WindowCloseRequested);

    assert!(app.shell.should_quit());
    assert_eq!(
        app.shell.quit_reason(),
        Some(taskmanager_shell::QuitReason::WindowClose)
    );
}

#[test]
fn native_close_request_minimizes_when_the_tray_owns_the_process() {
    let mut app = IcedApp::demo();
    app.runtime.install_tray(Some(Box::new(TestTray)), None);

    let _minimize_task = app.update(Message::WindowCloseRequested);

    assert!(!app.shell.should_quit());
}
