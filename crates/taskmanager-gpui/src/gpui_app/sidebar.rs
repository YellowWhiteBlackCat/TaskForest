//! Devices sidebar: one row per monitored device (CPU, Memory, each Disk, each NIC,
//! each GPU), each with a sparkline + two caption lines, click-to-select with highlight.
//! Plus a minimal "focus" view shown when a non-CPU device is selected (full per-device
//! views — Memory composition, Disk dual-graph, etc. — come in later milestones).

use gpui::{
    App, AppContext, Context, Div, DragMoveEvent, Empty, Entity, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, Render, ScrollHandle, Stateful, StatefulInteractiveElement,
    Styled, Window, div, px,
};
use std::rc::Rc;
use taskmanager_core::core::config::SidebarDeviceOverrideConfig;
use taskmanager_telemetry_store::TelemetryStore;
use taskmanager_ui_contract::IconId;

use crate::gpui_app::formatting::{PerformanceSettings, gpu_identity_text};
use crate::gpui_app::graph::GraphCacheHandle;
use crate::gpui_app::history_samples::{
    battery_capacity_samples, fan_rpm_samples, gpu_usage_samples, network_rate_samples,
    storage_activity_samples,
};
use crate::gpui_app::root::{Hover, RootView};
use taskmanager_application::i18n;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::{PowerSupplySnapshot, SensorCenterSnapshot, SensorQuantity};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::layout::scroll_region_with_overlay_rail;

mod captions;
mod edit;
mod network;
mod order;
mod row;

use captions::{
    append_status_badge, battery_capacity_caption, clean_disk_name, cpu_caption,
    disk_activity_caption, fan_speed_caption, gpu_caption_line1, gpu_caption_line2, mem_caption,
    network_rate_caption,
};
pub use network::NetworkVisibility;
pub(crate) use network::{network_category_label, nic_caption_line2};
pub(crate) use order::{ordered_indices, visible_with_override};
use row::{DeviceRowProps, device_row};
use taskmanager_theme::WindowCorner;

