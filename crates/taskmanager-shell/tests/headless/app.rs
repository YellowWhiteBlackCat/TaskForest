use super::*;
use taskmanager_core::core::alerts::NotificationPolicy;
use taskmanager_core::core::metrics::CpuMetrics;

impl ShellApp {
    pub(crate) fn apply_session_control_outcome(
        &mut self,
        outcome: SessionControlOutcome,
    ) -> Option<SessionControlOutcome> {
        let accepted = self.data.apply_session_control_outcome(outcome.clone());
        if let Some(outcome) = accepted.as_ref() {
            let (severity, lifecycle, text) = match &outcome.result {
                Ok(()) => (
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    format!(
                        "Session {} {:?} completed",
                        outcome.session_id, outcome.action
                    ),
                ),
                Err(error) => (
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!(
                        "Session {} {:?} failed: {error:?}",
                        outcome.session_id, outcome.action
                    ),
                ),
            };
            self.report_notice(FeedbackSource::Control, severity, lifecycle, text);
        }
        accepted
    }

    pub(crate) fn apply_startup_control_outcome(
        &mut self,
        outcome: StartupControlOutcome,
    ) -> Option<StartupControlOutcome> {
        let accepted = self.data.apply_startup_control_outcome(outcome.clone());
        if let Some(outcome) = accepted.as_ref() {
            let intent = if outcome.enabled { "enable" } else { "disable" };
            let (severity, lifecycle, text) = match &outcome.result {
                Ok(()) => (
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    format!("Startup {} {intent} completed", outcome.target_name),
                ),
                Err(error) => (
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!("Startup {} {intent} failed: {error:?}", outcome.target_name),
                ),
            };
            self.report_notice(FeedbackSource::Control, severity, lifecycle, text);
        }
        accepted
    }

    pub(crate) fn apply_batch_outcome(&mut self, result: ProcessBatchResult) -> ProcessBatchResult {
        let total = result.targets.len();
        let applied = result.applied_count();
        let action = result.intent.action;
        let complete = total > 0 && applied == total;
        let text = if complete {
            format!("Process {action:?} applied to {applied} target(s)")
        } else {
            format!("Process {action:?}: {applied}/{total} targets applied")
        };
        self.report_notice(
            FeedbackSource::Control,
            if complete {
                FeedbackSeverity::Success
            } else {
                FeedbackSeverity::Error
            },
            if complete {
                FeedbackLifecycle::SHORT
            } else {
                FeedbackLifecycle::UntilReplaced
            },
            text,
        );
        result
    }
}

#[path = "app/gate_confirmation_vocab.rs"]
mod gate_confirmation_vocab;
#[path = "app/lifecycle.rs"]
mod lifecycle;
#[path = "app/misc.rs"]
mod misc;
#[path = "app/on_demand_dispatch.rs"]
mod on_demand_dispatch;
#[path = "app/process_control.rs"]
mod process_control;
#[path = "app/request_sessions.rs"]
mod request_sessions;
#[path = "app/row_identity.rs"]
mod row_identity;
#[path = "app/selection_writes.rs"]
mod selection_writes;

/// Resolve one demo process's validated row identity by pid (fixtures carry
/// deterministic start tokens).
fn identity_of(app: &crate::ShellApp, pid: u32) -> ProcessLiveKey {
    app.projection()
        .processes_slice()
        .iter()
        .find(|process| process.pid == pid)
        .and_then(ProcessLiveKey::from_process)
        .expect("demo process carries a current start token")
}
#[path = "app/row_summary.rs"]
mod row_summary;
#[path = "app/search_paste.rs"]
mod search_paste;
#[allow(unused_imports)]
use super::search_input::SEARCH_QUERY_MAX;
#[path = "app/frame_state.rs"]
mod frame_state;
#[path = "app/gpu_engine_rows.rs"]
mod gpu_engine_rows;
#[path = "app/service_control.rs"]
mod service_control;
#[path = "app/service_log.rs"]
mod service_log;
#[path = "app/session_control.rs"]
mod session_control;
#[path = "app/smbios_rapl_sessions.rs"]
mod smbios_rapl_sessions;
#[path = "app/sort.rs"]
mod sort;
#[path = "app/source_status.rs"]
mod source_status;

