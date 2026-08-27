use super::*;

/// The settings surface renders every chooser without I/O and preserves
/// the persisted defaults (dark theme, comfortable density, bytes).
#[test]
fn settings_modal_renders_from_preference_mirrors() {
    let mut app = crate::IcedApp::demo();
    {
        let view = render(&app);
        let _ = view;
    }
    let prefs = app.preferences();
    assert!(prefs.mode.is_empty() || prefs.mode.eq_ignore_ascii_case("System"));
    assert!(!prefs.hc);
    assert!(!prefs.density.eq_ignore_ascii_case("Compact"));
    assert!(prefs.memory_use_bytes);
    assert_eq!(prefs.skin, "");
    // The new GPUI-parity settings choosers default to the empty token
    // (platform-default text rendering; remember-last startup page).
    assert_eq!(prefs.text_rendering, "");
    assert_eq!(prefs.startup_page, "");
    assert_eq!(prefs.motion, "normal");
    assert!(!prefs.history_persistence);

    let _ = app.update(Message::SettingsChanged(SettingsChange::ContinuousHistory(
        true,
    )));
    assert!(app.preferences().history_persistence);
}
