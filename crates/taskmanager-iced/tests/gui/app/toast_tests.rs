// test-intent: behavior

use crate::app::toast::ToastState;
use std::time::Duration;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

fn report(shell: &mut ShellApp, lifecycle: FeedbackLifecycle, text: &str) {
    shell.report_notice(
        FeedbackSource::Interaction,
        FeedbackSeverity::Info,
        lifecycle,
        text,
    );
}

#[test]
fn a_dismissed_toast_stays_hidden_until_the_notice_changes() {
    let mut shell = ShellApp::new();
    report(&mut shell, FeedbackLifecycle::UntilReplaced, "copied");
    let toast = ToastState::default();
    assert!(toast.sync(&shell, 1_000).is_some());
    toast.dismiss(&shell);
    assert!(toast.sync(&shell, 1_100).is_none());
    report(&mut shell, FeedbackLifecycle::UntilReplaced, "exported");
    assert!(toast.sync(&shell, 1_200).is_some());
}

#[test]
fn a_toast_expires_after_the_shared_short_feedback_duration() {
    let mut shell = ShellApp::new();
    report(&mut shell, FeedbackLifecycle::TIMED_SHORT, "copied");
    let toast = ToastState::default();
    let start = 10_000;
    assert!(toast.sync(&shell, start).is_some());
    let before = start + FeedbackLifecycle::SHORT_DURATION.as_millis() as u64 - 1;
    assert!(toast.sync(&shell, before).is_some());
    let after = start + FeedbackLifecycle::SHORT_DURATION.as_millis() as u64;
    assert!(toast.sync(&shell, after).is_none());
    assert!(toast.sync(&shell, after + 1).is_none());
}

#[test]
fn an_until_replaced_toast_survives_the_short_duration() {
    let mut shell = ShellApp::new();
    report(&mut shell, FeedbackLifecycle::UntilReplaced, "saved");
    let toast = ToastState::default();
    let start = 10_000;
    assert!(toast.sync(&shell, start).is_some());
    let after = start + FeedbackLifecycle::LONG_DURATION.as_millis() as u64 * 2;
    assert!(
        toast.sync(&shell, after).is_some(),
        "the shell's UntilReplaced lifetime outlives renderer time"
    );
    toast.dismiss(&shell);
    assert!(
        toast.sync(&shell, after + 1).is_none(),
        "explicit dismissal is the renderer-local visibility stop"
    );
}

#[test]
fn a_platform_batched_toast_waits_for_shell_replacement() {
    let mut shell = ShellApp::new();
    report(&mut shell, FeedbackLifecycle::SHORT, "batched");
    let toast = ToastState::default();
    let start = 10_000;
    assert!(toast.sync(&shell, start).is_some());
    let after = start + FeedbackLifecycle::LONG_DURATION.as_millis() as u64 * 2;
    assert!(toast.sync(&shell, after).is_some());
    report(&mut shell, FeedbackLifecycle::UntilReplaced, "replaced");
    assert!(toast.sync(&shell, after + 1).is_some());
}

#[test]
fn a_timed_toast_keeps_its_visibility_receipt_across_shell_countdown_ticks() {
    let mut shell = ShellApp::new();
    report(&mut shell, FeedbackLifecycle::TIMED_SHORT, "saving");
    let toast = ToastState::default();
    let start = 10_000;
    assert!(toast.sync(&shell, start).is_some());
    toast.dismiss(&shell);
    assert!(toast.sync(&shell, start + 100).is_none());
    shell.advance_feedback_time(Duration::from_millis(100));
    assert!(
        toast.sync(&shell, start + 200).is_none(),
        "the shell's remaining-duration countdown does not replace the notice identity"
    );
}

#[test]
fn a_root_dismiss_message_hides_the_current_toast_without_clearing_the_shell_notice() {
    let mut app = crate::IcedApp::demo();
    report(&mut app.shell, FeedbackLifecycle::UntilReplaced, "copied");
    assert!(app.toast.sync(&app.shell, 100).is_some());
    let _ = app.update(crate::Message::DismissToast);
    assert!(
        app.shell.feedback_notice().is_some(),
        "toast dismissal is side-effect free for the shared feedback fact"
    );
    assert!(app.toast.sync(&app.shell, 200).is_none());
}

#[test]
fn clearing_the_notice_resets_the_dismissed_receipt() {
    let mut shell = ShellApp::new();
    report(&mut shell, FeedbackLifecycle::UntilReplaced, "copied");
    let toast = ToastState::default();
    assert!(toast.sync(&shell, 100).is_some());
    toast.dismiss(&shell);
    assert!(toast.sync(&shell, 200).is_none());
    shell.clear_feedback_notice();
    assert!(toast.sync(&shell, 300).is_none());
    report(&mut shell, FeedbackLifecycle::UntilReplaced, "copied");
    assert!(toast.sync(&shell, 400).is_some());
}
