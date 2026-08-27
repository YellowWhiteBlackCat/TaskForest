use super::*;

fn valid_icon() -> TrayIconData {
    TrayIconData::from_rgba(vec![0u8; 16 * 16 * 4], 16, 16).unwrap()
}

fn valid_menu() -> TrayMenuSpec {
    TrayMenuSpec::from_items(vec![TrayMenuItem::Action {
        id: 1,
        label: "Show".into(),
        enabled: true,
    }])
    .unwrap()
}

#[test]
fn icon_accepts_a_canonical_rgba_buffer() {
    let icon = valid_icon();
    assert_eq!(icon.width(), 16);
    assert_eq!(icon.height(), 16);
    assert_eq!(icon.pixels().len(), 16 * 16 * 4);
}

#[test]
fn icon_rejects_empty_dimensions() {
    assert_eq!(
        TrayIconData::from_rgba(Vec::new(), 0, 16),
        Err(TrayIconError::EmptyDimension)
    );
    assert_eq!(
        TrayIconData::from_rgba(Vec::new(), 16, 0),
        Err(TrayIconError::EmptyDimension)
    );
}

#[test]
fn icon_rejects_oversized_dimensions() {
    let dimension = MAX_TRAY_ICON_DIMENSION + 1;
    assert_eq!(
        TrayIconData::from_rgba(Vec::new(), dimension, 1),
        Err(TrayIconError::DimensionTooLarge { dimension })
    );
}

#[test]
fn icon_rejects_length_mismatch() {
    assert_eq!(
        TrayIconData::from_rgba(vec![0u8; 16 * 16 * 4 - 1], 16, 16),
        Err(TrayIconError::PixelBufferLengthMismatch {
            expected: 16 * 16 * 4,
            actual: 16 * 16 * 4 - 1,
        })
    );
    assert_eq!(
        TrayIconData::from_rgba(vec![0u8; 0], 16, 16),
        Err(TrayIconError::PixelBufferLengthMismatch {
            expected: 16 * 16 * 4,
            actual: 0,
        })
    );
}

#[test]
fn menu_item_kind_enumeration_is_exhaustive() {
    let items = [
        TrayMenuItem::Action {
            id: 1,
            label: "a".into(),
            enabled: true,
        },
        TrayMenuItem::Checkmark {
            id: 2,
            label: "c".into(),
            checked: false,
            enabled: true,
        },
        TrayMenuItem::Radio {
            id: 3,
            label: "r".into(),
            checked: false,
            enabled: true,
            radio_group: Some(1),
        },
        TrayMenuItem::Submenu {
            label: "s".into(),
            items: Vec::new(),
            enabled: true,
        },
        TrayMenuItem::Separator,
    ];
    let mut kinds: Vec<_> = items.iter().map(TrayMenuItem::kind).collect();
    kinds.sort_by_key(|kind| {
        TrayMenuItemKind::ALL
            .iter()
            .position(|candidate| candidate == kind)
            .unwrap()
    });
    assert_eq!(kinds, TrayMenuItemKind::ALL.to_vec());
}

#[test]
fn menu_counts_every_nesting_level() {
    let spec = TrayMenuSpec::from_items(vec![
        TrayMenuItem::Separator,
        TrayMenuItem::Submenu {
            label: "sub".into(),
            enabled: true,
            items: vec![
                TrayMenuItem::Action {
                    id: 1,
                    label: "leaf".into(),
                    enabled: true,
                },
                TrayMenuItem::Submenu {
                    label: "nested".into(),
                    enabled: true,
                    items: vec![TrayMenuItem::Action {
                        id: 2,
                        label: "deep".into(),
                        enabled: true,
                    }],
                },
            ],
        },
    ])
    .unwrap();
    assert_eq!(spec.node_count(), 5);
}

