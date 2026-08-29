//! Devices column of the Settings modal: one show/hide toggle per sidebar
//! category (CPU / Memory / Disks / Network / GPUs). Network expands into the
//! five Mission Center categories so the provider can retain all interfaces
//! while the presentation layer controls visibility.

use std::collections::HashMap;

use gpui::{Context, Div, Entity, InteractiveElement, IntoElement, ParentElement, Styled, div};

use taskmanager_ui::inputs::switch::{Switch, SwitchState};

use crate::gpui_app::root::{DevicePreference, RootView};
use taskmanager_application::i18n;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

/// Devices column: one [`device_toggle`] per sidebar category (CPU / Memory /
/// Disks / Network / GPUs), with Network owning five typed child toggles.
/// Each toggle's initial state comes from the immutable presentation snapshot
/// threaded in from `render_settings`.
#[derive(Clone, Copy)]
pub(super) struct DeviceVisibility {
    pub(super) cpu: bool,
    pub(super) memory: bool,
    pub(super) disks: bool,
    pub(super) network: bool,
    pub(super) network_wired: bool,
    pub(super) network_wireless: bool,
    pub(super) network_vpn: bool,
    pub(super) network_virtual: bool,
    pub(super) network_other: bool,
    pub(super) gpus: bool,
}

#[derive(Clone, Copy)]
struct DeviceToggleSpec {
    id: &'static str,
    device: DevicePreference,
    label: &'static str,
    on: bool,
}

pub(super) fn devices_row(
    t: &Theme,
    ent: Entity<RootView>,
    visibility: DeviceVisibility,
    switches: &HashMap<&'static str, Entity<SwitchState>>,
    cx: &mut Context<RootView>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(device_toggle(
            t,
            ent.clone(),
            DeviceToggleSpec {
                id: "device-cpu",
                device: DevicePreference::Cpu,
                label: i18n::t("settings.show_cpu"),
                on: visibility.cpu,
            },
            switches,
            cx,
        ))
        .child(device_toggle(
            t,
            ent.clone(),
            DeviceToggleSpec {
                id: "device-memory",
                device: DevicePreference::Memory,
                label: i18n::t("settings.show_memory"),
                on: visibility.memory,
            },
            switches,
            cx,
        ))
        .child(device_toggle(
            t,
            ent.clone(),
            DeviceToggleSpec {
                id: "device-disks",
                device: DevicePreference::Disks,
                label: i18n::t("settings.show_disks"),
                on: visibility.disks,
            },
            switches,
            cx,
        ))
        .child(network_visibility_group(
            t,
            ent.clone(),
            visibility,
            switches,
            cx,
        ))
        .child(device_toggle(
            t,
            ent.clone(),
            DeviceToggleSpec {
                id: "device-gpus",
                device: DevicePreference::Gpus,
                label: i18n::t("settings.show_gpus"),
                on: visibility.gpus,
            },
            switches,
            cx,
        ))
}

fn network_visibility_group(
    t: &Theme,
    ent: Entity<RootView>,
    visibility: DeviceVisibility,
    switches: &HashMap<&'static str, Entity<SwitchState>>,
    cx: &mut Context<RootView>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_4)
        .child(device_toggle(
            t,
            ent.clone(),
            DeviceToggleSpec {
                id: "device-network",
                device: DevicePreference::Network,
                label: i18n::t("settings.show_network"),
                on: visibility.network,
            },
            switches,
            cx,
        ))
        .children(
            [
                (
                    "network-wired",
                    DevicePreference::NetworkWired,
                    "settings.show_network_wired",
                    visibility.network_wired,
                ),
                (
                    "network-wireless",
                    DevicePreference::NetworkWireless,
                    "settings.show_network_wireless",
                    visibility.network_wireless,
                ),
                (
                    "network-vpn",
                    DevicePreference::NetworkVpn,
                    "settings.show_network_vpn",
                    visibility.network_vpn,
                ),
                (
                    "network-virtual",
                    DevicePreference::NetworkVirtual,
                    "settings.show_network_virtual",
                    visibility.network_virtual,
                ),
                (
                    "network-other",
                    DevicePreference::NetworkOther,
                    "settings.show_network_other",
                    visibility.network_other,
                ),
            ]
            .into_iter()
            .map(|(id, device, label, on)| {
                div().pl(tokens::SPACE_16).child(device_toggle(
                    t,
                    ent.clone(),
                    DeviceToggleSpec {
                        id,
                        device,
                        label: i18n::t(label),
                        on,
                    },
                    switches,
                    cx,
                ))
            }),
        )
}

/// One labeled show/hide toggle: a category caption + a `switch`. `on` is the
/// current visibility flag; flipping the switch issues a typed preference
/// mutation and re-renders.
fn device_toggle(
    t: &Theme,
    ent: Entity<RootView>,
    spec: DeviceToggleSpec,
    switches: &HashMap<&'static str, Entity<SwitchState>>,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let state = switches[spec.id].clone();
    state.update(cx, |state, cx| state.set_on(spec.on, cx));
    let entity = ent.clone();
    div()
        .debug_selector(|| spec.id.to_string())
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(tokens::FONT_13)
                .text_color(t.fg)
                .child(spec.label),
        )
        .child(
            Switch::new(state, t.palette()).on_change(move |on, _win, cx| {
                entity.update(cx, |v, cx| {
                    v.set_device_visibility(spec.device, on, cx);
                });
            }),
        )
}