use taskmanager_application::{
    CorrelatedDirectoryUsageEvent, CorrelatedEvent, DeviceLifecyclePartition, DirectoryUsageEvent,
    KeyCode, Modifiers, PlatformEventContext, SensorEvent, SessionControlOutcome,
    StartupControlOutcome,
};
use taskmanager_core::core::directory_usage::{
    DirectoryScanId, DirectoryScanStatus, DirectoryScanTotals, DirectoryUsageSnapshot,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::{DeviceId, ProviderId};
use taskmanager_core::core::metrics::{CpuScalarObservations, ScalarObservation};
use taskmanager_core::core::power::PowerSupplySnapshot;
use taskmanager_core::core::process::{
    ProcessBatchAction, ProcessBatchIntent, ProcessBatchResult, ProcessBatchTargetResult,
};
use taskmanager_core::core::sensors::SensorCenterSnapshot;
use taskmanager_core::core::session::SessionControlAction;
use taskmanager_platform_contract::{
    CapabilityId, DeviceDiscovery, DeviceSourceSnapshot, EventSequence, RequestId,
};

fn snapshot_with_cpu(cpu_usage: f32, timestamp_ms: u64) -> SystemSnapshot {
    SystemSnapshot {
        timestamp_ms,
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(cpu_usage, timestamp_ms),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A zero-duration CPU rule so the test fires on the first evaluation.
fn instant_cpu_rule() -> taskmanager_core::core::alerts::AlertRule {
    taskmanager_core::core::alerts::AlertRule::new(
        "cpu-high",
        taskmanager_core::core::alerts::AlertMetric::CpuUsagePercent,
        taskmanager_core::core::alerts::AlertSeverity::Warning,
        90.0,
        std::time::Duration::ZERO,
        0.0,
    )
}

#[test]
fn alert_center_evaluates_each_new_telemetry_tick_and_queues_notifications() {
    let mut app = crate::demo_app();
    app.data.alert_center.set_policy(NotificationPolicy {
        enabled: true,
        cooldown_ms: 0,
        ..NotificationPolicy::default()
    });
    app.data
        .alert_center
        .edit_rules(taskmanager_application::ManagedAlertRuleEdit::Import {
            rules: vec![taskmanager_application::ManagedAlertRule::new(
                instant_cpu_rule(),
                true,
            )],
            mode: taskmanager_application::AlertRuleImportMode::Replace,
        })
        .unwrap();
    app.data.snapshot = Some(snapshot_with_cpu(95.0, 100_000));
    app.data.last_recorded_snapshot_ms = 0;
    app.apply_platform_batch(PlatformEventBatch::default());
    assert_eq!(app.data.alert_active.len(), 1);
    assert_eq!(app.data.alert_active[0].instance_id, "cpu-high:system");
    let requests = app.drain_alert_notifications();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].instance_id, "cpu-high:system");

    // Same snapshot timestamp: no re-evaluation, no re-notification.
    app.apply_platform_batch(PlatformEventBatch::default());
    assert!(app.drain_alert_notifications().is_empty());

    // New tick clears the alert, then a later tick re-fires: one new request.
    app.data.snapshot = Some(snapshot_with_cpu(10.0, 200_000));
    app.apply_platform_batch(PlatformEventBatch::default());
    assert!(app.data.alert_active.is_empty());
    app.data.snapshot = Some(snapshot_with_cpu(95.0, 300_000));
    app.apply_platform_batch(PlatformEventBatch::default());
    assert_eq!(app.data.alert_active.len(), 1);
    assert_eq!(app.drain_alert_notifications().len(), 1);
}

#[test]
fn alert_notifications_respect_opt_out_policy() {
    let mut app = crate::demo_app();
    app.data
        .alert_center
        .edit_rules(taskmanager_application::ManagedAlertRuleEdit::Import {
            rules: vec![taskmanager_application::ManagedAlertRule::new(
                instant_cpu_rule(),
                true,
            )],
            mode: taskmanager_application::AlertRuleImportMode::Replace,
        })
        .unwrap();
    app.data.snapshot = Some(snapshot_with_cpu(95.0, 100_000));
    app.data.last_recorded_snapshot_ms = 0;
    app.apply_platform_batch(PlatformEventBatch::default());
    assert_eq!(
        app.data.alert_active.len(),
        1,
        "evaluation still runs when opted out"
    );
    assert!(
        app.drain_alert_notifications().is_empty(),
        "no desktop notification without explicit opt-in"
    );
}

#[test]
fn managed_rule_toggle_has_one_semantics_on_composed_and_direct_frontend_tracks() {
    let mut composed = crate::demo_app();
    let mut direct = DirectTrackState::default();
    let edit = taskmanager_application::ManagedAlertRuleEdit::Toggle {
        rule_id: "cpu-high".into(),
    };

    let composed_outcome = composed.edit_alert_rules(edit.clone()).unwrap();
    let direct_outcome = direct.edit_alert_rules(edit).unwrap();

    assert_eq!(composed_outcome, direct_outcome);
    assert_eq!(
        composed.projection().alert_center.managed_rules(),
        direct.projection().alert_center.managed_rules()
    );
    assert!(!composed.projection().alert_center.managed_rules()[0].enabled);
}

#[test]
fn alert_transition_history_is_identical_on_composed_and_direct_tracks() {
    let mut composed = crate::ShellApp::new();
    let mut direct = crate::DirectTrackState::default();
    let edit = taskmanager_application::ManagedAlertRuleEdit::Import {
        rules: vec![taskmanager_application::ManagedAlertRule::new(
            instant_cpu_rule(),
            true,
        )],
        mode: taskmanager_application::AlertRuleImportMode::Replace,
    };
    composed.edit_alert_rules(edit.clone()).unwrap();
    direct.edit_alert_rules(edit).unwrap();

    let high = snapshot_with_cpu(95.0, 100_000);
    let low = snapshot_with_cpu(10.0, 200_000);
    let _ = composed.evaluate_alerts(&high, 100_000);
    let _ = direct.evaluate_alerts(&high, 100_000);
    let _ = composed.evaluate_alerts(&low, 200_000);
    let _ = direct.evaluate_alerts(&low, 200_000);

    assert_eq!(
        composed.projection().alert_center.event_history(),
        direct.projection().alert_center.event_history(),
        "both shell tracks must consume the same transition authority"
    );
    assert_eq!(composed.projection().alert_center.event_history().len(), 2);

    composed.clear_alert_event_history();
    direct.clear_alert_event_history();
    assert!(
        composed
            .projection()
            .alert_center
            .event_history()
            .is_empty()
    );
    assert!(direct.projection().alert_center.event_history().is_empty());
}

#[test]
fn delete_uses_shared_reducer_and_never_executes_before_confirmation() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let effect = app.dispatch_key(ShellKeyEvent::new(KeyCode::Delete, Modifiers::NONE));
    assert_eq!(effect, InputDispatch::Consumed);
    assert!(app.pending_end().is_some());

    let confirmed = app.confirm_end_task();
    assert!(matches!(confirmed, Some(PlatformEffect::EndTask(_))));
    assert!(app.pending_end().is_none());
}

