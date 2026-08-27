//! Small-window policy shared by the production window and headless tests.

use crate::core::config::SidebarDeviceOverrideConfig;
use crate::core::metrics::SystemSnapshot;
use crate::core::{PowerSupplySnapshot, SensorCenterSnapshot, SensorQuantity};
use crate::gpui_app::elements;
use crate::gpui_app::sidebar::{
    NetworkVisibility, SelectedDevice, ordered_indices, visible_with_override,
};
use crate::gpui_app::theme::{Theme, mono_font_with_fallback, tokens};
use crate::i18n;
use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Pixels, Size,
    StatefulInteractiveElement, Styled, div, px, size,
};

use super::{NavOrientation, RootView};

pub const MIN_WIDTH: f32 = 720.0;
pub const MIN_HEIGHT: f32 = 480.0;

/// Navigation is a layout region, not a collection of independently sized
/// controls. These widths are the only rail-width decisions; the page body
/// always owns the remaining space through a flex child with `min_w(0)`.
pub const NAV_RAIL_COMPACT_WIDTH: f32 = 54.0;
pub const NAV_RAIL_WIDTH: f32 = 144.0;
const ULTRA_COMPACT_CONTENT_WIDTH: f32 = 840.0;
const COMPACT_CONTENT_WIDTH: f32 = 1080.0;
const WIDE_CONTENT_WIDTH: f32 = 1600.0;
const CONSTRAINED_CONTENT_HEIGHT: f32 = 700.0;
const GENEROUS_CONTENT_HEIGHT: f32 = 960.0;

/// Horizontal layout capacity shared by every GPUI page.
///
/// Height is deliberately not folded into this enum. A very wide, short
/// window still has enough horizontal room to combine page chrome even when
/// its vertical budget requires secondary content to collapse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayoutProfile {
    UltraCompact,
    Compact,
    Standard,
    Wide,
}

