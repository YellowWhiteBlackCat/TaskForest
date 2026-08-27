//! test-intent: behavior
//! Cross-crate confirmation behavior through the application-owned typed
//! interaction state. No renderer projection fixture is mutated here.

use gpui::TestAppContext;
use taskmanager::core::process::{FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent};
use taskmanager::core::services::ServiceAction;
use taskmanager::core::{DeviceGeneration, DeviceId, ServiceId, SmartSelfTestKind};
use taskmanager_application::{
    InteractionEvent, PendingConfirmation, ProcessTerminationConfirmation,
};
use taskmanager_gpui::gpui_app::root::{ProcessTerminationAction, RootView};
use taskmanager_gpui::gpui_app::system_health_view::SmartSelfTestConfirmationRequest;
use taskmanager_gpui::gpui_app::theme::Theme;

use super::proc;

fn termination_intent(action: ProcessTerminationAction) -> ProcessTerminationConfirmation {
    let root = FrozenProcessIdentity::from_process(&proc(4242, "important-worker"))
        .expect("authoritative process fixture");
    ProcessTerminationConfirmation {
        action,
        root,
        descendants_leaf_first: Vec::new(),
    }
}

#[gpui::test]
async fn process_termination_cancel_is_inert_and_confirm_emits_the_frozen_effect(
    cx: &mut TestAppContext,
) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        let intent = termination_intent(ProcessTerminationAction::EndTask);
        let _ = view
            .shell
            .interaction
            .reduce(InteractionEvent::ArmConfirmation(
                PendingConfirmation::ProcessTermination(intent.clone()),
            ));
        assert_eq!(view.process_termination_confirmation(), Some(&intent));

        view.cancel_process_termination();
        assert!(view.process_termination_confirmation().is_none());
        assert!(view.shell.feedback_notice().is_none());

        let _ = view
            .shell
            .interaction
            .reduce(InteractionEvent::ArmConfirmation(
                PendingConfirmation::ProcessTermination(intent),
            ));
        let mut submitted = None;
        assert!(view.confirm_process_termination_with(
            |effect| {
                submitted = Some(effect);
                Ok(())
            },
            cx,
        ));
        assert!(matches!(
            submitted,
            Some(taskmanager_application::PlatformEffect::EndTask(target)) if target.pid == 4242
        ));
        assert!(view.process_termination_confirmation().is_none());
    })
    .unwrap();
}

#[gpui::test]
async fn shared_and_renderer_local_surfaces_replace_without_parallel_owners(
    cx: &mut TestAppContext,
) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let service_id = ServiceId::new("authority.service");
    win.update(cx, |view, _window, _cx| {
        view.request_service_control_confirmation(service_id.clone(), ServiceAction::Stop);
        assert!(view.service_control_confirmation().is_some());

        view.show_settings();
        assert!(view.service_control_confirmation().is_none());
        assert!(view.settings_open());

        view.request_service_control_confirmation(service_id, ServiceAction::Restart);
        assert!(!view.settings_open());
        assert!(view.service_control_confirmation().is_some());
    })
    .unwrap();
}

#[gpui::test]
async fn service_control_cancel_is_inert_and_confirm_is_the_only_submit_path(
    cx: &mut TestAppContext,
) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let service_id = ServiceId::new("NetworkManager.service");
    win.update(cx, |view, _window, cx| {
        view.request_service_control_confirmation(service_id.clone(), ServiceAction::Stop);
        let pending = view
            .service_control_confirmation()
            .expect("gated service action");
        assert_eq!(pending.service_id, service_id);
        assert_eq!(pending.action, ServiceAction::Stop);
        assert!(view.services_feedback().is_none());

        view.cancel_service_control_confirmation();
        assert!(view.service_control_confirmation().is_none());
        assert!(view.services_feedback().is_none());

        view.request_service_control_confirmation(service_id, ServiceAction::Stop);
        assert!(view.confirm_service_control_confirmation(cx));
        assert!(view.service_control_confirmation().is_none());
        assert!(view.services_feedback().is_some());
    })
    .unwrap();
}

#[gpui::test]
async fn smart_self_test_uses_the_shared_gate_before_submission(cx: &mut TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        view.request_system_health_self_test_confirmation(SmartSelfTestConfirmationRequest {
            device_id: DeviceId::new("disk:smart-fixture"),
            device_generation: DeviceGeneration::new(7),
            disk_name: "nvme-fixture".into(),
            disk_label: "SMART fixture".into(),
            kind: SmartSelfTestKind::Short,
        });
        let pending = view
            .system_health_confirmation()
            .cloned()
            .expect("SMART request must be frozen");
        assert_eq!(pending.device_id, DeviceId::new("disk:smart-fixture"));
        assert_eq!(pending.device_generation, DeviceGeneration::new(7));

        assert!(
            !view.confirm_system_health_self_test(cx),
            "a headless view without a platform worker must fail honestly"
        );
        assert_eq!(
            view.system_health_confirmation(),
            Some(&pending),
            "submission rejection must re-arm the immutable intent for explicit retry"
        );
    })
    .unwrap();
}

#[gpui::test]
async fn completed_process_batch_history_exports_to_clipboard(cx: &mut TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let identity =
        FrozenProcessIdentity::from_authoritative_parts(4242, "audited-worker", 99, 9_900)
            .expect("fixture identity");

    win.update(cx, |view, _window, cx| {
        view.process_batch_history.record_result(
            123_456,
            taskmanager_application::ProcessBatchResult {
                intent: ProcessBatchIntent {
                    action: ProcessBatchAction::Suspend,
                    targets: vec![identity.clone()],
                    scope: Default::default(),
                },
                targets: vec![(
                    identity,
                    taskmanager_application::ProcessBatchTargetResult::Applied,
                )],
            },
        );
        view.copy_process_batch_history(cx);
    })
    .unwrap();

    let payload = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .expect("batch history clipboard payload");
    assert!(payload.contains("\"schema_version\": 1"));
    assert!(payload.contains("\"name\": \"audited-worker\""));
}
