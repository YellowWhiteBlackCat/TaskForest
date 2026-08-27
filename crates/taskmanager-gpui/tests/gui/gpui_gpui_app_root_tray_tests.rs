use super::*;

#[test]
fn action_ids_map_to_the_expected_intents() {
    for (id, expected) in [
        (TRAY_ACTION_SHOW, TrayIntent::ShowWindow),
        (TRAY_ACTION_PAUSE, TrayIntent::TogglePause),
        (TRAY_ACTION_QUIT, TrayIntent::Quit),
    ] {
        assert_eq!(resolve_tray_action(id), Some(expected), "id {id}");
    }
    assert_eq!(resolve_tray_action(0), None);
    assert_eq!(resolve_tray_action(99), None);
}

#[test]
fn branded_icon_has_visible_pixels_and_the_expected_shape() {
    let icon = tray_icon_pixels().expect("embedded product bitmap is valid");
    assert_eq!(icon.width(), PRODUCT_TRAY_ICON_SIZE);
    assert_eq!(icon.height(), PRODUCT_TRAY_ICON_SIZE);
    assert_eq!(
        icon.pixels().len(),
        (PRODUCT_TRAY_ICON_SIZE * PRODUCT_TRAY_ICON_SIZE * 4) as usize
    );
    let pixel = |x: u32, y: u32| {
        let i = ((y * PRODUCT_TRAY_ICON_SIZE + x) * 4) as usize;
        icon.pixels()[i..i + 4].to_vec()
    };
    assert_eq!(pixel(0, 0)[3], 0, "round tray-icon corner is transparent");
    assert_eq!(
        pixel(PRODUCT_TRAY_ICON_SIZE / 3, PRODUCT_TRAY_ICON_SIZE * 2 / 3,)[3],
        255,
        "the performance window is fully opaque"
    );
    assert!(
        icon.pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|rgba| rgba[3] != 0),
        "branded tray icon must not be fully transparent"
    );
    assert!(
        pixel(PRODUCT_TRAY_ICON_SIZE / 2, PRODUCT_TRAY_ICON_SIZE / 2)[3] != 0,
        "the product mark must occupy the icon center"
    );
}

#[test]
fn tray_spec_carries_localized_labels_and_pause_state() {
    let spec = build_tray_spec(true).expect("spec builds from embedded catalogs");
    assert_eq!(spec.menu().node_count(), 4);
    assert_eq!(spec.title(), Some(taskmanager_assets::product::GPUI_NAME));
    assert!(!spec.show_menu_on_left_click());
    let paused_spec = build_tray_spec(false).expect("spec builds");
    assert!(paused_spec.menu().items().len() == 4);
}
