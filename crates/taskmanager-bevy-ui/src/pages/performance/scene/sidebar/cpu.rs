//! CPU-focused Performance scene builders.

use super::super::blocks::section_scene;
use super::super::chart::curve_card_scene;
use super::*;
use crate::pages::performance::metrics::battery_sidebar_title;
use crate::pages::performance::sidebar_editor::SidebarState;
use crate::pages::performance::{DeviceCategoryKind, DeviceViewCategory};
use crate::widgets::layout_budget::PerformanceLowerBand;
use bevy::ui_widgets::ScrollArea;

fn cpu_metric_strip_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let metrics: Vec<Box<dyn Scene>> = [
        (t("common.utilization"), CpuField::Usage),
        (t("cpu.frequency"), CpuField::Frequency),
        (t("common.temperature"), CpuField::Temperature),
        (t("common.power"), CpuField::Power),
    ]
    .into_iter()
    .map(|(label, field)| {
        Box::new(super::cpu_metric_cell_scene(
            label.to_owned(),
            cpu_field_text(shell, field),
            field,
            palette,
        )) as Box<dyn Scene>
    })
    .collect();
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            { metrics }
        ]
    }
}

/// The per-core grid card: the shared graph card owns the card chrome, and
/// the per-core body beside it owns the class groups, the socket envelopes,
/// the responsive column policy, and the collapse/expand reveal.
///
/// The card composes the per-core grid. Its production mount owns the
/// lower-band marker, so one-off scene tests can still measure the card
/// itself without the frame-level admission policy.
pub(super) fn cpu_core_grid_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let body = super::cores::core_grid_body_scene(shell, palette);
    let card = graph_card_scene(
        t("common.cores").to_owned(),
        super::cores::core_grid_subtitle(shell),
        Box::new(body),
        palette,
    );
    bsn! {
        Node {
            width: percent(100.0),
            display: Display::Flex,
        }
        Children [
            @{ card }
        ]
    }
}

/// One per-core bar row: identity label over a bounded track whose fill is
/// the core's live utilization, with the numeric fact as the row's only
/// mutable text. The bar and the number are two views of the same
/// observation — the fill width is rewritten by the fold observer through
/// [`DynBar`], the number through [`DynText`], so a fold repaints both or
/// neither.
///
/// The grid itself — the heterogeneous class groups, the socket envelopes,
/// the responsive column policy, and the collapse/expand reveal — lives in
/// [`super::cores`]; this module keeps the CPU header and the metric strip.
fn device_pill_scene(
    target: PerformanceDeviceTarget,
    label: String,
    active: bool,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            min_width: rem(palette.control_height_rem * 2.8),
            height: rem(palette.control_height_rem),
            padding: UiRect::horizontal(Val::Px(space_12())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({
            if active { palette.nav_active_bg } else { palette.content_bg }
        })
        ControlVisual(ControlTone::Surface, active)
        Button
        PerformanceDeviceButton(target)
        on(device_button_activated)
        Children [
             Text(label) TextRole(Role::Body)
        ]
    }
}

pub(super) fn device_button_scene(
    target: PerformanceDeviceTarget,
    row: Box<dyn Scene>,
) -> impl Scene + use<> {
    bsn! {
         @{ row } Button PerformanceDeviceButton(target) on(device_button_activated)
    }
}

fn compact_device_pills_scene(
    shell: &ShellApp,
    palette: &UiPalette,
    preferences: &SidebarState,
) -> impl Scene + use<> {
    let mut labels = vec![
        (PerformanceDeviceTarget::Cpu, t("common.cpu").to_owned()),
        (
            PerformanceDeviceTarget::Memory,
            t("common.memory").to_owned(),
        ),
    ];
    if let Some(snapshot) = shell.projection().snapshot.as_ref() {
        labels.extend(snapshot.disks.iter().map(|disk| {
            (
                PerformanceDeviceTarget::Disk(disk.device_id.clone()),
                disk_sidebar_title(disk),
            )
        }));
    }
    if let Some(devices) = network_devices(shell) {
        labels.extend(devices.iter().map(|nic| {
            (
                PerformanceDeviceTarget::Network((*nic.device_id).to_owned()),
                if nic.interface_name.is_empty() {
                    (*nic.device_id).to_owned()
                } else {
                    (*nic.interface_name).to_owned()
                },
            )
        }));
    }
    if let Some(devices) = gpu_devices(shell) {
        labels.extend(devices.iter().map(|gpu| {
            (
                PerformanceDeviceTarget::Gpu(gpu.device_id.clone()),
                gpu_block_title(gpu),
            )
        }));
    }
    if let Some(power) = shell.projection().power_supplies.as_ref() {
        labels.extend(power.batteries.iter().enumerate().map(|(index, battery)| {
            (
                PerformanceDeviceTarget::Battery(battery.id.clone()),
                battery_sidebar_title(battery, index),
            )
        }));
    }
    let targets: Vec<_> = labels.iter().map(|(target, _)| target.clone()).collect();
    let mut labels: Vec<_> = labels.into_iter().map(Some).collect();
    let pills: Vec<Box<dyn Scene>> = preferences
        .order(&targets)
        .into_iter()
        .filter(|index| preferences.visible(&targets[*index], shell))
        .filter_map(|index| labels[index].take())
        .map(|(target, label)| {
            let active = target == PerformanceDeviceTarget::default();
            Box::new(device_pill_scene(target, label, active, palette)) as Box<dyn Scene>
        })
        .collect();
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            row_gap: Val::Px(space_4()),
            padding: UiRect::vertical(Val::Px(space_2())),
            display: Display::None,
        }
        PerformanceCompactDevicePills
        Children [
            { pills }
        ]
    }
}

