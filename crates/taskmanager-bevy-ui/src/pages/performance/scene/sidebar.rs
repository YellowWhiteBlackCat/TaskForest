//! Performance sidebar, CPU header, and responsive rail scenes.

use super::*;
use crate::pages::performance::metrics::{battery_sidebar_title, fans};
use crate::pages::performance::metrics::{disk_sidebar_title, gpu_block_title};
use crate::pages::performance::sidebar_editor::{SidebarAction, SidebarState};
use crate::pages::performance::sidebar_network::network_category_label;
use crate::widgets::chart::{MAX_CHART_POINTS, line_segments_in, polyline_scene};

pub(crate) mod cores;
pub(super) mod cpu;
pub(super) mod device_stats;
pub(super) mod device_stats_more;
pub(super) mod headers;

use super::chart::chart_grid_scene;
use crate::glass::{GlassPanelMaterial, card_material};
use bevy::picking::Pickable;
use bevy::text::{LineBreak, TextLayout};
use bevy::ui_widgets::ScrollArea;
use cpu::device_button_scene;
use taskmanager_ui_contract::IconId;

pub(super) fn marked_text_scene(value: String, role: Role, field: DynField) -> Box<dyn Scene> {
    Box::new(bsn! {
        Text(value)
        TextRole(role)
        DynText(field)
        TextLayout { linebreak: LineBreak::NoWrap }
    })
}

fn cpu_metric_cell_scene(
    label: String,
    value: String,
    field: CpuField,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_width: rem(palette.control_height_rem * 3.5),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_2()),
            overflow: Overflow::clip_x(),
        }
        Children [

                Node {
                    flex_shrink: 0.0,
                    overflow: Overflow::clip_x(),
                }
                Children [  Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }  ]
            --

                Node {
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    min_width: px(0.0),
                    overflow: Overflow::clip_x(),
                }
                Children [

                        Text(value)
                        TextRole(Role::Mono)
                        DynText(DynField::Cpu(field))
                        TextLayout { linebreak: LineBreak::NoWrap }

                ]

        ]
    }
}

/// A bounded history graph shared by device rows and per-core cells. It is a
/// product accessory, not a second chart authority: samples come from the
/// same shell history window as the hero graph, projected through the SAME
/// gap-aware polyline adapter (one visual grammar across the whole page), and
/// an empty window remains visually empty. The strip is static per sidebar
/// rebuild — the page refresh recreates it with the folded window.
/// The smallest graph budget, in rem: one pixel at the 16px baseline. The
/// graph never collapses to a zero-height strip.
const MIN_GRAPH_HEIGHT_REM: f32 = 0.0625;

fn activity_graph_scene(
    samples: &[f32],
    color: bevy::color::Color,
    height_rem: f32,
    capacity: usize,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    // The graph budget rides the rem axis: the node declares the rem ratio
    // and the polyline math resolves the same budget in px against the live
    // rem size, so the graph and its segments can never disagree.
    let height_rem = height_rem.max(MIN_GRAPH_HEIGHT_REM);
    let height_px = height_rem * palette.rem_px;
    let width_px = palette.control_height_px() * 1.6;
    // The strip is a capacity-wide frame too: a window holding fewer samples
    // grows from the right edge instead of claiming the whole strip, the same
    // rule the hero chart and the other frontends' sparklines follow.
    let segments = line_segments_in(samples, width_px, height_px, MAX_CHART_POINTS, capacity);
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: rem(height_rem),
            position_type: PositionType::Relative,
        }
        Children [

                Node {
                    width: percent(100),
                    height: rem(height_rem),
                    position_type: PositionType::Relative,
                }
                Children [
                     @chart_grid_scene(height_px, palette) --

                        Node {
                            width: percent(100),
                            height: percent(100),
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            top: px(0.0),
                            overflow: Overflow::clip_x(),
                        }
                        Children [
                             @{ polyline_scene(&segments, color) }
                        ]

                ]

        ]
    })
}

fn sidebar_activity_scene(
    icon: IconId,
    samples: &[f32],
    color: bevy::color::Color,
    capacity: usize,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    sidebar_activity_with_icon_scene(
        crate::icons::icon_scene(icon, palette, palette.dim_color),
        samples,
        color,
        capacity,
        palette,
    )
}

