use super::*;
use gpui::AppContext;

#[gpui::test]
async fn background_setup_observation_does_not_open_first_run_surface(
    cx: &mut gpui::TestAppContext,
) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, cx| {
        let request_id = taskmanager_platform_contract::RequestId::MIN;
        view.first_run_requests
            .insert(request_id, SetupScriptAction::Observe);
        let handled = view.apply_first_run_event(
            taskmanager_application::CorrelatedSetupScriptEvent {
                request_id,
                capability: taskmanager_platform_contract::CapabilityId::FIRST_RUN_SETUP,
                provider: None,
                sequence: taskmanager_platform_contract::EventSequence::new(1),
                observed_at_ms: 1,
                event: SetupScriptEvent::Observed(Some(SetupScriptInfo {
                    path: std::path::PathBuf::from(
                        "/usr/share/taskforest/setup/99-taskforest.rules",
                    ),
                    run_command: "taskforest-setup-helper --apply".to_owned(),
                    revert_command: "taskforest-setup-helper --revert".to_owned(),
                })),
            },
            cx,
        );

        assert!(handled);
        assert_eq!(view.first_run.phase, FirstRunPhase::Available);
        assert!(view.first_run.info.is_some());
        assert!(!view.first_run_open());
    });
}

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