#[test]
fn legacy_process_row_cannot_open_or_confirm_a_dangerous_action() {
    use taskmanager_core::core::metrics::ScalarObservation;
    use taskmanager_core::core::process::ProcessScalarObservations;

    let mut app = ShellApp::new();
    // Direct construction (not the fixture builder): this row deliberately
    // carries NO provider start token — the legacy shape whose exact
    // identity is unprovable and therefore must never arm a dangerous
    // action (CORE-01 fail-closed rule).
    app.data.processes = Some(
        vec![
            taskmanager_core::core::process::ProcessItem::new(42, "legacy-worker")
                .with_scalar_observations(ProcessScalarObservations {
                    start_time_secs: ScalarObservation::available(7_500, 1),
                    ..ProcessScalarObservations::default()
                }),
        ]
        .into(),
    );
    app.application.active_page = AppPage::Applications;

    assert_eq!(app.selected_process_identity(), None);
    assert_eq!(
        app.dispatch_key(ShellKeyEvent::new(KeyCode::Delete, Modifiers::NONE)),
        InputDispatch::Unhandled
    );
    assert!(app.pending_end().is_none());
    assert_eq!(app.confirm_end_task(), None);
}

#[test]
fn session_action_freezes_the_selected_opaque_target_and_action() {
    let mut app = crate::demo_app();
    app.selected = 1;

    let effect = app
        .request_session_control(SessionControlAction::Lock)
        .expect("demo session selection should produce an action");
    let PlatformEffect::SessionControl(target) = effect else {
        panic!("session action must cross the typed platform-effect boundary");
    };

    assert_eq!(target.session_id.as_str(), "9");
    assert_eq!(target.action, SessionControlAction::Lock);
    assert_ne!(target.request_id.get(), 0);
}

#[test]
fn session_control_completion_accepts_only_the_latest_intent() {
    let mut app = crate::demo_app();
    let first = app
        .request_session_control(SessionControlAction::Lock)
        .expect("first session action");
    let second = app
        .request_session_control(SessionControlAction::Disconnect)
        .expect("second session action");
    let PlatformEffect::SessionControl(first) = first else {
        panic!("first action must be session control");
    };
    let PlatformEffect::SessionControl(second) = second else {
        panic!("second action must be session control");
    };

    app.apply_session_control_outcome(SessionControlOutcome {
        request_id: first.request_id,
        session_id: first.session_id,
        action: first.action,
        result: Ok(()),
    });
    assert!(app.feedback_text().contains("Demo snapshot"));

    app.apply_session_control_outcome(SessionControlOutcome {
        request_id: second.request_id,
        session_id: second.session_id,
        action: second.action,
        result: Err(FailureKind::PermissionDenied),
    });
    assert!(app.feedback_text().contains("failed"));
    assert!(app.feedback_text().contains("PermissionDenied"));
}

