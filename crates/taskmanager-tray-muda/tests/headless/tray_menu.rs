use super::*;

#[test]
fn menu_ids_round_trip_for_ours_only() {
    assert_eq!(decode_menu_id(&menu_id_for(42)), Some(42));
    assert_eq!(decode_menu_id(&menu_id_for(0)), Some(0));
    assert_eq!(decode_menu_id("other-app:7"), None);
    assert_eq!(decode_menu_id("taskmanager:not-a-number"), None);
    assert_eq!(decode_menu_id("taskmanager:"), None);
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
#[test]
fn build_menu_preserves_structure_and_check_states() {
    let spec = TrayMenuSpec::from_items(vec![
        TrayMenuItem::Separator,
        TrayMenuItem::Action {
            id: 1,
            label: "Show".into(),
            enabled: true,
        },
        TrayMenuItem::Checkmark {
            id: 2,
            label: "Pause".into(),
            checked: true,
            enabled: true,
        },
        TrayMenuItem::Radio {
            id: 3,
            label: "a".into(),
            checked: true,
            enabled: true,
            radio_group: Some(9),
        },
        TrayMenuItem::Radio {
            id: 4,
            label: "b".into(),
            checked: false,
            enabled: true,
            radio_group: Some(9),
        },
        TrayMenuItem::Submenu {
            label: "More".into(),
            enabled: true,
            items: vec![TrayMenuItem::Action {
                id: 5,
                label: "Deep".into(),
                enabled: true,
            }],
        },
    ])
    .unwrap();
    let built = build_menu(&spec).expect("menu builds on a tray-capable target");
    assert_eq!(built.menu.items().len(), 6);
    assert!(built.radio.is_checked(2));
    assert!(built.radio.is_checked(3));
    assert!(!built.radio.is_checked(4));
    assert!(!built.radio.is_checked(1));
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
#[test]
fn radio_state_enforces_one_selected_per_group() {
    let spec = TrayMenuSpec::from_items(vec![
        TrayMenuItem::Radio {
            id: 1,
            label: "a".into(),
            checked: true,
            enabled: true,
            radio_group: Some(9),
        },
        TrayMenuItem::Radio {
            id: 2,
            label: "b".into(),
            checked: false,
            enabled: true,
            radio_group: Some(9),
        },
    ])
    .unwrap();
    let built = build_menu(&spec).expect("menu builds on a tray-capable target");
    built.radio.set_checked(2, true);
    assert!(built.radio.is_checked(2));
    assert!(!built.radio.is_checked(1));
    built.radio.set_checked(1, true);
    assert!(built.radio.is_checked(1));
    assert!(!built.radio.is_checked(2));
    built.radio.set_checked(1, false);
    assert!(!built.radio.is_checked(1));
    built.radio.set_checked(999, true);
    assert!(!built.radio.is_checked(999));
}