/// Dedicated right-edge hit gutter for sidebar resizing. It is deliberately
/// outside the scroll viewport so the rail's wheel/click hit layer can never
/// cover the horizontal drag target.
const SIDEBAR_RESIZE_GUTTER: Pixels = px(8.0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectedDevice {
    Cpu,
    Memory,
    Disk(usize),
    Nic(usize),
    Gpu(usize),
    Battery(usize),
    Fan(usize),
}

// structural: straight-through render params from RootView state; props consolidation is design-debt item 1
/// Typed drag payload for a sidebar-width resize. Carries the width captured at
/// drag start so each `on_drag_move` computes the new width as a stable delta
/// from the drag origin (`start_width + (cursor.x - anchor_x)`). Implements
/// `gpui::Render` (returning `Empty`) because `on_drag` requires its drag value
/// to be a view the framework can instantiate — mirrors
/// `processes_view::chrome::resize::ProcResizeCol`.
#[derive(Clone, Copy)]
pub(crate) struct SidebarResize {
    start_width: Pixels,
}

impl Render for SidebarResize {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// The right-edge drag handle for the devices sidebar. Its dedicated gutter
/// sits beside, rather than underneath, the scrollbar rail and shows a 1px
/// visible rule in the border color.
/// `cursor_col_resize` + the typed `SidebarResize` drag drive
/// `RootView::resize_sidebar` (clamped `[200, 460]`); `on_mouse_up_out` clears
/// the drag anchor. Mirrors `processes_view::chrome::resize::proc_resize_handle`.
fn sidebar_resize_handle(
    theme: &Theme,
    start_width: Pixels,
    entity: &Entity<RootView>,
) -> Stateful<Div> {
    let ent_move = entity.clone();
    let ent_drag = entity.clone();
    let ent_up = entity.clone();
    let border = theme.border;
    div()
        .flex()
        .flex_row()
        .id("sidebar-resize")
        .debug_selector(|| "tm-sidebar-resize-handle".to_string())
        .occlude()
        .cursor_col_resize()
        .h_full()
        .w(SIDEBAR_RESIZE_GUTTER)
        .flex_none()
        .justify_end()
        .items_center()
        .child(
            div()
                .h_full()
                .justify_center()
                .bg(taskmanager_ui::theme_binding::fill(border))
                .w(px(1.0)),
        )
        .on_drag_move(
            move |e: &DragMoveEvent<SidebarResize>, _win, cx: &mut App| {
                let payload = *e.drag(cx);
                ent_move.update(cx, |view, cx| {
                    let anchor_x = match view.sidebar_resize_anchor_x {
                        Some(x) => x,
                        None => {
                            // First move of this drag: capture the start cursor x
                            // so every later move is a stable delta from drag start.
                            view.sidebar_resize_anchor_x = Some(e.event.position.x);
                            e.event.position.x
                        }
                    };
                    let new_width = payload.start_width + (e.event.position.x - anchor_x);
                    view.resize_sidebar(new_width, cx);
                });
            },
        )
        .on_drag(
            SidebarResize { start_width },
            move |_value, _offset, _win, cx: &mut App| {
                cx.stop_propagation();
                // Reset any stale anchor so the first `on_drag_move` of THIS
                // drag re-captures (covers a prior drag whose mouse-up landed
                // outside the window and never cleared it).
                ent_drag.update(cx, |view, _cx| {
                    view.sidebar_resize_anchor_x = None;
                });
                cx.new(|_| SidebarResize { start_width })
            },
        )
        .on_mouse_up_out(MouseButton::Left, move |_ev, _win, cx: &mut App| {
            ent_up.update(cx, |view, cx| {
                view.sidebar_resize_anchor_x = None;
                cx.notify();
            });
        })
}

/// All straight-through sidebar render inputs (design-debt #1 props
/// consolidation). `cx` stays explicit: it is a render-lifetime handle.
pub(crate) struct SidebarProps<'a> {
    pub theme: &'a Theme,
    pub scroll: &'a ScrollHandle,
    pub width: Pixels,
    pub snap: &'a SystemSnapshot,
    pub telemetry: &'a TelemetryStore,
    /// Generation-cached aggregate CPU usage for the sidebar sparkline (from
    /// `RootView`'s `CpuHistoryCache`): the sidebar renders on every page, so
    /// its CPU row must not re-extract the correlated history each frame.
    pub cpu_usage_samples: std::rc::Rc<[f32]>,
    /// Generation-cached memory usage for the sidebar sparkline (from the
    /// same `MemoryHistoryCache` the Memory page consumes).
    pub memory_usage_samples: std::rc::Rc<[f32]>,
    pub power_supplies: &'a PowerSupplySnapshot,
    pub sensors: &'a SensorCenterSnapshot,
    pub selected: SelectedDevice,
    pub show_cpu: bool,
    pub show_memory: bool,
    pub show_disks: bool,
    pub network_visibility: NetworkVisibility,
    pub show_gpus: bool,
    pub performance: PerformanceSettings,
    pub graph_cache: GraphCacheHandle,
    pub sidebar_order: &'a [String],
    pub sidebar_device_overrides: &'a [SidebarDeviceOverrideConfig],
    pub edit_mode: bool,
    pub hovered: Option<&'a Hover>,
    pub corner_factor: f32,
}

pub(crate) fn render_sidebar(
    props: SidebarProps<'_>,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let SidebarProps {
        theme,
        scroll,
        width,
        snap,
        telemetry,
        cpu_usage_samples,
        memory_usage_samples,
        power_supplies,
        sensors,
        selected,
        show_cpu,
        show_memory,
        show_disks,
        network_visibility,
        show_gpus,
        performance,
        graph_cache,
        sidebar_order,
        sidebar_device_overrides,
        edit_mode,
        hovered,
        corner_factor,
    } = props;
    let units = performance.units;
    let graph_settings = performance.graph;
    let mut entries: Vec<(String, DeviceRowProps<'_>)> = Vec::new();
    let mut body = div()
        .flex()
        .flex_col()
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_10,
        ))
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_2,
        ))
        .child(
            div()
                .px(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_14,
                ))
                .pb(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .flex()
                .items_center()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_6,
                ))
                .font_weight(taskmanager_ui::theme_binding::font_weight(
                    tokens::FONT_WEIGHT_STRONG,
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(taskmanager_ui::icons_binding::icon(IconId::System).size(px(18.0)))
                .child(div().flex_1().child(i18n::t("sidebar.devices")))
                .child(edit::edit_button(theme, edit_mode, cx)),
        );

    // CPU
    let cpu_visible = visible_with_override("cpu", show_cpu, sidebar_device_overrides);
    if edit_mode || cpu_visible {
        let (c1, c2) = cpu_caption(snap);
        entries.push((
            "cpu".to_string(),
            DeviceRowProps {
                theme,
                selected,
                dev: SelectedDevice::Cpu,
                heading: i18n::t("common.cpu").to_string(),
                cap1: c1,
                cap2: c2,
                samples: cpu_usage_samples,
                base: theme.cpu,
                max: 100.0,
                graph_settings,
                graph_cache: graph_cache.clone(),
                hovered,
                id: "dev-cpu".into(),
                icon: IconId::Cpu,
                key: "cpu".into(),
                visible: cpu_visible,
                edit_mode,
            },
        ));
    }

    // Memory
    let memory_visible = visible_with_override("memory", show_memory, sidebar_device_overrides);
    if edit_mode || memory_visible {
        let (c1, c2) = mem_caption(snap, units);
        let samples = memory_usage_samples;
        entries.push((
            "memory".to_string(),
            DeviceRowProps {
                theme,
                selected,
                dev: SelectedDevice::Memory,
                heading: i18n::t("common.memory").to_string(),
                cap1: c1,
                cap2: c2,
                samples,
                base: theme.memory,
                max: 100.0,
                graph_settings,
                graph_cache: graph_cache.clone(),
                hovered,
                id: "dev-mem".into(),
                icon: IconId::Memory,
                key: "memory".into(),
                visible: memory_visible,
                edit_mode,
            },
        ));
    }

    // Disks
    for (i, d) in snap.disks.iter().enumerate() {
        let key = format!("disk:{}", d.device_id);
        let visible = visible_with_override(&key, show_disks, sidebar_device_overrides);
        if !edit_mode && !visible {
            continue;
        }
        let samples = storage_activity_samples(
            &graph_cache,
            &telemetry.system_history,
            &d.device_id,
            d.device_generation,
        );
        let base = clean_disk_name(&d.name);
        // Prefer the vendor+model string (e.g. "ZHITAI TiPro9000 2TB") as the
        // row title; fall back to the device node when sysfs exposes no model.
        let heading = if d.model.is_empty() {
            format!("{} ({})", i18n::t("sidebar.drive"), base)
        } else {
            d.model.clone()
        };
        let c1 = disk_activity_caption(d, units);
        // Append the SMART/NVMe composite temperature when the health node
        // exposes one (`smart_temperature_c` is None on non-NVMe / no hwmon),
        // matching the temperature surfacing on the CPU/GPU caption lines.
        let mut c2 = match d.smart_temperature_c {
            Some(t) if t > 0.0 => format!("{base}  ·  {}  ·  {:.0} °C", d.disk_type, t.round()),
            _ => format!("{base}  ·  {}", d.disk_type),
        };
        if taskmanager_shell::presentation::smart_section_visible(d) {
            append_status_badge(
                &mut c2,
                taskmanager_shell::presentation::effective_smart_status(d),
            );
        }
        entries.push((
            key.clone(),
            DeviceRowProps {
                theme,
                selected,
                dev: SelectedDevice::Disk(i),
                heading,
                cap1: c1,
                cap2: c2,
                samples,
                base: theme.disk,
                max: 100.0,
                graph_settings,
                graph_cache: graph_cache.clone(),
                hovered,
                id: ("dev-disk", i).into(),
                icon: IconId::Disk,
                key: key.clone(),
                visible,
                edit_mode,
            },
        ));
    }

    // Network interfaces
    for (i, n) in snap.networks.iter().enumerate() {
        let key = format!("network:{}", n.device_id);
        let visible = visible_with_override(
            &key,
            network_visibility.allows(n.adapter_type()),
            sidebar_device_overrides,
        );
        if !edit_mode && !visible {
            continue;
        }
        let samples = network_rate_samples(
            &graph_cache,
            &telemetry.system_history,
            &n.device_id,
            n.device_generation,
        );
        // Every category carries the interface name so two adapters of the
        // same type remain distinguishable in the sidebar.
        let heading = format!(
            "{} ({})",
            network_category_label(n.adapter_type()),
            n.interface_name
        );
        let c1 = network_rate_caption(n, units);
        let mut c2 = nic_caption_line2(n);
        append_status_badge(&mut c2, n.device_state.status);
        let max = samples.iter().copied().fold(1.0_f32, f32::max);
        entries.push((
            key.clone(),
            DeviceRowProps {
                theme,
                selected,
                dev: SelectedDevice::Nic(i),
                heading,
                cap1: c1,
                cap2: c2,
                samples,
                base: theme.network,
                max,
                graph_settings,
                graph_cache: graph_cache.clone(),
                hovered,
                id: ("dev-nic", i).into(),
                icon: IconId::Network,
                key,
                visible,
                edit_mode,
            },
        ));
    }

    // GPUs
    for (i, g) in snap.gpu.iter().enumerate() {
        let key = format!("gpu:{}", g.device_id);
        let visible = visible_with_override(&key, show_gpus, sidebar_device_overrides);
        if !edit_mode && !visible {
            continue;
        }
        let samples = gpu_usage_samples(
            &graph_cache,
            &telemetry.system_history,
            &g.device_id,
            g.device_generation,
        );
        // Consume the same product-first identity projection as the detail
        // page so a resolved model never regresses to a generic adapter label.
        let (heading, _) = gpu_identity_text(g, i);
        let c1 = gpu_caption_line1(g, units);
        let mut c2 = gpu_caption_line2(g);
        append_status_badge(&mut c2, g.device_state.status);
        entries.push((
            key.clone(),
            DeviceRowProps {
                theme,
                selected,
                dev: SelectedDevice::Gpu(i),
                heading,
                cap1: c1,
                cap2: c2,
                samples,
                base: theme.gpu,
                max: 100.0,
                graph_settings,
                graph_cache: graph_cache.clone(),
                hovered,
                id: ("dev-gpu", i).into(),
                icon: IconId::Gpu,
                key,
                visible,
                edit_mode,
            },
        ));
    }

    // Runtime power supplies are sourced from the dynamic capability
    // projection, not static hardware inventory: batteries may hot-plug and
    // their generation-scoped history must remain separate.
    for (i, battery) in power_supplies.batteries.iter().enumerate() {
        let key = format!("battery:{}", battery.id);
        let visible = visible_with_override(&key, true, sidebar_device_overrides);
        if !edit_mode && !visible {
            continue;
        }
        let samples = battery_capacity_samples(
            &graph_cache,
            &telemetry.dynamic_history,
            &battery.id,
            battery.device_generation,
        );
        let heading = if battery.model_name.is_empty() {
            if battery.display_name.is_empty() {
                format!("{} {}", i18n::t("common.battery"), i)
            } else {
                battery.display_name.clone()
            }
        } else {
            battery.model_name.clone()
        };
        let capacity = battery_capacity_caption(battery);
        let mut caption = battery.status.clone();
        append_status_badge(&mut caption, battery.device_state.status);
        entries.push((
            key.clone(),
            DeviceRowProps {
                theme,
                selected,
                dev: SelectedDevice::Battery(i),
                heading,
                cap1: capacity,
                cap2: caption,
                samples,
                base: theme.accent,
                max: 100.0,
                graph_settings,
                graph_cache: graph_cache.clone(),
                hovered,
                id: ("dev-battery", i).into(),
                icon: IconId::Health,
                key,
                visible,
                edit_mode,
            },
        ));
    }

    // Fan channels are dynamic hwmon readings. The UI index is only the
    // presentation slot; RootView retains the provider channel ID and
    // generation for hot-plug reconciliation.
    for (i, reading) in sensors
        .readings
        .iter()
        .filter(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
        .enumerate()
    {
        let key = format!("fan:{}", reading.id());
        let visible = visible_with_override(&key, true, sidebar_device_overrides);
        if !edit_mode && !visible {
            continue;
        }
        let samples = fan_rpm_samples(
            &graph_cache,
            &telemetry.dynamic_history,
            reading.id(),
            reading.device_generation(),
        );
        let rpm = fan_speed_caption(reading);
        let mut caption = reading.label().to_owned();
        append_status_badge(&mut caption, reading.state().status);
        entries.push((
            key.clone(),
            DeviceRowProps {
                theme,
                selected,
                dev: SelectedDevice::Fan(i),
                heading: format!("{} {}", i18n::t("common.fan"), i + 1),
                cap1: rpm,
                cap2: caption,
                samples,
                base: theme.cpu,
                max: 3000.0,
                graph_settings,
                graph_cache: graph_cache.clone(),
                hovered,
                id: ("dev-fan", i).into(),
                icon: IconId::Health,
                key,
                visible,
                edit_mode,
            },
        ));
    }

    let keys: Vec<String> = entries.iter().map(|(key, _)| key.clone()).collect();
    let ordered = ordered_indices(&keys, sidebar_order);
    let rendered_order: Rc<[String]> = ordered
        .iter()
        .map(|index| keys[*index].clone())
        .collect::<Vec<_>>()
        .into();
    let mut entries: Vec<Option<DeviceRowProps<'_>>> =
        entries.into_iter().map(|(_, props)| Some(props)).collect();
    for index in ordered {
        if let Some(props) = entries[index].take() {
            body = body.child(device_row(props, Rc::clone(&rendered_order), cx));
        }
    }

    // Scroll the device list vertically so lower devices (many disks/NICs/
    // GPUs) stay reachable instead of being clipped off-screen by the
    // summary card. The shared region owns the intrinsic-content boundary and
    // the shrinkable viewport; the sidebar keeps only its width/background and
    // CSD-corner responsibilities here.
    let content_width = (width - SIDEBAR_RESIZE_GUTTER).max(px(0.0));
    let col = scroll_region_with_overlay_rail(
        "sidebar-list",
        "tm-sidebar-scroll",
        "sidebar-scrollbar",
        "tm-sidebar-scrollbar",
        scroll.clone(),
        theme.palette(),
        body,
    )
    .flex_none()
    .w(content_width)
    .min_w(content_width)
    .max_w(content_width)
    .h_full()
    .bg(taskmanager_ui::theme_binding::fill(theme.sidebar_bg))
    // Round the sidebar's BOTTOM-LEFT corner: it spans the full window
    // height beside the content area and would otherwise paint a square
    // pixel into the transparent CSD corner (its top-left sits under the
    // titlebar, inside the window). 0 when tiled at left/bottom,
    // maximized, or fullscreen (and on non-transparent macOS/Windows),
    // AND when the compositor granted Server decorations (corner_factor=0)
    // — under SSD the system frame owns the outline, so the app must paint
    // a square corner flush into it, never a second app-drawn arc.
    .rounded_bl(px(
        theme.window_corner_radius(WindowCorner::BottomLeft) * corner_factor
    ));

    // The outer slot remains the sole width authority. Its final gutter is
    // reserved for resizing; the scroll viewport and rail end before it.
    div()
        .id("sidebar-fixed-width-frame")
        .debug_selector(|| "tm-sidebar-fixed-width-frame".to_string())
        .flex()
        .flex_row()
        // `w(width)` is only a preferred flex basis. Without the matching
        // min/max contract, a long provider-owned model or interface name can
        // expand the wrapper through min-content sizing even though every row
        // truncates. The resize state is the sole width authority.
        .w(width)
        .min_w(width)
        .max_w(width)
        .flex_none()
        .bg(taskmanager_ui::theme_binding::fill(theme.sidebar_bg))
        .child(col)
        .child(sidebar_resize_handle(theme, width, &cx.entity()))
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_sidebar_tests.rs"]
mod tests;