#[test]
fn startup_action_freezes_the_selected_entry_and_intent() {
    let mut app = crate::demo_app();
    app.selected = 1;

    // Every startup Enable/Disable is gated behind an explicit confirmation
    // (mirrors GPUI): request sets pending_startup and returns None.
    assert!(
        app.request_startup_control(false).is_none(),
        "request must gate behind pending_startup, not fire directly"
    );
    let pending = app
        .pending_startup()
        .expect("the pending slot is set by request_startup_control");
    assert!(!pending.enabled);
    let effect = app
        .confirm_startup_control()
        .expect("confirm emits the gated request");
    let PlatformEffect::StartupControl(request) = effect else {
        panic!("startup action must cross the typed platform-effect boundary");
    };
    assert!(app.pending_startup().is_none(), "confirm clears the gate");
    let expected_id = app
        .data
        .startup_entries
        .as_deref()
        .and_then(|entries| entries.get(1))
        .expect("demo startup entry at index 1")
        .id
        .clone();
    // The selected entry is frozen into the request (a later refresh cannot
    // retarget it), the intent is carried, and a fresh request id is allocated.
    assert_eq!(request.entry.id, expected_id);
    assert!(!request.enabled);
    assert_ne!(request.request_id.get(), 0);
}

#[test]
fn startup_control_completion_accepts_only_the_latest_intent() {
    let mut app = crate::demo_app();
    // Each request gates behind pending_startup; confirm emits it. Two
    // confirms produce two correlation ids so latest-wins can be exercised.
    let _ = app.request_startup_control(false);
    let first = app
        .confirm_startup_control()
        .expect("first confirm emits the gated request");
    let _ = app.request_startup_control(true);
    let second = app
        .confirm_startup_control()
        .expect("second confirm emits the gated request");
    let PlatformEffect::StartupControl(first) = first else {
        panic!("first action must be startup control");
    };
    let PlatformEffect::StartupControl(second) = second else {
        panic!("second action must be startup control");
    };

    // The superseded (first) intent's outcome is dropped — latest-wins, so the
    // status is unchanged.
    let before = app.feedback_text().to_owned();
    app.apply_startup_control_outcome(StartupControlOutcome {
        request_id: first.request_id,
        target_id: first.entry.id.clone(),
        target_name: first.entry.name.clone(),
        enabled: first.enabled,
        result: Ok(()),
    });
    assert_eq!(
        app.feedback_text(),
        before,
        "a superseded startup outcome must not land"
    );

    // The latest (second = enable) intent's outcome lands with its intent.
    app.apply_startup_control_outcome(StartupControlOutcome {
        request_id: second.request_id,
        target_id: second.entry.id.clone(),
        target_name: second.entry.name.clone(),
        enabled: second.enabled,
        result: Ok(()),
    });
    assert!(
        app.feedback_text().contains("enable completed"),
        "accepted enable outcome must surface in status: {}",
        app.feedback_text()
    );
}

#[test]
fn process_batch_freezes_the_selected_target_and_applies_outcome() {
    let mut app = crate::demo_app();
    app.selected = 1; // the demo's second process (gnome-shell, pid 1810)

    let effect = app
        .request_process_batch(ProcessBatchAction::Suspend)
        .expect("demo process selection should produce a batch intent");
    let PlatformEffect::ExecuteBatch(intent) = effect else {
        panic!("batch action must cross the typed platform-effect boundary");
    };
    // The selected process is frozen into the intent (action + one target).
    assert_eq!(intent.action, ProcessBatchAction::Suspend);
    assert_eq!(intent.targets.len(), 1);
    assert_eq!(intent.targets[0].pid, 1810);

    // A fully-applied outcome surfaces a success status carrying the action.
    let target = intent.targets[0].clone();
    app.apply_batch_outcome(ProcessBatchResult {
        intent: ProcessBatchIntent {
            action: ProcessBatchAction::Suspend,
            scope: Default::default(),
            targets: vec![target.clone()],
        },
        targets: vec![(target, ProcessBatchTargetResult::Applied)],
    });
    assert!(
        app.feedback_text().contains("applied to 1 target"),
        "applied batch outcome must surface in status: {}",
        app.feedback_text()
    );
}

#[test]
fn hold_ctrl_pauses_telemetry_via_the_shared_refresh_policy() {
    let mut app = crate::demo_app();
    assert!(!app.telemetry_refresh_policy.is_control_held());
    // Holding Ctrl pauses telemetry (mirrors GPUI); releasing resumes.
    app.set_control_held(true);
    assert!(app.telemetry_refresh_policy.is_control_held());
    assert!(
        app.telemetry_refresh_policy.is_paused(),
        "control-held must pause telemetry refresh so a frame can be inspected"
    );
    app.set_control_held(false);
    assert!(!app.telemetry_refresh_policy.is_paused());
    // Re-setting the same state is a no-op (no spurious status change).
    let before = app.feedback_text().to_owned();
    app.set_control_held(false);
    assert_eq!(app.feedback_text(), before);
}

