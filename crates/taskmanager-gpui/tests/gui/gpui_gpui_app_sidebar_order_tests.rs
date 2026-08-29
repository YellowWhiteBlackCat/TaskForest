use taskmanager_core::core::config::SidebarDeviceOverrideConfig;

use super::{ordered_indices, visible_with_override};

fn keys() -> Vec<String> {
    ["cpu", "memory", "disk:nvme0n1", "network:enp3s0"]
        .into_iter()
        .map(str::to_string)
        .collect()
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
