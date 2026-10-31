//! test-intent: behavior
use super::*;

fn keys() -> Vec<String> {
    ["cpu", "memory", "disk:nvme0n1", "network:enp3s0"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

#[test]
fn relative_moves_preserve_absent_device_choices_and_reject_missing_targets() {
    let live = keys();
    let preferred = vec!["future:device".to_owned()];
    assert_eq!(
        move_sidebar_order(&live, &preferred, "memory", -1),
        Some(vec![
            "memory".into(),
            "cpu".into(),
            "disk:nvme0n1".into(),
            "network:enp3s0".into(),
            "future:device".into()
        ])
    );
    assert_eq!(
        move_sidebar_order(&live, &preferred, "cpu", 1),
        Some(vec![
            "memory".into(),
            "cpu".into(),
            "disk:nvme0n1".into(),
            "network:enp3s0".into(),
            "future:device".into()
        ])
    );
    assert_eq!(move_sidebar_order(&live, &preferred, "cpu", -1), None);
    assert_eq!(move_sidebar_order(&live, &preferred, "missing", 1), None);
}

#[test]
fn order_ignores_unknowns_deduplicates_and_appends_new_devices() {
    let keys = keys();
    let preferred = [
        "network:enp3s0".to_string(),
        "future:device".to_string(),
        "network:enp3s0".to_string(),
        "cpu".to_string(),
    ];
    assert_eq!(ordered_indices(&keys, &preferred), [3, 0, 1, 2]);
}

#[test]
fn empty_order_preserves_discovery_order() {
    let keys = keys();
    assert_eq!(ordered_indices(&keys, &[]), [0, 1, 2, 3]);
}

#[test]
fn concrete_override_wins_and_last_duplicate_is_authoritative() {
    let overrides = vec![
        SidebarDeviceOverrideConfig {
            device: "disk:nvme0n1".into(),
            visible: true,
        },
        SidebarDeviceOverrideConfig {
            device: "disk:nvme0n1".into(),
            visible: false,
        },
    ];
    assert!(!visible_with_override("disk:nvme0n1", true, &overrides));
    assert!(!visible_with_override("disk:nvme0n1", false, &overrides));
    assert!(visible_with_override("cpu", true, &overrides));
    assert!(!visible_with_override("cpu", false, &overrides));
}

fn order(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn reorder_moves_dragged_entry_before_target_and_preserves_stale_keys() {
    assert_eq!(
        reordered_sidebar_order(
            &order(&["cpu", "memory", "gpu"]),
            &order(&["legacy:disk"]),
            "cpu",
            "gpu",
        ),
        Some(order(&["memory", "cpu", "gpu", "legacy:disk"])),
    );
    assert_eq!(
        reordered_sidebar_order(&order(&["cpu"]), &[], "cpu", "cpu"),
        None
    );
    assert_eq!(
        reordered_sidebar_order(&order(&["cpu"]), &[], "future", "cpu"),
        None
    );
}

#[test]
fn override_replaces_duplicate_key_without_erasing_other_devices() {
    let mut overrides = vec![SidebarDeviceOverrideConfig {
        device: "disk:a".into(),
        visible: true,
    }];
    set_sidebar_override(&mut overrides, "network:enp3s0", false);
    set_sidebar_override(&mut overrides, "disk:a", false);
    assert_eq!(
        overrides,
        vec![
            SidebarDeviceOverrideConfig {
                device: "network:enp3s0".into(),
                visible: false,
            },
            SidebarDeviceOverrideConfig {
                device: "disk:a".into(),
                visible: false,
            },
        ]
    );
}

#[test]
fn corrupt_sidebar_preferences_are_trimmed_deduplicated_and_bounded() {
    let order = vec![
        " memory ".into(),
        String::new(),
        "disk:nvme0n1".into(),
        "memory".into(),
        "future:device".into(),
    ];
    let overrides = vec![
        SidebarDeviceOverrideConfig {
            device: " ".into(),
            visible: false,
        },
        SidebarDeviceOverrideConfig {
            device: " disk:nvme0n1 ".into(),
            visible: true,
        },
        SidebarDeviceOverrideConfig {
            device: "disk:nvme0n1".into(),
            visible: false,
        },
        SidebarDeviceOverrideConfig {
            device: "network:future".into(),
            visible: true,
        },
    ];

    let (order, overrides) = normalize_sidebar_preferences(&order, &overrides);
    assert_eq!(order, ["memory", "disk:nvme0n1", "future:device"]);
    assert_eq!(
        overrides,
        [
            SidebarDeviceOverrideConfig {
                device: "disk:nvme0n1".into(),
                visible: false,
            },
            SidebarDeviceOverrideConfig {
                device: "network:future".into(),
                visible: true,
            },
        ]
    );

    let oversized = (0..(MAX_PERSISTED_SIDEBAR_KEYS + 9))
        .map(|index| format!("future:{index}"))
        .collect::<Vec<_>>();
    let (order, _) = normalize_sidebar_preferences(&oversized, &[]);
    assert_eq!(order.len(), MAX_PERSISTED_SIDEBAR_KEYS);
    assert_eq!(order.first().map(String::as_str), Some("future:0"));
    assert_eq!(order.last().map(String::as_str), Some("future:127"));
}