/// The device row's accessory: the row's own mark beside its activity
/// graph.
///
/// The mark is a scene, not an icon id, so a device whose mark carries its
/// own state — the battery's charge flow — can wear it.
fn sidebar_activity_with_icon_scene(
    icon: Box<dyn Scene>,
    samples: &[f32],
    color: bevy::color::Color,
    capacity: usize,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let height_rem = palette.control_height_rem * 1.35;
    let graph = activity_graph_scene(samples, color, height_rem, capacity, palette);
    Box::new(bsn! {
        Node {
            flex_shrink: 0.0,
            height: rem(height_rem),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
        }
        Children [
             @{ icon } --

                Node {
                    width: rem(palette.control_height_rem * 1.6),
                    height: rem(height_rem),
                    flex_shrink: 0.0,
                }
                Children [
                     @{ graph }
                ]

        ]
    })
}

fn sidebar_curve_scene(
    icon: IconId,
    curve: SystemCurve,
    shell: &ShellApp,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    sidebar_activity_scene(
        icon,
        &curve_samples(shell, curve),
        curve.color(palette),
        shell.history.capacity(),
        palette,
    )
}

pub(super) fn device_sidebar_scene(
    shell: &ShellApp,
    palette: &UiPalette,
    preferences: &SidebarState,
) -> impl Scene + use<> {
    // The strip's frame represents this many samples: the shell's own history
    // window, the same `graph_data_points` preference the other frontends read.
    let capacity = shell.history.capacity();
    let mut rows: Vec<(PerformanceDeviceTarget, Box<dyn Scene>)> = vec![(
        PerformanceDeviceTarget::Cpu,
        Box::new(device_button_scene(
            PerformanceDeviceTarget::Cpu,
            Box::new(device_row_with_accessory_scene(
                t("common.cpu").to_owned(),
                sidebar_curve_scene(IconId::Cpu, SystemCurve::Cpu, shell, palette),
                headers::cpu_caption_scene(shell),
                true,
                palette,
            )),
        )) as Box<dyn Scene>,
    )];
    rows.push((
        PerformanceDeviceTarget::Memory,
        Box::new(device_button_scene(
            PerformanceDeviceTarget::Memory,
            Box::new(device_row_with_accessory_scene(
                t("common.memory").to_owned(),
                sidebar_curve_scene(IconId::Memory, SystemCurve::Memory, shell, palette),
                headers::memory_device_caption_scene(shell),
                false,
                palette,
            )),
        )),
    ));
    if let Some(disks) = shell
        .projection()
        .snapshot
        .as_ref()
        .map(|snapshot| &snapshot.disks)
    {
        for disk in disks {
            let samples = shell
                .history
                .disk_active_time_pct_for(&disk.device_id, disk.device_generation.get());
            rows.push((
                PerformanceDeviceTarget::Disk(disk.device_id.clone()),
                Box::new(device_button_scene(
                    PerformanceDeviceTarget::Disk(disk.device_id.clone()),
                    Box::new(device_row_with_accessory_scene(
                        disk_sidebar_title(disk),
                        sidebar_activity_scene(
                            IconId::Disk,
                            &samples,
                            palette.accent,
                            capacity,
                            palette,
                        ),
                        headers::device_disk_caption_scene(disk),
                        false,
                        palette,
                    )),
                )),
            ));
        }
    }
    if let Some(devices) = network_devices(shell) {
        for nic in devices {
            let key = (*nic.device_id).to_owned();
            let samples = shell
                .history
                .network_bytes_per_sec_for(&key, nic.device_generation.get());
            rows.push((
                PerformanceDeviceTarget::Network(key.clone()),
                Box::new(device_button_scene(
                    PerformanceDeviceTarget::Network(key.clone()),
                    Box::new(device_row_with_accessory_scene(
                        format!(
                            "{} ({})",
                            network_category_label(nic.adapter_type()),
                            nic.interface_name
                        ),
                        sidebar_activity_scene(
                            IconId::Network,
                            &samples,
                            palette.accent,
                            capacity,
                            palette,
                        ),
                        headers::device_network_caption_scene(nic),
                        false,
                        palette,
                    )),
                )),
            ));
        }
    }
    if let Some(devices) = gpu_devices(shell) {
        for gpu in devices {
            let key = gpu.device_id.clone();
            let samples = shell
                .history
                .gpu_usage_pct_for(&key, gpu.device_generation.get());
            rows.push((
                PerformanceDeviceTarget::Gpu(key.clone()),
                Box::new(device_button_scene(
                    PerformanceDeviceTarget::Gpu(key.clone()),
                    Box::new(device_row_with_accessory_scene(
                        gpu_block_title(gpu),
                        sidebar_activity_scene(
                            IconId::Gpu,
                            &samples,
                            palette.accent,
                            capacity,
                            palette,
                        ),
                        headers::device_gpu_caption_scene(gpu),
                        false,
                        palette,
                    )),
                )),
            ));
        }
    }
    if let Some(power) = shell.projection().power_supplies.as_ref() {
        for (index, battery) in power.batteries.iter().enumerate() {
            let key = battery.id.clone();
            let title = battery_sidebar_title(battery, index);
            rows.push((
                PerformanceDeviceTarget::Battery(key.clone()),
                Box::new(device_button_scene(
                    PerformanceDeviceTarget::Battery(key.clone()),
                    Box::new(device_row_with_accessory_scene(
                        title,
                        // The battery wears its OWN mark: the pack's body, its
                        // charge fill, and — while the provider reports the
                        // pack as charging — the travelling charge flow.
                        sidebar_activity_with_icon_scene(
                            super::battery::battery_icon_scene(
                                battery,
                                crate::pages::performance::metrics::battery_charge_pct(battery),
                                palette,
                            ),
                            &[],
                            palette.accent,
                            capacity,
                            palette,
                        ),
                        headers::device_battery_caption_scene(battery),
                        false,
                        palette,
                    )),
                )),
            ));
        }
    }
    if let Some(fan_devices) = fans(shell) {
        for (index, fan) in fan_devices.iter().enumerate() {
            let key = fan.id().to_owned();
            let title = if fan.label().trim().is_empty() {
                format!("{} {}", t("common.fan"), index)
            } else {
                fan.label().trim().to_owned()
            };
            let samples = shell.history.fan_rpm_for(fan.id());
            rows.push((
                PerformanceDeviceTarget::Fan(key.clone()),
                Box::new(device_button_scene(
                    PerformanceDeviceTarget::Fan(key.clone()),
                    Box::new(device_row_with_accessory_scene(
                        title,
                        sidebar_activity_scene(
                            IconId::Performance,
                            &samples,
                            palette.accent,
                            capacity,
                            palette,
                        ),
                        headers::device_fan_caption_scene(fan),
                        false,
                        palette,
                    )),
                )),
            ));
        }
    }
    let edit_btn = crate::pages::performance::sidebar_editor::icon_button(
        IconId::Settings,
        SidebarAction::Open,
        palette,
    );
    let mut panel_children: Vec<Box<dyn Scene>> = vec![Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding: UiRect::bottom(px(crate::palette::space_4())),
        }
        Children [
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(crate::palette::space_4()),
                min_width: px(0.0),
            }
            Children [
                @{ crate::icons::icon_scene(IconId::System, palette, palette.dim_color) }
                Pickable::IGNORE --
                Text(t("sidebar.devices")) TextRole(Role::Caption)
            ] --
            @{ edit_btn }
        ]
    }) as Box<dyn Scene>];
    let targets: Vec<_> = rows.iter().map(|(target, _)| target.clone()).collect();
    let mut rows: Vec<_> = rows
        .into_iter()
        .map(|(target, scene)| (target, Some(scene)))
        .collect();
    for index in preferences.order(&targets) {
        if preferences.visible(&rows[index].0, shell)
            && let Some(scene) = rows[index].1.take()
        {
            panel_children.push(scene);
        }
    }
    let panel = surface_scene(SurfaceTone::Content, panel_children, palette);
    let rail = crate::widgets::scrollbar::scrollbar_rail_scene(palette);
    let lane = crate::widgets::scrollbar::rail_lane(UiRect::default());
    // The mount value is the persisted preference bounded by the shared
    // sidebar range; the root budget re-resolves it against the live window
    // every layout pass, so the scene never carries a private width constant.
    let sidebar_width = preferences.0.sidebar_width.clamp(
        PERFORMANCE_SIDEBAR_MIN_WIDTH_PX,
        PERFORMANCE_SIDEBAR_MAX_WIDTH_PX,
    );
    bsn! {
        // The viewport wrapper owns the scroll area AND the floating rail:
        // the rail is a sibling of the scroll area, so it never scrolls with
        // the content and never takes layout space away from it.
        Node {
            width: px(sidebar_width),
            min_width: px(sidebar_width),
            height: percent(100),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
        }
        PerformanceDeviceSidebar
        Children [
            Node {
                width: percent(100),
                height: percent(100),
                flex_shrink: 0.0,
                overflow: Overflow::scroll_y(),
                padding: { lane },
            }
            ScrollArea
            Children [
                 @{ panel }
            ]
            --
            @{ rail }
        ]
    }
}