#[test]
fn focus_search_switches_to_the_applications_page_and_opens_the_field() {
    use taskmanager_application::AppAction;
    let mut app = crate::demo_app();
    // Start elsewhere (Performance) so the search field is not visible.
    app.application.active_page = AppPage::Performance;
    assert!(!app.search_active());

    // Ctrl+F routes to FocusSearch; the shared reducer must switch to the
    // Applications page (where the field lives) AND open the search field —
    // mirrors GPUI and applies to every frontend.
    let _ = app.apply_action(AppAction::FocusSearch);
    assert_eq!(app.application.active_page, AppPage::Applications);
    assert!(app.search_active());
}

#[test]
fn pointer_selection_uses_the_active_projection_and_rejects_stale_rows() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;

    assert!(app.select_row(1));
    assert_eq!(app.selected, 1);
    assert_eq!(app.selected_process_identity().map(|id| id.pid), Some(1810));

    assert!(!app.select_row(999));
    assert_eq!(app.selected, 1);
    assert_eq!(app.selected_process_identity().map(|id| id.pid), Some(1810));
}

#[test]
fn home_and_end_jump_to_the_visible_list_bounds() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;

    // Capture the visible pid order so the test does not hardcode fixture pids.
    let pids: Vec<u32> = app.visible_processes().iter().map(|p| p.pid).collect();
    assert!(pids.len() >= 2, "demo process table must cover a jump");

    // Park the cursor mid-list, then End jumps to the last visible row.
    assert!(app.select_row(1));
    assert_eq!(
        app.selected_process_identity().map(|id| id.pid),
        Some(pids[1])
    );

    app.move_selection_to_last();
    assert_eq!(app.selected, pids.len() - 1);
    assert_eq!(
        app.selected_process_identity().map(|id| id.pid),
        Some(pids[pids.len() - 1])
    );

    // Home jumps back to the first visible row.
    app.move_selection_to_first();
    assert_eq!(app.selected, 0);
    assert_eq!(
        app.selected_process_identity().map(|id| id.pid),
        Some(pids[0])
    );

    // Both jumps collapse a wider multi-selection to the anchor row (bare
    // arrow semantics).
    assert!(app.select_row(0));
    assert!(app.toggle_row_selection(2));
    assert_eq!(app.selected_identities().len(), 2);
    app.move_selection_to_last();
    let anchor: std::collections::HashSet<ProcessLiveKey> =
        std::iter::once(identity_of(&app, pids[pids.len() - 1])).collect();
    assert_eq!(app.selected_identities(), &anchor);
}

#[test]
fn home_and_end_on_an_empty_list_reset_to_zero() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    // A query matching nothing empties the visible projection.
    app.query = "no-such-process-zzz".to_owned();
    assert_eq!(app.visible_processes().len(), 0);

    app.move_selection_to_first();
    assert_eq!(app.selected, 0);
    app.move_selection_to_last();
    assert_eq!(app.selected, 0);
}

#[test]
fn multi_select_toggle_extend_and_freeze_the_whole_set() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;

    // The demo ships ≥5 processes; capture the visible pid order so the test
    // does not hardcode fixture pids.
    let pids: Vec<u32> = app.visible_processes().iter().map(|p| p.pid).collect();
    assert!(
        pids.len() >= 5,
        "demo process table must cover a range select"
    );

    // Plain click collapses to a single anchor.
    assert!(app.select_row(0));
    let one: std::collections::HashSet<ProcessLiveKey> =
        std::iter::once(identity_of(&app, pids[0])).collect();
    assert_eq!(app.selected_identities(), &one);

    // Ctrl-click toggles a non-adjacent row into the set without losing the
    // anchor.
    assert!(app.toggle_row_selection(2));
    assert_eq!(app.selected, 2);
    let two: std::collections::HashSet<ProcessLiveKey> = [pids[0], pids[2]]
        .iter()
        .map(|&pid| identity_of(&app, pid))
        .collect();
    assert_eq!(app.selected_identities(), &two);

    // Shift-click grows a range from the anchor (2) to 4, folding in pids 2..=4.
    assert!(app.extend_row_selection(4));
    assert_eq!(app.selected, 4);
    let grown: std::collections::HashSet<ProcessLiveKey> = [pids[0], pids[2], pids[3], pids[4]]
        .iter()
        .map(|&pid| identity_of(&app, pid))
        .collect();
    assert_eq!(app.selected_identities(), &grown);

    // The batch intent freezes the entire multi-select set (4 targets), not
    // just the keyboard anchor. A four-row Suspend is reversible but not
    // local to the row the user is looking at, so the shared authority gates
    // it behind the same confirmation a Kill gets.
    assert!(app.selection_requires_batch_confirmation(ProcessBatchAction::Suspend));
    assert_eq!(app.request_process_batch(ProcessBatchAction::Suspend), None);
    assert!(app.pending_batch().is_some());
    let Some(PlatformEffect::ExecuteBatch(intent)) = app.confirm_process_batch() else {
        panic!("confirming the armed multi-select gate must emit the frozen batch");
    };
    assert_eq!(intent.action, ProcessBatchAction::Suspend);
    assert_eq!(intent.targets.len(), 4);
    let frozen: std::collections::HashSet<ProcessLiveKey> = intent
        .targets
        .iter()
        .map(|id| identity_of(&app, id.pid))
        .collect();
    assert_eq!(frozen, grown);

    // Toggling the anchor off drops exactly that pid.
    assert!(app.toggle_row_selection(4));
    assert!(
        !app.selected_identities()
            .contains(&identity_of(&app, pids[4]))
    );
    assert_eq!(app.selected_identities().len(), 3);
}

