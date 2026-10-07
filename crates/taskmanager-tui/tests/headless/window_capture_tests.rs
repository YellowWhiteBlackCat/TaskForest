//! Current window screenshot capture tests for the TUI adapter.

use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, ShellApp};
use taskmanager_test_support::pin_english;

use crate::TuiApp;

#[test]
fn window_capture_without_installed_client_reports_unavailable() {
    pin_english();
    let mut app = TuiApp::from_shell(ShellApp::new());
    assert!(!app.request_current_window_capture());
    let notice = app.shell.feedback_notice().expect("unavailable notice");
    assert_eq!(notice.severity(), FeedbackSeverity::Error);
    assert_eq!(notice.lifecycle(), FeedbackLifecycle::TIMED_LONG);
    assert_eq!(notice.text(), "Current window capture is unavailable");
}

#[test]
fn drain_window_capture_completions_without_client_is_inert() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    assert!(!app.drain_window_capture_completions());
}