#[test]
fn menu_rejects_oversized_trees() {
    let many = (0..=MAX_TRAY_MENU_NODES)
        .map(|_| TrayMenuItem::Separator)
        .collect();
    assert!(matches!(
        TrayMenuSpec::from_items(many),
        Err(TrayMenuSpecError::TooManyNodes { .. })
    ));
}

#[test]
fn menu_rejects_deep_nesting() {
    let depth = MAX_TRAY_MENU_DEPTH + 1;
    let mut tree = vec![TrayMenuItem::Separator];
    for _ in 0..depth {
        tree = vec![TrayMenuItem::Submenu {
            label: "wrap".into(),
            enabled: true,
            items: tree,
        }];
    }
    assert!(matches!(
        TrayMenuSpec::from_items(tree),
        Err(TrayMenuSpecError::NestingTooDeep { .. })
    ));
}

#[test]
fn menu_rejects_overlong_labels() {
    let long = "x".repeat(MAX_TRAY_LABEL_CHARS + 1);
    assert!(matches!(
        TrayMenuSpec::from_items(vec![TrayMenuItem::Action {
            id: 1,
            label: long,
            enabled: true,
        }]),
        Err(TrayMenuSpecError::LabelTooLong { .. })
    ));
}

#[test]
fn menu_rejects_two_checked_radios_in_one_group() {
    let spec = TrayMenuSpec::from_items(vec![
        TrayMenuItem::Radio {
            id: 1,
            label: "a".into(),
            checked: true,
            enabled: true,
            radio_group: Some(7),
        },
        TrayMenuItem::Radio {
            id: 2,
            label: "b".into(),
            checked: true,
            enabled: true,
            radio_group: Some(7),
        },
    ]);
    assert_eq!(
        spec,
        Err(TrayMenuSpecError::RadioGroupConflict { group: 7 })
    );
    let across_submenus = TrayMenuSpec::from_items(vec![
        TrayMenuItem::Radio {
            id: 1,
            label: "a".into(),
            checked: true,
            enabled: true,
            radio_group: Some(7),
        },
        TrayMenuItem::Submenu {
            label: "sub".into(),
            enabled: true,
            items: vec![TrayMenuItem::Radio {
                id: 2,
                label: "b".into(),
                checked: true,
                enabled: true,
                radio_group: Some(7),
            }],
        },
    ]);
    assert_eq!(
        across_submenus,
        Err(TrayMenuSpecError::RadioGroupConflict { group: 7 })
    );
}

#[test]
fn spec_bounds_tooltip_and_title() {
    let icon = valid_icon();
    let menu = valid_menu();
    let tooltip = "t".repeat(MAX_TRAY_TOOLTIP_CHARS + 1);
    assert!(matches!(
        TraySpec::new(icon.clone(), Some(tooltip), None, menu.clone(), false),
        Err(TraySpecError::TooltipTooLong { .. })
    ));
    let title = "t".repeat(MAX_TRAY_TITLE_CHARS + 1);
    assert!(matches!(
        TraySpec::new(icon, None, Some(title), menu, false),
        Err(TraySpecError::TitleTooLong { .. })
    ));
}

#[test]
fn spec_round_trips_accessors() {
    let icon = valid_icon();
    let menu = valid_menu();
    let spec = TraySpec::new(
        icon.clone(),
        Some("TaskForest".into()),
        Some("TaskForest".into()),
        menu.clone(),
        true,
    )
    .unwrap();
    assert_eq!(spec.icon(), &icon);
    assert_eq!(spec.tooltip(), Some("TaskForest"));
    assert_eq!(spec.title(), Some("TaskForest"));
    assert_eq!(spec.menu(), &menu);
    assert!(spec.show_menu_on_left_click());
}

#[test]
fn empty_menu_is_a_valid_icon_only_tray() {
    let menu = TrayMenuSpec::from_items(Vec::new()).unwrap();
    assert!(menu.is_empty());
    let spec = TraySpec::new(valid_icon(), None, None, menu, false).unwrap();
    assert_eq!(spec.menu().node_count(), 0);
}