#[test]
fn selected_rows_range_spans_the_display_order_between_two_identities() {
    // The demo's visible order (sorted); the range must follow that order,
    // not the pid order.
    let app = crate::demo_app();
    let rows = app.visible_processes();
    let first = identity_of(&app, rows[0].pid);
    let third = identity_of(&app, rows[2].pid);

    // Forward range: anchor → end.
    let forward = selected_rows_range(&rows, first, third);
    assert_eq!(forward.len(), 3, "the range spans anchor..=end");
    assert_eq!(forward[0], first);
    assert_eq!(forward[2], third);

    // Reverse range: end before anchor spans the same rows.
    let reverse = selected_rows_range(&rows, third, first);
    assert_eq!(reverse.len(), 3);
    assert_eq!(reverse[0], first, "the range follows display order");
    assert_eq!(reverse[2], third);

    // A stale end identity degenerates to the single identity (never a panic).
    let stale_id = ProcessLiveKey::from_parts(u32::MAX, 1).expect("non-zero parts");
    let stale = selected_rows_range(&rows, first, stale_id);
    assert_eq!(stale, vec![stale_id]);
}

#[test]
fn toggle_selected_identity_flips_one_row_without_an_index() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let pids: Vec<u32> = app.visible_processes().iter().map(|p| p.pid).collect();

    // Toggle on, then off — the identity-level API flips the same way the
    // index-based path does, so a grouped/tree frontend can mark a row it
    // resolved to a live identity without touching the flat projection.
    app.toggle_selected_identity(identity_of(&app, pids[0]));
    assert!(
        app.selected_identities()
            .contains(&identity_of(&app, pids[0]))
    );
    app.toggle_selected_identity(identity_of(&app, pids[0]));
    assert!(
        !app.selected_identities()
            .contains(&identity_of(&app, pids[0]))
    );

    // Two distinct identities accumulate.
    app.toggle_selected_identity(identity_of(&app, pids[0]));
    app.toggle_selected_identity(identity_of(&app, pids[1]));
    assert_eq!(app.selected_identities().len(), 2);
}

#[test]
fn stale_selected_pids_are_pruned_when_the_process_list_refreshes() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let pids: Vec<u32> = app.visible_processes().iter().map(|p| p.pid).collect();

    assert!(app.select_row(0));
    assert!(app.toggle_row_selection(2));
    let reaped = identity_of(&app, pids[2]);
    assert!(app.selected_identities().contains(&reaped));

    // A refresh that drops pid[2] must prune it from the selection set. This
    // exercises the same `prune_stale_selection` call the process-snapshot
    // refresh path runs, without assembling a full `PlatformEventBatch`.
    let surviving: Vec<_> = app
        .projection()
        .processes_slice()
        .iter()
        .filter(|process| process.pid != pids[2])
        .cloned()
        .collect();
    app.data.processes = Some(surviving.into());
    app.prune_stale_selection();

    assert!(
        !app.selected_identities().contains(&reaped),
        "a reaped identity must not survive as a batch target"
    );
    // pids[0] survives and stays selected.
    assert!(
        app.selected_identities()
            .contains(&identity_of(&app, pids[0]))
    );
}

#[test]
fn table_row_count_follows_the_active_typed_projection() {
    let mut app = crate::demo_app();
    for (page, expected) in [
        (AppPage::Applications, 12),
        (AppPage::Services, 5),
        (AppPage::Startup, 2),
        (AppPage::Users, 2),
    ] {
        app.application.active_page = page;
        assert_eq!(app.table_row_count(), Some(expected));
    }

    app.application.active_page = AppPage::Performance;
    assert_eq!(app.table_row_count(), None);
    app.application.active_page = AppPage::System;
    assert_eq!(app.table_row_count(), None);
}

#[test]
fn shared_pause_action_toggles_only_the_local_telemetry_policy() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = app.apply_action(AppAction::FocusSearch);
    assert!(app.search_active());
    app.push_search_char('r');
    assert!(!app.visible_processes().is_empty());
    app.close_search();
    assert_eq!(app.apply_action(AppAction::TogglePause), None);
    assert!(app.paused());
    assert!(!app.telemetry_refresh_due(std::time::Duration::from_secs(60)));
    assert_eq!(app.apply_action(AppAction::TogglePause), None);
    assert!(!app.paused());
    assert!(app.telemetry_refresh_due(std::time::Duration::from_secs(1)));
}