pub(super) fn stats_rail_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let cpu_panel = device_stats::cpu_stats_card(shell, palette);
    let memory_panel = device_stats::memory_stats_card(shell, palette);
    let disk_panel = device_stats::disk_stats_card(shell, palette);
    let network_panel = device_stats::network_stats_card(shell, palette);
    let gpu_panel = device_stats::gpu_stats_card(shell, palette);
    let battery_panel = device_stats::battery_stats_card(shell, palette);
    let fan_panel = device_stats::fan_stats_card(shell, palette);

    let cpu_stats = bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            min_height: px(0.0),
            display: Display::Flex,
        }
        PerformanceStatsRailTarget(PerformanceDeviceTarget::Cpu)
        Children [
            @{ cpu_panel }
        ]
    };
    let memory_stats = bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            min_height: px(0.0),
            display: Display::None,
        }
        PerformanceStatsRailTarget(PerformanceDeviceTarget::Memory)
        Children [
            @{ memory_panel }
        ]
    };
    let disk_stats = bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            min_height: px(0.0),
            display: Display::None,
        }
        PerformanceStatsRailTarget(PerformanceDeviceTarget::Disk(String::new()))
        Children [
            @{ disk_panel }
        ]
    };
    let network_stats = bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            min_height: px(0.0),
            display: Display::None,
        }
        PerformanceStatsRailTarget(PerformanceDeviceTarget::Network(String::new()))
        Children [
            @{ network_panel }
        ]
    };
    let gpu_stats = bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            min_height: px(0.0),
            display: Display::None,
        }
        PerformanceStatsRailTarget(PerformanceDeviceTarget::Gpu(String::new()))
        Children [
            @{ gpu_panel }
        ]
    };
    let battery_stats = bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            min_height: px(0.0),
            display: Display::None,
        }
        PerformanceStatsRailTarget(PerformanceDeviceTarget::Battery(String::new()))
        Children [
            @{ battery_panel }
        ]
    };
    let fan_stats = bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            min_height: px(0.0),
            display: Display::None,
        }
        PerformanceStatsRailTarget(PerformanceDeviceTarget::Fan(String::new()))
        Children [
            @{ fan_panel }
        ]
    };

    let rail = crate::widgets::scrollbar::scrollbar_rail_scene(palette);
    let lane = crate::widgets::scrollbar::rail_lane(UiRect::default());
    bsn! {
        Node {
            width: px(WIDE_STATS_WIDTH_PX),
            min_width: px(WIDE_STATS_WIDTH_PX),
            height: percent(100),
            min_height: px(0.0),
            max_height: percent(100),
            flex_shrink: 0.0,
            position_type: PositionType::Relative,
        }
        PerformanceStatsRail
        Children [
            Node {
                width: percent(100),
                height: percent(100),
                min_height: px(0.0),
                flex_shrink: 0.0,
                overflow: Overflow::scroll_y(),
                padding: { lane },
            }
            ScrollArea
            Children [
                @{ cpu_stats } --
                @{ memory_stats } --
                @{ disk_stats } --
                @{ network_stats } --
                @{ gpu_stats } --
                @{ battery_stats } --
                @{ fan_stats }
            ]
            --
            @{ rail }
        ]
    }
}

