use super::*;
use gpui::AppContext;
use taskmanager_platform_contract::CapabilityId;

#[test]
fn typed_submission_errors_preserve_provider_unavailable_presentation() {
    for submission in [
        SubmissionErrorKind::Busy,
        SubmissionErrorKind::RuntimeStopped,
        SubmissionErrorKind::InvalidRequest,
        SubmissionErrorKind::UnsupportedCapability,
    ] {
        assert_eq!(
            submission_failure_kind(SubmissionError {
                capability: CapabilityId::PROCESS_CONTROL,
                kind: submission,
            }),
            FailureKind::TemporarilyUnavailable
        );
    }
}

#[gpui::test]
fn accepted_control_feedback_replaces_local_toast_with_one_shell_notice(
    cx: &mut gpui::TestAppContext,
) {
    let entity = cx.new(|cx| RootView::new(taskmanager_theme::Theme::dark(), cx));
    let target = FrozenProcessIdentity::from_authoritative_parts(42, "worker", 10, 99)
        .expect("fixture identity is authoritative");
    let feedback = taskmanager_shell::ProcessControlFeedback {
        target,
        kind: taskmanager_shell::ProcessControlKind::EndTask,
        result: Ok(()),
    };

    entity.update(cx, |view, cx| {
        view.show_local_feedback("older local copy notice", cx);
        assert!(view.local_feedback_toast.is_some());
        view.accept_shared_process_control_feedback(&feedback);

        assert!(
            view.local_feedback_toast.is_none(),
            "the accepted control outcome must not remain in the renderer-local toast channel"
        );
        let notice = view
            .shell
            .feedback_notice()
            .expect("accepted control feedback publishes one typed notice");
        assert_eq!(notice.source(), taskmanager_shell::FeedbackSource::Control);
        assert_eq!(
            notice.severity(),
            taskmanager_shell::FeedbackSeverity::Success
        );
        assert!(notice.text().contains("42"));
    });
}
