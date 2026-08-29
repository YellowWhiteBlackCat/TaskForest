use taskmanager_application::{
    ConfirmationKind, InteractionEvent, InteractionState, PendingConfirmation, PlatformEffect,
    ProcessTerminationAction, ProcessTerminationConfirmation, SmartControlRequest,
    SurfaceDismissReason, SurfaceKind, SurfaceTransition,
};
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId};
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessGroupScope,
};
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_core::core::system_health::SmartSelfTestIntent;

fn frozen(pid: u32, name: &str) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, name, 10, u64::from(pid) + 100)
        .expect("valid frozen process fixture")
}

#[test]
fn opening_a_new_primary_surface_atomically_replaces_the_previous_branch() {
    let mut state = InteractionState::default();
    let properties = state.reduce(InteractionEvent::OpenProcessProperties(frozen(
        6, "details",
    )));
    assert_eq!(
        properties.transition,
        SurfaceTransition::Opened(SurfaceKind::ProcessProperties)
    );

    let opened = state.reduce(InteractionEvent::ArmConfirmation(
        PendingConfirmation::EndTask(frozen(7, "first")),
    ));
    assert_eq!(
        opened.transition,
        SurfaceTransition::Replaced {
            previous: SurfaceKind::ProcessProperties,
            current: SurfaceKind::Confirmation(ConfirmationKind::EndTask),
        }
    );

    let batch = ProcessBatchIntent {
        action: ProcessBatchAction::Kill,
        scope: ProcessGroupScope::PidAdjacency,
        targets: vec![frozen(8, "second")],
    };
    let replaced = state.reduce(InteractionEvent::ArmConfirmation(
        PendingConfirmation::ProcessBatch(batch.clone()),
    ));
    assert_eq!(
        replaced.transition,
        SurfaceTransition::Replaced {
            previous: SurfaceKind::Confirmation(ConfirmationKind::EndTask),
            current: SurfaceKind::Confirmation(ConfirmationKind::ProcessBatch),
        }
    );
    assert_eq!(
        state.pending_confirmation(),
        Some(&PendingConfirmation::ProcessBatch(batch))
    );
}

#[test]
fn only_a_branch_matched_confirm_emits_the_exact_frozen_effect() {
    let mut state = InteractionState::default();
    let target = frozen(19, "target");
    let armed = state.reduce(InteractionEvent::ArmConfirmation(
        PendingConfirmation::EndTask(target.clone()),
    ));
    assert!(armed.effect.is_none());

    let stale = state.reduce(InteractionEvent::Confirm(ConfirmationKind::ServiceControl));
    assert_eq!(stale.transition, SurfaceTransition::Unchanged);
    assert!(stale.effect.is_none());
    assert_eq!(
        state.pending_confirmation(),
        Some(&PendingConfirmation::EndTask(target.clone()))
    );

    let confirmed = state.reduce(InteractionEvent::Confirm(ConfirmationKind::EndTask));
    assert_eq!(
        confirmed.transition,
        SurfaceTransition::Confirmed(ConfirmationKind::EndTask)
    );
    assert_eq!(confirmed.effect, Some(PlatformEffect::EndTask(target)));
    assert!(!state.is_open());

    let repeated = state.reduce(InteractionEvent::Confirm(ConfirmationKind::EndTask));
    assert_eq!(repeated.transition, SurfaceTransition::Unchanged);
    assert!(repeated.effect.is_none());
}

#[test]
fn dismiss_clears_the_surface_without_platform_work_and_records_the_reason() {
    let mut state = InteractionState::default();
    let _ = state.reduce(InteractionEvent::ArmConfirmation(
        PendingConfirmation::EndTask(frozen(23, "dismissed")),
    ));
    let dismissed = state.reduce(InteractionEvent::Dismiss(SurfaceDismissReason::PageChanged));
    assert_eq!(
        dismissed.transition,
        SurfaceTransition::Dismissed {
            surface: SurfaceKind::Confirmation(ConfirmationKind::EndTask),
            reason: SurfaceDismissReason::PageChanged,
        }
    );
    assert!(dismissed.effect.is_none());
    assert!(!state.is_open());
}

#[test]
fn smart_self_test_is_frozen_and_only_confirm_converts_it_to_platform_work() {
    let intent = SmartSelfTestIntent {
        device_id: DeviceId::new("disk:fixture"),
        device_generation: DeviceGeneration::new(4),
        device_key: "nvme0n1".into(),
        display_name: String::from("Fixture disk"),
        kind: SmartSelfTestKind::Short,
    };
    let mut state = InteractionState::default();
    let armed = state.reduce(InteractionEvent::ArmConfirmation(
        PendingConfirmation::SmartSelfTest(intent.clone()),
    ));
    assert_eq!(
        armed.transition,
        SurfaceTransition::Opened(SurfaceKind::Confirmation(ConfirmationKind::SmartSelfTest))
    );
    assert!(armed.effect.is_none());

    let confirmed = state.reduce(InteractionEvent::Confirm(ConfirmationKind::SmartSelfTest));
    assert_eq!(
        confirmed.effect,
        Some(PlatformEffect::SmartControl(
            SmartControlRequest::StartSelfTest(intent)
        ))
    );
    assert!(!state.is_open());
}

#[test]
fn process_termination_preserves_leaf_first_scope_in_the_confirmed_effect() {
    let root = frozen(40, "root");
    let child = frozen(41, "child");
    let intent = ProcessTerminationConfirmation {
        action: ProcessTerminationAction::ForceKill,
        root: root.clone(),
        descendants_leaf_first: vec![child.clone()],
    };
    let mut state = InteractionState::default();
    let armed = state.reduce(InteractionEvent::ArmConfirmation(
        PendingConfirmation::ProcessTermination(intent),
    ));
    assert!(armed.effect.is_none());

    let confirmed = state.reduce(InteractionEvent::Confirm(
        ConfirmationKind::ProcessTermination,
    ));
    assert_eq!(
        confirmed.effect,
        Some(PlatformEffect::ExecuteBatch(ProcessBatchIntent {
            action: ProcessBatchAction::Kill,
            scope: ProcessGroupScope::PidAdjacency,
            targets: vec![child, root],
        }))
    );
}
