//! Compact device-strip layout and its stable selection projection.

use super::{
    Context, InteractiveElement, IntoElement, NetworkVisibility, ParentElement,
    PowerSupplySnapshot, RootView, SelectedDevice, SensorCenterSnapshot, SensorQuantity,
    SidebarDeviceOverrideConfig, StatefulInteractiveElement, Styled, SystemSnapshot, Theme, div,
    elements, i18n, ordered_indices, tokens, visible_with_override,
};

pub struct DeviceStripProps<'a> {
    pub theme: &'a Theme,
    pub snapshot: &'a SystemSnapshot,
    pub power_supplies: &'a PowerSupplySnapshot,
    pub sensors: &'a SensorCenterSnapshot,
    pub selected: SelectedDevice,
    pub show_cpu: bool,
    pub show_memory: bool,
    pub show_disks: bool,
    pub network_visibility: NetworkVisibility,
    pub show_gpus: bool,
    pub sidebar_order: &'a [String],
    pub sidebar_device_overrides: &'a [SidebarDeviceOverrideConfig],
}

struct CompactDevice {
    key: String,
    device: SelectedDevice,
    label: String,
}

pub fn device_strip(props: DeviceStripProps<'_>, cx: &mut Context<RootView>) -> impl IntoElement {
    let DeviceStripProps {
        theme,
        snapshot,
        power_supplies,
        sensors,
        selected,
        show_cpu,
        show_memory,
        show_disks,
        network_visibility,
        show_gpus,
        sidebar_order,
        sidebar_device_overrides,
    } = props;
    let mut devices = Vec::new();
    if visible_with_override("cpu", show_cpu, sidebar_device_overrides) {
        devices.push(CompactDevice {
            key: "cpu".into(),
            device: SelectedDevice::Cpu,
            label: i18n::t("common.cpu").into(),
        });
    }
    if visible_with_override("memory", show_memory, sidebar_device_overrides) {
        devices.push(CompactDevice {
            key: "memory".into(),
            device: SelectedDevice::Memory,
            label: i18n::t("common.memory").into(),
        });
    }
    for (index, disk) in snapshot.disks.iter().enumerate() {
        let key = format!("disk:{}", disk.device_id);
        if !visible_with_override(&key, show_disks, sidebar_device_overrides) {
            continue;
        }
        let label = if disk.model.is_empty() {
            disk.name.clone()
        } else {
            disk.model.clone()
        };
        devices.push(CompactDevice {
            key,
            device: SelectedDevice::Disk(index),
            label,
        });
    }
    for (index, nic) in snapshot.networks.iter().enumerate() {
        let key = format!("network:{}", nic.device_id);
        if !visible_with_override(
            &key,
            network_visibility.allows(nic.adapter_type()),
            sidebar_device_overrides,
        ) {
            continue;
        }
        devices.push(CompactDevice {
            key,
            device: SelectedDevice::Nic(index),
            label: nic.interface_name.as_ref().to_owned(),
        });
    }
    for (index, gpu) in snapshot.gpu.iter().enumerate() {
        let key = format!("gpu:{}", gpu.device_id);
        if !visible_with_override(&key, show_gpus, sidebar_device_overrides) {
            continue;
        }
        devices.push(CompactDevice {
            key,
            device: SelectedDevice::Gpu(index),
            label: gpu.brand.clone(),
        });
    }
    for (index, battery) in power_supplies.batteries.iter().enumerate() {
        let key = format!("battery:{}", battery.id);
        if !visible_with_override(&key, true, sidebar_device_overrides) {
            continue;
        }
        let label = if battery.model_name.is_empty() {
            if battery.display_name.is_empty() {
                format!("{} {}", i18n::t("common.battery"), index)
            } else {
                battery.display_name.clone()
            }
        } else {
            battery.model_name.clone()
        };
        devices.push(CompactDevice {
            key,
            device: SelectedDevice::Battery(index),
            label,
        });
    }
    for (index, reading) in sensors
        .readings
        .iter()
        .filter(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
        .enumerate()
    {
        let key = format!("fan:{}", reading.id());
        if !visible_with_override(&key, true, sidebar_device_overrides) {
            continue;
        }
        devices.push(CompactDevice {
            key,
            device: SelectedDevice::Fan(index),
            label: format!("{} {}", i18n::t("common.fan"), index + 1),
        });
    }

    let entity = cx.entity();
    let mut row = div()
        .id("compact-device-strip")
        .w_full()
        .flex()
        .flex_row()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_4,
        ))
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_5,
        ))
        .bg(taskmanager_ui::theme_binding::fill(theme.sidebar_bg))
        .border_b_1()
        .border_color(taskmanager_ui::theme_binding::hsla(theme.border))
        .overflow_x_scroll();
    let keys: Vec<String> = devices.iter().map(|entry| entry.key.clone()).collect();
    for (position, index) in ordered_indices(&keys, sidebar_order)
        .into_iter()
        .enumerate()
    {
        let entry = &devices[index];
        let device = entry.device;
        let label = &entry.label;
        let entity = entity.clone();
        row = row.child(elements::pill(
            theme,
            ("compact-device", position),
            label,
            selected == device,
            false,
            move |_window, cx| {
                entity.update(cx, |view, cx| {
                    view.select_device(device);
                    cx.notify();
                });
            },
            |_, _, _| {},
        ));
    }
    row
}
