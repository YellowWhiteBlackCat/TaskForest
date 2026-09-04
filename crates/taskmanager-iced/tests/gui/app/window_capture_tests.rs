//! Current window screenshot capture tests for the Iced adapter.

use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity};

use super::*;
use crate::ui::current_window_capture_btn;
use crate::ui::view;

#[test]
fn window_capture_without_installed_client_reports_unavailable() {
    taskmanager_test_support::pin_english();
    let mut app = IcedApp::demo();

    let result = app.request_current_window_capture();
    assert!(!result);

    let notice = app.shell.feedback_notice().expect("unavailable notice");
    assert_eq!(notice.severity(), FeedbackSeverity::Error);
    assert_eq!(notice.lifecycle(), FeedbackLifecycle::TIMED_LONG);
    assert_eq!(notice.text(), "Current window capture is unavailable");
}

#[test]
fn window_capture_message_routes_to_request_current_window_capture() {
    taskmanager_test_support::pin_english();
    let mut app = IcedApp::demo();

    let _ = app.update(Message::RequestCurrentWindowCapture);

    let notice = app.shell.feedback_notice().expect("unavailable notice");
    assert_eq!(notice.severity(), FeedbackSeverity::Error);
    assert_eq!(notice.lifecycle(), FeedbackLifecycle::TIMED_LONG);
    assert_eq!(notice.text(), "Current window capture is unavailable");
}

#[test]
fn current_window_capture_button_renders_in_header_strip() {
    let app = IcedApp::demo();
    let _btn = current_window_capture_btn(app.theme(), app.language());
    let _view = view(&app);
}

#[test]
fn current_window_capture_focus_target_is_stable_and_in_all() {
    assert!(FocusTarget::ALL.contains(&FocusTarget::WindowCapture));
    assert_eq!(
        crate::focus::focus_id(FocusTarget::WindowCapture),
        "iced-window-capture"
    );
}

#[test]
fn drain_window_capture_completions_without_client_is_inert() {
    let mut app = IcedApp::demo();
    assert!(!app.drain_window_capture_completions());

    let _ = app.update(Message::Tick);
    assert!(!app.drain_window_capture_completions());
}