/// Independent vertical capacity. Page-specific projections use this axis to
/// collapse secondary regions without discarding available horizontal space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerticalSpace {
    Constrained,
    Standard,
    Generous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationPresentation {
    IconOnly,
    Labeled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceNavigationPresentation {
    Strip,
    Sidebar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceDetailsPresentation {
    Hidden,
    Pinned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceChartInventory {
    AggregateOnly,
    Full,
}

/// Typed Performance-page allocation derived once at the viewport boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformancePageBudget {
    pub device_navigation: DeviceNavigationPresentation,
    pub details: PerformanceDetailsPresentation,
    pub chart_inventory: PerformanceChartInventory,
    /// Content inset before the pinned details divider. The outer PageFrame
    /// trailing inset remains zero so the pinned rail itself reaches the edge.
    pub main_trailing_inset: f32,
}

impl PerformancePageBudget {
    #[must_use]
    pub const fn from_page_layout(layout: PageLayoutBudget) -> Self {
        let device_navigation = match layout.profile {
            LayoutProfile::UltraCompact => DeviceNavigationPresentation::Strip,
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                DeviceNavigationPresentation::Sidebar
            }
        };
        let details = match layout.profile {
            LayoutProfile::UltraCompact => PerformanceDetailsPresentation::Hidden,
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                PerformanceDetailsPresentation::Pinned
            }
        };
        let chart_inventory = match (layout.profile, layout.vertical_space) {
            (LayoutProfile::UltraCompact, _) | (_, VerticalSpace::Constrained) => {
                PerformanceChartInventory::AggregateOnly
            }
            (
                LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide,
                VerticalSpace::Standard | VerticalSpace::Generous,
            ) => PerformanceChartInventory::Full,
        };
        Self {
            device_navigation,
            details,
            chart_inventory,
            main_trailing_inset: layout.page_padding,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemSurfacePresentation {
    SingleColumn,
    MultiColumn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemPageBudget {
    pub surfaces: SystemSurfacePresentation,
}

impl SystemPageBudget {
    #[must_use]
    pub const fn from_page_layout(layout: PageLayoutBudget) -> Self {
        let surfaces = match layout.profile {
            LayoutProfile::UltraCompact => SystemSurfacePresentation::SingleColumn,
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                SystemSurfacePresentation::MultiColumn
            }
        };
        Self { surfaces }
    }
}

/// Frame-local layout allocation shared by page renderers.
///
/// This is a projection, not persisted UI state: resize computes one new value
/// and every page consumes the same immutable decision for that frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageLayoutBudget {
    pub profile: LayoutProfile,
    pub vertical_space: VerticalSpace,
    pub page_padding: f32,
    pub navigation: NavigationPresentation,
}

#[must_use]
pub fn layout_profile(viewport: Size<Pixels>) -> LayoutProfile {
    match f32::from(viewport.width) {
        width if width < ULTRA_COMPACT_CONTENT_WIDTH => LayoutProfile::UltraCompact,
        width if width < COMPACT_CONTENT_WIDTH => LayoutProfile::Compact,
        width if width < WIDE_CONTENT_WIDTH => LayoutProfile::Standard,
        _ => LayoutProfile::Wide,
    }
}

#[must_use]
pub fn vertical_space(viewport: Size<Pixels>) -> VerticalSpace {
    match f32::from(viewport.height) {
        height if height < CONSTRAINED_CONTENT_HEIGHT => VerticalSpace::Constrained,
        height if height < GENEROUS_CONTENT_HEIGHT => VerticalSpace::Standard,
        _ => VerticalSpace::Generous,
    }
}

impl PageLayoutBudget {
    #[must_use]
    pub fn for_viewport(viewport: Size<Pixels>) -> Self {
        let profile = layout_profile(viewport);
        let vertical_space = vertical_space(viewport);
        let navigation = match profile {
            LayoutProfile::UltraCompact => NavigationPresentation::IconOnly,
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                NavigationPresentation::Labeled
            }
        };
        Self::for_capacity(profile, vertical_space, navigation)
    }

    /// Allocate the page body after accounting for a vertical navigation rail.
    /// The rail presentation and page profile are separate facts: a 900px
    /// window can need an icon-only rail while its remaining body still earns
    /// the Compact profile.
    #[must_use]
    pub fn for_frame(viewport: Size<Pixels>, orientation: NavOrientation) -> Self {
        match orientation {
            NavOrientation::Horizontal => Self::for_viewport(viewport),
            NavOrientation::Vertical => {
                let viewport_width = f32::from(viewport.width);
                let navigation = if viewport_width - NAV_RAIL_WIDTH >= ULTRA_COMPACT_CONTENT_WIDTH {
                    NavigationPresentation::Labeled
                } else {
                    NavigationPresentation::IconOnly
                };
                let body_viewport = size(
                    px((viewport_width - nav_rail_width(navigation)).max(0.0)),
                    viewport.height,
                );
                Self::for_capacity(
                    layout_profile(body_viewport),
                    vertical_space(body_viewport),
                    navigation,
                )
            }
        }
    }

    fn for_capacity(
        profile: LayoutProfile,
        vertical_space: VerticalSpace,
        navigation: NavigationPresentation,
    ) -> Self {
        let page_padding = match profile {
            LayoutProfile::UltraCompact => 8.0,
            LayoutProfile::Compact => 12.0,
            LayoutProfile::Standard | LayoutProfile::Wide => 16.0,
        };
        Self {
            profile,
            vertical_space,
            page_padding,
            navigation,
        }
    }
}

#[must_use]
pub const fn nav_rail_width(presentation: NavigationPresentation) -> f32 {
    match presentation {
        NavigationPresentation::IconOnly => NAV_RAIL_COMPACT_WIDTH,
        NavigationPresentation::Labeled => NAV_RAIL_WIDTH,
    }
}

pub fn settings_content_max_height(viewport: Size<Pixels>) -> f32 {
    (f32::from(viewport.height) - 200.0).max(220.0)
}

/// Parse `WIDTHxHEIGHT` for deterministic capture runs. Invalid values retain
/// the normal 1180x780 launch size; valid values are clamped to the UI contract.
pub fn parse_window_size(value: &str) -> Option<Size<Pixels>> {
    let (width, height) = value.trim().split_once(['x', 'X'])?;
    let width = width.trim().parse::<f32>().ok()?;
    let height = height.trim().parse::<f32>().ok()?;
    if !width.is_finite() || !height.is_finite() {
        return None;
    }
    Some(size(
        px(width.clamp(MIN_WIDTH, 3840.0)),
        px(height.clamp(MIN_HEIGHT, 2160.0)),
    ))
}

pub fn initial_window_size() -> Size<Pixels> {
    std::env::var("TM_WINDOW_SIZE")
        .ok()
        .and_then(|value| parse_window_size(&value))
        .unwrap_or_else(|| size(px(1180.0), px(780.0)))
}

pub fn disconnected_device(theme: &Theme, stable_id: Option<&str>) -> impl IntoElement {
    let mut card = div()
        .max_w(px(460.0))
        .p(px(18.0))
        .rounded(tokens::card_radius(theme))
        .border_1()
        .border_color(theme.gpu)
        .bg(theme.sidebar_card_bg)
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(
            div()
                .font_weight(tokens::FONT_WEIGHT_SEMIBOLD.into())
                .text_color(theme.fg)
                .child(i18n::t("device.disconnected")),
        )
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(i18n::t("device.reconnect_hint")),
        )
        .child(
            div()
                .text_size(tokens::FONT_11)
                .font(mono_font_with_fallback(theme))
                .text_color(theme.fg_dim)
                .child(
                    stable_id.map_or_else(crate::gpui_app::formatting::missing_value, String::from),
                ),
        );
    card = card.debug_selector(|| "tm-device-disconnected".to_string());
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(card)
}

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
        .gap(tokens::SPACE_4)
        .px(tokens::SPACE_8)
        .py(tokens::SPACE_5)
        .bg(theme.sidebar_bg)
        .border_b_1()
        .border_color(theme.border)
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

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_responsive_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_layout_profile_parity_tests.rs"]
mod profile_parity_tests;