#[test]
fn sensor_and_power_lifecycle_partitions_use_their_own_event_sequences() {
    let request_id = RequestId::new(2).expect("fixture request ID");
    let mut batch = PlatformEventBatch::default();
    batch.sensor_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id,
            capability: CapabilityId::SENSORS,
            provider: None,
            sequence: EventSequence::new(7),
            observed_at_ms: 70,
        },
        SensorEvent::Snapshot(DeviceSourceSnapshot::from_discovery(
            SensorCenterSnapshot::default(),
            ProviderId::borrowed("fixture.sensor.discovery"),
            DeviceDiscovery::Empty,
            Vec::new(),
        )),
    ));
    batch.power_supply_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id,
            capability: CapabilityId::POWER_SUPPLIES,
            provider: None,
            sequence: EventSequence::new(9),
            observed_at_ms: 90,
        },
        PowerSupplyEvent::Snapshot(DeviceSourceSnapshot::from_discovery(
            PowerSupplySnapshot::default(),
            ProviderId::borrowed("fixture.power.discovery"),
            DeviceDiscovery::Empty,
            Vec::new(),
        )),
    ));
    let mut app = ShellApp::new();

    app.apply_platform_batch(batch);

    assert_eq!(
        app.data
            .device_lifecycle_projection
            .accepted_revision_for(DeviceLifecyclePartition::Sensors),
        Some(DeviceLifecycleSnapshotRevision::new(7))
    );
    assert_eq!(
        app.data
            .device_lifecycle_projection
            .accepted_revision_for(DeviceLifecyclePartition::PowerSupplies),
        Some(DeviceLifecycleSnapshotRevision::new(9))
    );
    assert_eq!(app.data.device_lifecycle_diagnostics.len(), 2);
    // The power snapshot is also stored on ShellData for frontend rendering
    // (not just applied to device-lifecycle presence tracking).
    assert!(
        app.data.power_supplies.is_some(),
        "power snapshot must be stored for rendering"
    );
}

#[test]
fn help_overlay_toggle_closes_search_and_round_trips() {
    let mut app = crate::demo_app();
    app.open_search();
    assert!(!app.help_open());

    app.toggle_help();
    assert!(app.help_open());
    assert!(!app.search_active());

    app.toggle_help();
    assert!(!app.help_open());
}

#[test]
fn informational_overlays_are_mutually_exclusive_and_release_search() {
    let mut app = crate::demo_app();
    app.open_search();
    app.toggle_help();

    app.toggle_suggestions();
    assert!(app.suggestions_open());
    assert!(!app.help_open());
    assert!(!app.search_active());

    app.toggle_help();
    assert!(app.help_open());
    assert!(!app.suggestions_open());

    app.dismiss_overlay();
    assert!(!app.help_open());
    assert!(!app.suggestions_open());
}

#[test]
fn sensor_snapshot_is_stored_for_frontend_rendering() {
    let request_id = RequestId::new(3).expect("fixture request ID");
    let mut batch = PlatformEventBatch::default();
    batch.sensor_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id,
            capability: CapabilityId::SENSORS,
            provider: None,
            sequence: EventSequence::new(11),
            observed_at_ms: 110,
        },
        SensorEvent::Snapshot(DeviceSourceSnapshot::from_discovery(
            SensorCenterSnapshot::default(),
            ProviderId::borrowed("fixture.sensor.discovery"),
            DeviceDiscovery::Empty,
            Vec::new(),
        )),
    ));
    let mut app = ShellApp::new();

    app.apply_platform_batch(batch);

    assert!(
        app.data.sensors.is_some(),
        "sensor snapshot must be stored for frontend fan/temperature rendering"
    );
}

#[test]
fn process_insights_request_freezes_the_selected_identity() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    app.selected = 0;

    let effect = app
        .request_process_insights()
        .expect("selected process should produce an insight request");
    let PlatformEffect::ProcessInsights(identity) = effect else {
        panic!("insight request must cross the typed effect boundary");
    };
    assert_eq!(identity.pid, 4201);
    assert!(identity.authoritative_start_token().is_some());
}

#[test]
fn insight_projections_are_stored_last_wins_for_rendering() {
    use taskmanager_application::{
        ProcessInsightsProjection, ProcessInsightsRevision, ProjectedProcessInsights,
    };
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let target = app
        .selected_process_identity()
        .expect("demo selection has an identity");
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target.clone(), ProcessInsightsRevision::new(1));
    let first = tracker.snapshot().expect("projection exists after begin");
    tracker.begin(target.clone(), ProcessInsightsRevision::new(2));
    let second = tracker.snapshot().expect("projection exists after begin");

    let mut batch = PlatformEventBatch::default();
    batch
        .process_insight_projections
        .push(ProjectedProcessInsights::clone(&first));
    batch.process_insight_projections.push(second.clone());
    app.apply_platform_batch(batch);

    assert_eq!(app.data.process_insights, Some(second));
}

