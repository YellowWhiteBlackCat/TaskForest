//! Behavior tests for GPU engines breakdown panel and SMART self-test control in Iced.

use crate::app::Message;
use taskmanager_core::core::SmartSelfTestKind;

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
    let _ = app.update(Message::RequestSmartSelfTest {
        index: 0,
        kind: SmartSelfTestKind::Short,
    });

    let pending = app
        .shell
        .pending_smart_self_test()
        .expect("self test armed");
    assert_eq!(pending.kind, SmartSelfTestKind::Short);
    assert_eq!(
        app.shell.confirmation_kind(),
        Some(taskmanager_application::ConfirmationKind::SmartSelfTest)
    );

    // Confirm the self-test
    let _ = app.update(Message::ConfirmSmartSelfTest);
    assert!(
        app.shell.pending_smart_self_test().is_none(),
        "confirmation cleared after confirm"
    );
}
