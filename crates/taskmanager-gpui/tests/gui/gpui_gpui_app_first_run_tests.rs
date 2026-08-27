use super::*;

#[test]
fn phase_and_failure_mapping_remain_typed() {
    let mut state = FirstRunUiState::default();
    assert_eq!(state.phase, FirstRunPhase::Hidden);
    state.phase = FirstRunPhase::Failed(FailureKind::PermissionDenied);
    assert_eq!(
        state.phase,
        FirstRunPhase::Failed(FailureKind::PermissionDenied)
    );
    assert_eq!(
        failure_key(FailureKind::MissingDependency),
        "first_run.failure_missing_dependency"
    );
}

#[test]
fn unsupported_without_setup_info_is_rendered_as_failure_not_discovery() {
    assert_eq!(
        empty_state_message_key(&FirstRunPhase::Discovering),
        "first_run.discovering"
    );
    assert_eq!(
        empty_state_message_key(&FirstRunPhase::Failed(FailureKind::Unsupported)),
        "first_run.failure_unsupported"
    );
}

#[test]
fn action_event_shape_is_not_confused_with_observation() {
    assert!(matches!(
        SetupScriptEvent::ActionCompleted {
            action: SetupScriptAction::Run
        },
        SetupScriptEvent::ActionCompleted {
            action: SetupScriptAction::Run
        }
    ));
}