pub(super) fn summary_card_scene(
    rows: Vec<Box<dyn Scene>>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let GlassPanelMaterial {
        fill,
        edge,
        shadows: _,
        edge_width_rem,
        radius_rem,
        grain: _,
    } = card_material(palette);
    bsn! {
        Node {
            width: percent(100),
            max_height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::axes(Val::Px(space_12()), Val::Px(space_4())),
            border: UiRect::all(Val::Rem(edge_width_rem)),
            border_radius: BorderRadius::all(Val::Rem(radius_rem)),
            overflow: Overflow::clip_y(),
        }
        BackgroundColor({ fill })
        BorderColor::all(edge)
        Children [
            { rows }
        ]
    }
}

fn metric_selector_button_scene(curve: SystemCurve, palette: &UiPalette) -> impl Scene + use<> {
    let active = curve == SystemCurve::default();
    let pill = pill_scene(curve_selector_label(curve), active, palette);
    bsn! {
         @{ pill } PerformanceFocusButton(curve) on(focus_button_activated)
    }
}

fn metric_selector_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let buttons: Vec<Box<dyn Scene>> = SystemCurve::STRIP
        .iter()
        .filter(|&&curve| curve_wanted(shell, curve))
        .map(|&curve| Box::new(metric_selector_button_scene(curve, palette)) as Box<dyn Scene>)
        .collect();
    bsn! {
        Node {
            width: percent(100),
            display: Display::None,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_4())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            { buttons }
        ]
    }
}
