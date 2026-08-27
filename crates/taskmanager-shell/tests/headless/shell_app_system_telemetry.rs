use super::*;

#[test]
fn partial_acceptance_never_commits_a_render_frame() {
    let acceptance = ProjectionAcceptance::Accepted { snapshot: None };
    let result = SystemTelemetryApply::from_acceptance(&acceptance);

    assert_eq!(result, SystemTelemetryApply::AcceptedPartial);
    assert!(result.is_accepted());
    assert_eq!(result.frame_commit(), FrameCommit::Unchanged);
}

#[test]
fn complete_acceptance_commits_exactly_one_render_frame() {
    let acceptance = ProjectionAcceptance::Accepted {
        snapshot: Some(Box::new(SystemSnapshot::default())),
    };
    let result = SystemTelemetryApply::from_acceptance(&acceptance);

    assert_eq!(result, SystemTelemetryApply::Committed);
    assert!(result.is_accepted());
    assert_eq!(result.frame_commit(), FrameCommit::Committed);
}

#[test]
fn rejected_acceptance_does_not_commit_or_advance_the_frame() {
    let acceptance = ProjectionAcceptance::Rejected;
    let result = SystemTelemetryApply::from_acceptance(&acceptance);

    assert_eq!(result, SystemTelemetryApply::Rejected);
    assert!(!result.is_accepted());
    assert_eq!(result.frame_commit(), FrameCommit::Unchanged);
}
