use super::{
    ProcessInsightsErrorKind, ProcessInsightsLifecycle, process_insights_submission_error,
};
use taskmanager_application::{
    ProcessInsightFacetState, ProcessInsightUnavailable, ProcessInsightsProjection,
    ProcessInsightsRevision,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessLiveKey};
use taskmanager_platform_contract::SubmissionErrorKind;

fn target(pid: u32, start_token: u64) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, format!("process-{pid}"), 10, start_token)
        .expect("fixture identity is authoritative")
}

fn terminal_failure(
    target: FrozenProcessIdentity,
    revision: ProcessInsightsRevision,
) -> taskmanager_application::ProjectedProcessInsights {
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, revision);
    let mut projection = tracker.snapshot().expect("begin publishes a projection");
    let unavailable = ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
        FailureKind::PermissionDenied,
    ));
    projection.network = unavailable;
    projection.gpu = ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
        FailureKind::PermissionDenied,
    ));
    projection.resources = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    projection.isolation = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    projection.threads = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    projection.open_files = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    projection
}

#[test]
fn submission_failures_remain_typed_in_process_insights_state() {
    for kind in [
        SubmissionErrorKind::Busy,
        SubmissionErrorKind::RuntimeStopped,
        SubmissionErrorKind::InvalidRequest,
        SubmissionErrorKind::UnsupportedCapability,
    ] {
        assert_eq!(
            process_insights_submission_error(kind),
            ProcessInsightsErrorKind::WorkerDisconnected
        );
    }
}

#[test]
fn lifecycle_rejects_wrong_target_late_and_duplicate_terminals() {
    let first = target(42, 100);
    let reused_pid = target(42, 200);
    let first_revision = ProcessInsightsRevision::new(7);
    let next_revision = ProcessInsightsRevision::new(8);
    let first_terminal = terminal_failure(first.clone(), first_revision);
    let next_terminal = terminal_failure(reused_pid.clone(), next_revision);
    let mut lifecycle = ProcessInsightsLifecycle::default();

    lifecycle.begin(reused_pid.clone(), next_revision);
    assert!(
        !lifecycle.apply(first_terminal.clone()),
        "same PID with an older provider-native identity is not the active request"
    );
    assert!(matches!(
        lifecycle,
        ProcessInsightsLifecycle::Loading { ref request }
            if request.target == reused_pid && request.revision == next_revision
    ));

    assert!(lifecycle.apply(next_terminal.clone()));
    assert!(matches!(
        lifecycle,
        ProcessInsightsLifecycle::Failed { ref error, .. }
            if error.identity == ProcessLiveKey::from_parts(42, 200)
                && error.kind == ProcessInsightsErrorKind::PermissionDenied
    ));
    assert!(
        !lifecycle.apply(next_terminal),
        "a terminal phase cannot consume a duplicate completion"
    );
    assert!(
        !lifecycle.apply(first_terminal),
        "an older terminal cannot resurrect after the current request completed"
    );
}