fn directory_usage_event(scan_id: u64, root: &str) -> CorrelatedDirectoryUsageEvent {
    CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(scan_id).expect("non-zero fixture request id"),
            capability: CapabilityId::DIRECTORY_USAGE,
            provider: None,
            sequence: EventSequence::new(scan_id),
            observed_at_ms: 10,
        },
        DirectoryUsageEvent::Update(DirectoryUsageSnapshot {
            scan_id: DirectoryScanId::new(scan_id),
            root: root.to_string(),
            status: DirectoryScanStatus::Scanning,
            entries: Vec::new(),
            totals: DirectoryScanTotals::fresh(10),
        }),
    )
}

/// The Disk-page directory-usage lane crosses the batch boundary as bounded
/// `Update` publications; the shell fold is latest-wins by `EventSequence` (the
/// snapshot's own root/scan_id identifies the mount it belongs to), and a
/// batch carrying no directory-usage events leaves the stored value untouched.
#[test]
fn directory_usage_snapshots_fold_latest_wins_by_domain_sequence() {
    let mut app = ShellApp::new();
    let mut batch = PlatformEventBatch::default();
    batch
        .directory_usage_events
        .push(directory_usage_event(5, "/home"));
    batch
        .directory_usage_events
        .push(directory_usage_event(4, "/mnt/data"));

    app.apply_platform_batch(batch);

    let stored = app
        .data
        .directory_usage
        .as_ref()
        .expect("the latest snapshot must be stored for frontend rendering");
    assert_eq!(stored.root, "/home", "the last Update event wins");
    assert_eq!(stored.scan_id, DirectoryScanId::new(5));

    app.apply_platform_batch(PlatformEventBatch::default());
    assert_eq!(
        app.data.directory_usage.as_ref().map(|s| s.root.as_str()),
        Some("/home"),
        "an empty-events batch must leave the prior snapshot untouched"
    );
}

/// The visible-row projection memo must be transparent: repeated calls
/// return identical rows, and a query / sort / snapshot change invalidates it
/// (a stale hit would freeze the table against the new inputs).
#[test]
fn visible_processes_memo_is_transparent_and_invalidates_correctly() {
    let mut app = crate::demo_app();
    let pids_of = |app: &crate::ShellApp| {
        app.visible_processes()
            .iter()
            .map(|p| p.pid)
            .collect::<Vec<_>>()
    };
    let baseline = pids_of(&app);
    assert!(!baseline.is_empty(), "demo fixture carries processes");

    // Cache hit: same rows, same order, no observable change.
    assert_eq!(pids_of(&app), baseline);

    // Query change invalidates: a query matching nothing yields no rows...
    app.query = "zzz-no-such-process".to_owned();
    assert!(app.visible_processes().is_empty());
    // ...and clearing the query restores the baseline projection.
    app.query.clear();
    assert_eq!(pids_of(&app), baseline);

    // Sort change invalidates: cycling the column re-orders the projection
    // (ties aside — the demo table is large enough for a distinct order).
    app.cycle_sort_column();
    let sorted = pids_of(&app);
    assert_eq!(sorted.len(), baseline.len());
    assert_ne!(sorted, baseline);

    // Snapshot change invalidates: replacing the table the way a platform
    // batch does (bump the refresh watermark) is reflected without touching
    // query or sort — and a same-key different-length direct swap is caught
    // by the length guard even without the bump.
    app.data.processes = Some(Vec::new().into());
    assert!(app.visible_processes().is_empty());
    let mut app2 = crate::demo_app();
    let _ = app2.visible_processes();
    let replacement = vec![app2.projection().processes_slice()[0].clone()];
    app2.data.processes = Some(replacement.into());
    assert_eq!(app2.visible_processes().len(), 1);
}

/// The Applications state bucket belongs to the same shell projection as the
/// shared query/sort. Changing it must reset the cursor and every row returned
/// to a frontend must satisfy the selected bucket.
#[test]
fn process_status_filter_rebuilds_the_shared_rows_and_resets_selection() {
    let mut app = crate::demo_app();
    let all_count = app.visible_processes().len();
    assert!(
        all_count > 1,
        "demo fixture needs more than one process state"
    );
    app.selected = all_count.saturating_sub(1);

    app.set_process_status_filter(crate::ProcessStatusFilter::Running);
    assert_eq!(app.selected, 0, "changing the bucket re-anchors selection");
    let running = app.visible_processes();
    assert!(
        !running.is_empty(),
        "demo fixture contains a running process"
    );
    assert!(
        running
            .iter()
            .all(|process| crate::ProcessStatusFilter::Running.matches(&process.status)),
        "the shared row projection must not leak another status"
    );
    assert!(
        running.len() < all_count,
        "the running bucket must actually filter"
    );

    app.set_process_status_filter(crate::ProcessStatusFilter::All);
    assert_eq!(app.visible_processes().len(), all_count);
}
