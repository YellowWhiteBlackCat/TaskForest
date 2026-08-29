//! Behavior parity for the two frontend refresh-policy owners.
//!
//! RootView (GPUI) and ShellApp (tui/iced) intentionally retain separate
//! `TelemetryRefreshPolicy` instances because modifier state is window-local.
//! Drive both public owner surfaces through one timeline so their scheduling
//! semantics cannot drift.

use std::time::Duration;

use gpui::AppContext;
use taskmanager_application::{AppAction, TelemetryInterval, TelemetryRefreshPolicyChange};
use taskmanager_gpui::gpui_app::root::RootView;
use taskmanager_shell::ShellApp;
use taskmanager_theme::Theme;

/// Observable refresh-scheduling tuple both owners must agree on: combined
/// pause, transient Ctrl hold, cadence, and the due decision at three
/// representative elapsed values (unsubmitted / just under / at cadence).
fn gpui_policy_observables(policy: &taskmanager_application::TelemetryRefreshPolicy) -> PolicyView {
    PolicyView {
        paused: policy.is_paused(),
        control_held: policy.is_control_held(),
        interval_ms: policy.interval().duration().as_millis(),
        due_unsubmitted: policy.should_submit(None),
        due_half: policy.should_submit(Some(policy.interval().duration() / 2)),
        due_at_interval: policy.should_submit(Some(policy.interval().duration())),
    }
}

fn shell_policy_observables(app: &ShellApp) -> PolicyView {
    PolicyView {
        paused: app.paused(),
        control_held: app.control_held(),
        interval_ms: app.telemetry_interval().duration().as_millis(),
        // ShellApp has no public `should_submit(None)`; a never-submitted
        // scheduler is due immediately whenever it is not paused.
        due_unsubmitted: !app.paused(),
        due_half: app.telemetry_refresh_due(app.telemetry_interval().duration() / 2),
        due_at_interval: app.telemetry_refresh_due(app.telemetry_interval().duration()),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PolicyView {
    paused: bool,
    control_held: bool,
    interval_ms: u128,
    due_unsubmitted: bool,
    due_half: bool,
    due_at_interval: bool,
}

fn assert_policies_in_parity(
    view: &gpui::Entity<RootView>,
    shell: &ShellApp,
    cx: &mut gpui::TestAppContext,
    step: &str,
) {
    let gpui = view.update(cx, |view, _| {
        gpui_policy_observables(&view.telemetry_refresh_policy)
    });
    let shell_view = shell_policy_observables(shell);
    assert_eq!(
        gpui, shell_view,
        "GPUI-owned and shell-owned refresh policies diverged at step `{step}`"
    );
}

/// One modifier timeline, applied through each owner's real public mutation
/// surface: the shell goes through `set_control_held` / `apply_action` /
/// `set_telemetry_interval` (what tui/iced call), GPUI applies the exact
/// `TelemetryRefreshPolicyChange` values its keyboard and Settings paths emit
/// (`root/keyboard.rs` Ctrl+Space + modifiers, `settings_view/refresh.rs`).
#[gpui::test]
async fn gpui_and_shell_refresh_policies_track_one_modifier_timeline_identically(
    cx: &mut gpui::TestAppContext,
) {
    let view = cx.new(|cx| RootView::new(Theme::dark(), cx));
    let mut shell = ShellApp::new();

    // Baseline: default 1s cadence, unpaused.
    assert_policies_in_parity(&view, &shell, cx, "baseline");

    // Hold Ctrl: transient pause on both.
    shell.set_control_held(true);
    view.update(cx, |view, _| {
        view.telemetry_refresh_policy
            .apply(TelemetryRefreshPolicyChange::SetControlHeld(true));
    });
    assert_policies_in_parity(&view, &shell, cx, "ctrl held");

    // Release Ctrl: resume on both.
    shell.set_control_held(false);
    view.update(cx, |view, _| {
        view.telemetry_refresh_policy
            .apply(TelemetryRefreshPolicyChange::SetControlHeld(false));
    });
    assert_policies_in_parity(&view, &shell, cx, "ctrl released");

    // Manual pause (shell: shared TogglePause action; GPUI: the keyboard.rs
    // Ctrl+Space computation — flip the manual reason only).
    // TogglePause is a pure UI effect: the platform-effect slot it returns is
    // legitimately `None`.
    let _ = shell.apply_action(AppAction::TogglePause);
    view.update(cx, |view, _| {
        let paused = view.telemetry_refresh_policy.is_manually_paused();
        view.telemetry_refresh_policy
            .apply(TelemetryRefreshPolicyChange::SetPaused(!paused));
    });
    assert_policies_in_parity(&view, &shell, cx, "manual pause");

    // Cadence change clears the manual pause on both, kept Ctrl-independent.
    let interval = TelemetryInterval::clamped(Duration::from_millis(250));
    shell.set_telemetry_interval(interval);
    view.update(cx, |view, _| {
        view.telemetry_refresh_policy
            .apply(TelemetryRefreshPolicyChange::SetInterval(interval));
    });
    assert_policies_in_parity(&view, &shell, cx, "interval change clears manual pause");

    // A held Ctrl survives an interval change (transient reason is independent).
    shell.set_control_held(true);
    view.update(cx, |view, _| {
        view.telemetry_refresh_policy
            .apply(TelemetryRefreshPolicyChange::SetControlHeld(true));
    });
    let faster = TelemetryInterval::clamped(Duration::from_millis(500));
    shell.set_telemetry_interval(faster);
    view.update(cx, |view, _| {
        view.telemetry_refresh_policy
            .apply(TelemetryRefreshPolicyChange::SetInterval(faster));
    });
    assert_policies_in_parity(&view, &shell, cx, "ctrl hold survives interval change");
}
