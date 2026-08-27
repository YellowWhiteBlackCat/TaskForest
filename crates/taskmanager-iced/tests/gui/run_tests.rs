use super::*;

#[test]
fn demo_shape_is_available_without_a_platform_client() {
    // The demo path never spawns the native runtime; the launcher decides
    // before iced boots. This test pins the public surface the unified
    // binary compiles against.
    let app = IcedApp::demo();
    assert!(app.is_demo());
}

#[test]
fn iced_uses_its_own_desktop_identity() {
    assert_eq!(taskmanager_assets::product::ICED_NAME, "TaskForestI");
    #[cfg(target_os = "linux")]
    assert_eq!(
        platform_specific_settings().application_id,
        taskmanager_assets::product::ICED_APP_ID
    );
}
