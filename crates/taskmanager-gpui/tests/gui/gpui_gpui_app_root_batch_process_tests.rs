use std::cell::RefCell;

use super::{
    export_process_batch_history_with, process_batch_failure_feedback_key,
    record_completed_process_batch, result_counts,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchHistory, ProcessBatchHistoryFormat,
    ProcessBatchIntent, ProcessBatchResult, ProcessBatchTargetResult,
};

#[gpui::test]
fn application_root_batch_freezes_the_exact_tree_without_a_representative_pid(
    cx: &mut gpui::TestAppContext,
) {
    use crate::gpui_app::root::RootView;
    use gpui::AppContext;
    use taskmanager_core::core::ScalarObservation;
    use taskmanager_core::core::process::ProcessScalarObservations;
    use taskmanager_theme::Theme;

    let process = |pid, parent_pid, token| {
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(pid)
            .parent_pid(parent_pid)
            .name(format!("worker-{pid}"))
            .scalar_observations(ProcessScalarObservations {
                start_token: ScalarObservation::available(token, 1),
                ..ProcessScalarObservations::default()
            })
            .current_start_time_secs(token)
            .build()
    };
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, _cx| {
        view.replace_processes_for_test(vec![
            process(10, None, 100),
            process(11, Some(10), 110),
            process(12, Some(11), 120),
        ]);
        view.select_application_root(10);
        assert_eq!(view.selected_pid(), None);
        view.request_process_batch(ProcessBatchAction::End);
    });

    let pids = root.read_with(cx, |view, _cx| {
        view.process_batch_confirmation()
            .expect("application selection must create a batch confirmation")
            .targets
            .iter()
            .map(|target| target.pid)
            .collect::<Vec<_>>()
    });
    assert_eq!(pids, vec![12, 11, 10]);
}

#[test]
fn clipboard_adapter_receives_the_exact_deterministic_payload() {
    let history = ProcessBatchHistory::default();
    let written = RefCell::new(None);
    let byte_count =
        export_process_batch_history_with(&history, ProcessBatchHistoryFormat::Json, |payload| {
            *written.borrow_mut() = Some(payload)
        })
        .expect("empty audit history must serialize");

    let expected = "{\n  \"schema_version\": 1,\n  \"entries\": []\n}\n";
    assert_eq!(written.into_inner().as_deref(), Some(expected));
    assert_eq!(byte_count, expected.len());
}

#[test]
fn completed_result_is_consumed_once_into_history() {
    let identity =
        taskmanager_core::core::process::FrozenProcessIdentity::from_authoritative_parts(
            42, "worker", 123, 1_230,
        )
        .expect("fixture identity");
    let result = ProcessBatchResult {
        intent: ProcessBatchIntent {
            action: ProcessBatchAction::End,
            scope: Default::default(),
            targets: vec![identity.clone()],
        },
        targets: vec![(
            identity,
            ProcessBatchTargetResult::Failed(FailureKind::PermissionDenied),
        )],
    };
    let mut history = ProcessBatchHistory::new(2);

    let summary = record_completed_process_batch(&mut history, 789, result);

    assert_eq!(history.len(), 1);
    assert_eq!(history.entries()[0].completed_at_unix_ms, 789);
    assert!(summary.ends_with(taskmanager_application::i18n::t(
        "feedback.permission_denied"
    )));
    assert!(matches!(
        history.entries()[0].targets[0].result,
        ProcessBatchTargetResult::Failed(FailureKind::PermissionDenied)
    ));
}

#[test]
fn typed_failures_drive_actionable_batch_feedback() {
    let cases = [
        (FailureKind::PermissionDenied, "feedback.permission_denied"),
        (FailureKind::IdentityChanged, "feedback.process_gone"),
        (FailureKind::Unsupported, "feedback.unsupported"),
        (
            FailureKind::MissingDependency,
            "health.failure_provider_unavailable",
        ),
        (
            FailureKind::TemporarilyUnavailable,
            "health.failure_provider_unavailable",
        ),
        (FailureKind::TimedOut, "feedback.timed_out"),
        (FailureKind::Rejected, "feedback.request_rejected"),
        (FailureKind::ProviderFault, "feedback.provider_failed"),
    ];

    for (failure, key) in cases {
        assert_eq!(process_batch_failure_feedback_key(failure), key);
    }

    let gone = FrozenProcessIdentity::from_authoritative_parts(7, "gone", 70, 700)
        .expect("fixture identity");
    let blocked = FrozenProcessIdentity::from_authoritative_parts(8, "blocked", 80, 800)
        .expect("fixture identity");
    let result = ProcessBatchResult {
        intent: ProcessBatchIntent {
            action: ProcessBatchAction::End,
            scope: Default::default(),
            targets: vec![gone.clone(), blocked.clone()],
        },
        targets: vec![
            (
                gone,
                ProcessBatchTargetResult::Failed(FailureKind::IdentityChanged),
            ),
            (
                blocked,
                ProcessBatchTargetResult::Failed(FailureKind::PermissionDenied),
            ),
        ],
    };

    assert_eq!(result_counts(&result), (0, 1, 1));
}