pub(crate) fn cpu_main_scene(
    shell: &ShellApp,
    palette: &UiPalette,
    preferences: &SidebarState,
) -> impl Scene + use<> {
    let cards: Vec<Box<dyn Scene>> = SystemCurve::STRIP
        .iter()
        .map(|&curve| Box::new(curve_card_scene(curve, shell, palette)) as Box<dyn Scene>)
        .collect();
    let selector = metric_selector_scene(shell, palette);
    let headers = super::headers::all_device_headers_scene(shell);

    let core_grid = bsn! {
        Node { width: percent(100), display: Display::Flex }
        PerformanceLowerBand
        Children [ @cpu_core_grid_scene(shell, palette) ]
    };
    let cpu_container = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            display: Display::Flex,
        }
        DeviceViewCategory(DeviceCategoryKind::Cpu)
        Children [
             @cpu_metric_strip_scene(shell, palette) --
             @{ core_grid }
        ]
    };

    let memory_container = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            display: Display::None,
        }
        DeviceViewCategory(DeviceCategoryKind::Memory)
        Children [
             @section_scene(Section::MemorySegments, shell, palette)
        ]
    };

    let disk_container = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            display: Display::None,
        }
        DeviceViewCategory(DeviceCategoryKind::Disk)
        Children [
             @section_scene(Section::Disk, shell, palette)
        ]
    };

    let network_container = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            display: Display::None,
        }
        DeviceViewCategory(DeviceCategoryKind::Network)
        Children [
             @section_scene(Section::Network, shell, palette)
        ]
    };

    let gpu_container = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            display: Display::None,
        }
        DeviceViewCategory(DeviceCategoryKind::Gpu)
        Children [
             @section_scene(Section::Gpu, shell, palette)
        ]
    };

    let battery_container = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            display: Display::None,
        }
        DeviceViewCategory(DeviceCategoryKind::Battery)
        Children [
             @section_scene(Section::Battery, shell, palette)
        ]
    };

    let fan_container = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            display: Display::None,
        }
        DeviceViewCategory(DeviceCategoryKind::Fan)
        Children [
             @section_scene(Section::Fan, shell, palette)
        ]
    };

    let rail = crate::widgets::scrollbar::scrollbar_rail_scene(palette);
    let lane = crate::widgets::scrollbar::rail_lane(UiRect::bottom(Val::Px(space_8())));
    bsn! {
        Node {
            width: percent(100),
            min_width: px(MAIN_GRAPH_MIN_WIDTH_PX),
            height: percent(100),
            flex_grow: 1.0,
            flex_shrink: 1.0,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::vertical(Val::Px(space_4())),
        }
        Children [
             @compact_device_pills_scene(shell, palette, preferences) --
             @{ selector } --
             { headers } --

                Node {
                    width: percent(100),
                    // The hero chart is the elastic primary region: it absorbs
                    // the column's surplus height down to the strip's own rem
                    // floor, so the lower detail card hugs the bottom edge
                    // instead of leaving dead background under a short group.
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    min_height: px(0.0),
                    flex_basis: px(0.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                Children [
                    { cards }
                ]
            --
            // The viewport wrapper owns the scroll area AND the floating
            // rail: the rail is a sibling of the scroll area, so it never
            // scrolls with the content and never takes layout space away
            // from it.
            Node {
                width: percent(100),
                // The lower group takes its content height and shrinks (then
                // scrolls) only when the column runs out of room, so a short
                // group never leaves dead space and a tall group never
                // breaches the viewport edge.
                min_height: px(0.0), flex_grow: 0.0, flex_shrink: 1.0,
                flex_direction: FlexDirection::Column,
            }
            Children [
                Node { width: percent(100), min_height: px(0.0), flex_grow: 1.0, flex_shrink: 1.0, flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), padding: { lane } }
                ScrollArea PerformanceDeviceBody Children [
                    @{ cpu_container } -- @{ memory_container } -- @{ disk_container } --
                    @{ network_container } -- @{ gpu_container } -- @{ battery_container } --
                    @{ fan_container }
                ]
                --
                @{ rail }
            ]
        ]
    }
}
