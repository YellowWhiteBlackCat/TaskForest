use super::{
    MAX_PERSISTED_SIDEBAR_KEYS, normalize_sidebar_preferences, reordered_sidebar_order,
    set_sidebar_override,
};
use crate::core::config::SidebarDeviceOverrideConfig;
use crate::gpui_app::root::RootView;
use gpui::{AppContext, TestAppContext};

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

#[gpui::test]
fn root_sidebar_reorder_updates_the_persisted_projection(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(crate::gpui_app::theme::Theme::dark(), cx));
    root.update(cx, |view, cx| {
        view.move_sidebar_device(
            "disk:nvme0n1",
            "cpu",
            &["cpu".into(), "disk:nvme0n1".into(), "memory".into()],
            cx,
        );
        assert_eq!(
            view.presentation_snapshot().sidebar_order(),
            ["disk:nvme0n1", "cpu", "memory"]
        );
    });
}
