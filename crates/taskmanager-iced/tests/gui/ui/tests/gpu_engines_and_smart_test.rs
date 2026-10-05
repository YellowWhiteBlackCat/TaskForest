//! Behavior tests for GPU engines breakdown panel and SMART self-test control in Iced.

use crate::app::Message;
use taskmanager_application::ConfirmationKind;
use taskmanager_core::core::SmartSelfTestKind;
use taskmanager_shell::presentation::smart::self_test_intent;

#[test]
fn gpu_engines_expanded_toggle_and_panel_rendering() {
    let mut app = crate::IcedApp::demo();
    assert!(
        app.performance.gpu_engines_expanded,
        "engines panel starts expanded"
    );

    let _ = app.update(Message::ToggleGpuEnginesExpanded);
    assert!(
        !app.performance.gpu_engines_expanded,
        "toggles to collapsed"
    );

    let _ = app.update(Message::ToggleGpuEnginesExpanded);
    assert!(
        app.performance.gpu_engines_expanded,
        "toggles back to expanded"
    );

    let snapshot = app
        .shell
        .projection()
        .snapshot
        .clone()
        .expect("demo snapshot");
    let gpu = snapshot.gpu.first().expect("demo gpu");
    let theme_snapshot = app.theme();
    let engine_rows = crate::ui::perf_devices::gpu::engine_rows_presentation(&app, gpu);

    let panel =
        crate::ui::perf_devices::gpu::gpu_engines_panel(&app, gpu, &engine_rows, theme_snapshot);
    assert!(
        panel.is_some(),
        "GPU engines panel renders when engines exist"
    );
}

#[test]
fn smart_self_test_control_request_and_confirm_flow() {
    let mut app = crate::IcedApp::demo();
    assert!(
        app.shell.pending_smart_self_test().is_none(),
        "starts with no pending test"
    );

    // Request short self-test on disk 0
    let intent = self_test_intent(
        &app.shell
            .projection()
            .snapshot
            .as_ref()
            .expect("snapshot")
            .disks[0],
        SmartSelfTestKind::Short,
    );
    let _ = app.update(Message::RequestSmartSelfTest(intent));

    let pending = app
        .shell
        .pending_smart_self_test()
        .expect("self test armed");
    assert_eq!(pending.kind, SmartSelfTestKind::Short);
    assert_eq!(
        app.shell.confirmation_kind(),
        Some(ConfirmationKind::SmartSelfTest)
    );

    // Confirm the self-test
    let _ = app.update(Message::ConfirmSmartSelfTest);
    assert!(
        app.shell.pending_smart_self_test().is_none(),
        "confirmation cleared after confirm"
    );
}

#[test]
fn a_painted_smart_control_cannot_retarget_a_replaced_disk() {
    use taskmanager_core::core::identity::DeviceGeneration;
    use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
    let mut app = crate::IcedApp::demo();
    let mut snapshot = app.shell.projection().snapshot.clone().expect("snapshot");
    let intent = self_test_intent(&snapshot.disks[0], SmartSelfTestKind::Short);
    snapshot.disks[0].device_generation =
        DeviceGeneration::new(snapshot.disks[0].device_generation.get() + 1);
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    let _ = app.update(Message::RequestSmartSelfTest(intent));
    assert!(app.shell.pending_smart_self_test().is_none());
    assert!(app.shell.confirmation_kind().is_none());
}
