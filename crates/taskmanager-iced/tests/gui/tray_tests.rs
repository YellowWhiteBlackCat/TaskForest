use super::*;

#[test]
fn action_mapping_is_complete_and_rejects_unknown_ids() {
    assert_eq!(
        resolve_tray_action(TRAY_ACTION_SHOW),
        Some(TrayIntent::ShowWindow)
    );
    assert_eq!(
        resolve_tray_action(TRAY_ACTION_PAUSE),
        Some(TrayIntent::TogglePause)
    );
    assert_eq!(
        resolve_tray_action(TRAY_ACTION_QUIT),
        Some(TrayIntent::Quit)
    );
    assert_eq!(resolve_tray_action(99), None);
}

#[test]
fn iced_tray_spec_uses_its_own_product_identity_and_valid_icon() {
    let spec = build_tray_spec(false).expect("tray spec is valid");
    assert_eq!(spec.title(), Some(product::ICED_NAME));
    assert_eq!(spec.icon().width(), PRODUCT_TRAY_ICON_SIZE);
    assert_eq!(spec.icon().height(), PRODUCT_TRAY_ICON_SIZE);
    assert_eq!(
        spec.icon().pixels().len(),
        (PRODUCT_TRAY_ICON_SIZE * PRODUCT_TRAY_ICON_SIZE * 4) as usize
    );
    assert!(
        spec.icon()
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|rgba| rgba[3] != 0),
        "the shared branded tray icon must contain visible pixels"
    );
    assert_eq!(spec.menu().items().len(), 4);
}

#[test]
fn tray_pause_event_reuses_the_shell_pause_policy() {
    let mut app = IcedApp::default();
    let (sender, receiver) = channel();
    app.runtime.install_tray(None, Some(receiver));
    sender
        .send(TrayEvent::MenuActivated {
            id: TRAY_ACTION_PAUSE,
        })
        .expect("test tray receiver is live");

    assert!(!app.shell.paused());
    assert!(!drain_tray_events(&mut app));
    assert!(app.shell.paused());
}

#[test]
fn tray_quit_event_records_the_typed_reason() {
    let mut app = IcedApp::default();
    let (sender, receiver) = channel();
    app.runtime.install_tray(None, Some(receiver));
    sender
        .send(TrayEvent::MenuActivated {
            id: TRAY_ACTION_QUIT,
        })
        .expect("test tray receiver is live");

    assert!(!drain_tray_events(&mut app));
    assert!(app.shell.should_quit());
    assert_eq!(
        app.shell.quit_reason(),
        Some(taskmanager_shell::QuitReason::Tray)
    );
}
