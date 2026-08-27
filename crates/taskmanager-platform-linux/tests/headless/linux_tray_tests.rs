use super::*;
use ksni::menu::MenuItem;

fn spec_with_menu(items: Vec<TrayMenuItem>) -> TraySpec {
    TraySpec::new(
        TrayIconData::from_rgba(vec![0u8; 4 * 4 * 4], 4, 4).unwrap(),
        Some("tip".into()),
        Some("TaskForest".into()),
        TrayMenuSpec::from_items(items).unwrap(),
        false,
    )
    .unwrap()
}

#[test]
fn rgba_icon_converts_to_argb_network_order() {
    let icon = TrayIconData::from_rgba(vec![1, 2, 3, 4, 5, 6, 7, 8], 2, 1).unwrap();
    let ksni = to_ksni_icon(&icon);
    assert_eq!(ksni.width, 2);
    assert_eq!(ksni.height, 1);
    assert_eq!(ksni.data, vec![4, 1, 2, 3, 8, 5, 6, 7]);
}

#[test]
fn action_item_carries_label_and_emits_activation() {
    let (tx, rx) = std::sync::mpsc::channel();
    let spec = spec_with_menu(vec![TrayMenuItem::Action {
        id: 11,
        label: "Show".into(),
        enabled: true,
    }]);
    let state = MenuState::from_spec(spec.menu());
    let mapped = map_items(spec.menu().items(), &state, &tx);
    let MenuItem::Standard(item) = &mapped[0] else {
        panic!("expected standard item");
    };
    assert_eq!(item.label, "Show");
    assert!(item.enabled);
    let mut tray = KsniTray {
        spec,
        tooltip: None,
        title: None,
        state,
        events: tx,
    };
    (item.activate)(&mut tray);
    assert_eq!(rx.try_recv(), Ok(TrayEvent::MenuActivated { id: 11 }));
}

#[test]
fn checkmark_reflects_state_and_toggles_before_emitting() {
    let (tx, rx) = std::sync::mpsc::channel();
    let spec = spec_with_menu(vec![TrayMenuItem::Checkmark {
        id: 5,
        label: "Pause".into(),
        checked: false,
        enabled: true,
    }]);
    let state = MenuState::from_spec(spec.menu());
    let mapped = map_items(spec.menu().items(), &state, &tx);
    let MenuItem::Checkmark(item) = &mapped[0] else {
        panic!("expected checkmark item");
    };
    assert!(!item.checked);
    let mut tray = KsniTray {
        spec,
        tooltip: None,
        title: None,
        state,
        events: tx,
    };
    (item.activate)(&mut tray);
    assert!(tray.state.checkmark(5, false));
    assert_eq!(rx.try_recv(), Ok(TrayEvent::MenuActivated { id: 5 }));
}

#[test]
fn consecutive_radio_items_form_one_group_with_selected_index() {
    let (tx, _rx) = std::sync::mpsc::channel();
    let spec = spec_with_menu(vec![
        TrayMenuItem::Radio {
            id: 1,
            label: "Low".into(),
            checked: false,
            enabled: true,
            radio_group: Some(9),
        },
        TrayMenuItem::Radio {
            id: 2,
            label: "High".into(),
            checked: true,
            enabled: true,
            radio_group: Some(9),
        },
        TrayMenuItem::Radio {
            id: 3,
            label: "Standalone".into(),
            checked: true,
            enabled: true,
            radio_group: None,
        },
    ]);
    let state = MenuState::from_spec(spec.menu());
    let mapped = map_items(spec.menu().items(), &state, &tx);
    assert_eq!(mapped.len(), 2);
    let MenuItem::RadioGroup(group) = &mapped[0] else {
        panic!("expected radio group");
    };
    assert_eq!(group.selected, 1);
    assert_eq!(group.options.len(), 2);
    let MenuItem::RadioGroup(standalone) = &mapped[1] else {
        panic!("expected standalone radio group");
    };
    assert_eq!(standalone.options.len(), 1);
    assert_eq!(standalone.selected, 0);
}

#[test]
fn radio_select_emits_the_matching_id() {
    let (tx, rx) = std::sync::mpsc::channel();
    let spec = spec_with_menu(vec![
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
    ]);
    let state = MenuState::from_spec(spec.menu());
    let mapped = map_items(spec.menu().items(), &state, &tx);
    let MenuItem::RadioGroup(group) = &mapped[0] else {
        panic!("expected radio group");
    };
    let mut tray = KsniTray {
        spec,
        tooltip: None,
        title: None,
        state,
        events: tx,
    };
    (group.select)(&mut tray, 1);
    assert!(tray.state.radio_checked(2, false));
    assert!(!tray.state.radio_checked(1, true));
    assert_eq!(rx.try_recv(), Ok(TrayEvent::MenuActivated { id: 2 }));
}

#[test]
fn submenu_recurses_and_separators_map_one_to_one() {
    let (tx, _rx) = std::sync::mpsc::channel();
    let spec = spec_with_menu(vec![
        TrayMenuItem::Separator,
        TrayMenuItem::Submenu {
            label: "More".into(),
            enabled: true,
            items: vec![TrayMenuItem::Separator],
        },
    ]);
    let state = MenuState::from_spec(spec.menu());
    let mapped = map_items(spec.menu().items(), &state, &tx);
    assert!(matches!(mapped[0], MenuItem::Separator));
    let MenuItem::SubMenu(sub) = &mapped[1] else {
        panic!("expected submenu");
    };
    assert_eq!(sub.label, "More");
    assert_eq!(sub.submenu.len(), 1);
    assert!(matches!(sub.submenu[0], MenuItem::Separator));
}

#[test]
fn set_checked_is_exclusive_within_a_radio_group() {
    let mut state = MenuState::default();
    state.radio_group_of.insert(1, 9);
    state.radio_group_of.insert(2, 9);
    state.radio_groups.insert(9, vec![1, 2]);
    state.checked_radios.insert(1, true);
    state.set_checked(2, true);
    assert!(!state.radio_checked(1, true));
    assert!(state.radio_checked(2, false));
    state.set_checked(1, true);
    assert!(state.radio_checked(1, false));
    assert!(!state.radio_checked(2, true));
}

#[test]
fn set_checked_ignores_unknown_ids() {
    let mut state = MenuState::default();
    state.set_checked(42, true);
    state.set_checked(42, false);
}

#[test]
fn spawn_failures_classify_honestly() {
    assert_eq!(
        classify_spawn_error(Error::Dbus(zbus::Error::Failure(String::from("boom")))),
        TrayFailure::MissingDependency
    );
    assert_eq!(
        classify_spawn_error(Error::Watcher(zbus::fdo::Error::ServiceUnknown(
            String::new()
        ))),
        TrayFailure::MissingDependency
    );
    assert_eq!(
        classify_spawn_error(Error::WontShow),
        TrayFailure::TemporarilyUnavailable
    );
}
