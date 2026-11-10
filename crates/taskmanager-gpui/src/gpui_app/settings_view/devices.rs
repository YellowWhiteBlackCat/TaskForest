//! Devices column of the Settings modal: one show/hide toggle per sidebar
//! category (CPU / Memory / Disks / Network / GPUs). Network expands into the
//! five Mission Center categories so the provider can retain all interfaces
//! while the presentation layer controls visibility.

use std::collections::HashMap;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::hsla;

use gpui::{Context, Div, Entity, InteractiveElement, IntoElement, ParentElement, Styled, div};

use taskmanager_ui::inputs::switch::{Switch, SwitchState};

use crate::gpui_app::root::RootView;
use taskforest_core::core::config::{DeviceVisibilityKey, DeviceVisibilityPreferences};
use taskmanager_application::i18n;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

/// Devices column: one [`device_toggle`] per sidebar category (CPU / Memory /
/// Disks / Network / GPUs), with Network owning five typed child toggles.
/// Each toggle's initial state comes from the immutable presentation snapshot
/// threaded in from `render_settings`.
#[derive(Clone, Copy)]
struct DeviceToggleSpec {
    id: &'static str,
    device: DeviceVisibilityKey,
    label: &'static str,
    on: bool,
}

pub(super) fn devices_row(
    t: &Theme,
    ent: Entity<RootView>,
    visibility: DeviceVisibilityPreferences,
    switches: &HashMap<&'static str, Entity<SwitchState>>,
    cx: &mut Context<RootView>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_8))
        .child(device_toggle(
            t,
            ent.clone(),
            DeviceToggleSpec {
                id: DeviceVisibilityKey::Cpu.stable_id(),
                device: DeviceVisibilityKey::Cpu,
                label: i18n::t(DeviceVisibilityKey::Cpu.i18n_key()),
                on: visibility.visible(DeviceVisibilityKey::Cpu),
            },
            switches,
            cx,
        ))
        .child(device_toggle(
            t,
            ent.clone(),
            DeviceToggleSpec {
                id: DeviceVisibilityKey::Memory.stable_id(),
                device: DeviceVisibilityKey::Memory,
                label: i18n::t(DeviceVisibilityKey::Memory.i18n_key()),
                on: visibility.visible(DeviceVisibilityKey::Memory),
            },
            switches,
            cx,
        ))
        .child(device_toggle(
            t,
            ent.clone(),
            DeviceToggleSpec {
                id: DeviceVisibilityKey::Disks.stable_id(),
                device: DeviceVisibilityKey::Disks,
                label: i18n::t(DeviceVisibilityKey::Disks.i18n_key()),
                on: visibility.visible(DeviceVisibilityKey::Disks),
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
                id: DeviceVisibilityKey::Gpus.stable_id(),
                device: DeviceVisibilityKey::Gpus,
                label: i18n::t(DeviceVisibilityKey::Gpus.i18n_key()),
                on: visibility.visible(DeviceVisibilityKey::Gpus),
            },
            switches,
            cx,
        ))
}

fn network_visibility_group(
    t: &Theme,
    ent: Entity<RootView>,
    visibility: DeviceVisibilityPreferences,
    switches: &HashMap<&'static str, Entity<SwitchState>>,
    cx: &mut Context<RootView>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_4))
        .child(device_toggle(
            t,
            ent.clone(),
            DeviceToggleSpec {
                id: DeviceVisibilityKey::Network.stable_id(),
                device: DeviceVisibilityKey::Network,
                label: i18n::t(DeviceVisibilityKey::Network.i18n_key()),
                on: visibility.visible(DeviceVisibilityKey::Network),
            },
            switches,
            cx,
        ))
        .children(
            DeviceVisibilityKey::NETWORK_SUBCATEGORIES
                .into_iter()
                .map(|device| {
                    div()
                        .pl(definite_length(tokens::SPACE_16))
                        .child(device_toggle(
                            t,
                            ent.clone(),
                            DeviceToggleSpec {
                                id: device.stable_id(),
                                device,
                                label: i18n::t(device.i18n_key()),
                                on: visibility.visible(device),
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
                .text_size(font_size(tokens::FONT_13))
                .text_color(hsla(t.fg))
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
