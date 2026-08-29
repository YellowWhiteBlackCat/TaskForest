//! Performance sidebar, CPU header, and responsive rail scenes.

use super::*;
use crate::widgets::chart::{MAX_CHART_POINTS, line_segments, polyline_scene};

pub(super) mod cpu;

use super::blocks::gpu_block_title;
use super::chart::chart_grid_scene;
use cpu::device_button_scene;

/// One label→value line; the value is the rewritable fact. Both columns are
/// strictly single-line: a long value clips at the rail edge instead of
/// wrapping the row into an unreadable stack.
fn fact_row(label: String, value: String, field: DynField) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            min_width: px(180.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: Val::Px(space_2()),
            padding: UiRect::vertical(Val::Px(space_2())),
        }
        Children [
            (
                Node {
                    min_width: px(100.0),
                    overflow: Overflow::clip_x(),
                }
                Children [ ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) ) ]
            ),
            (
                Node {
                    min_width: px(0.0),
                    flex_shrink: 1.0,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::FlexEnd,
                    overflow: Overflow::clip_x(),
                }
                Children [ ( Text(value) TextRole(Role::Mono) DynText(field) template_value(no_wrap_text()) ) ]
            ),
        ]
    }
}

fn marked_text_scene(value: String, role: Role, field: DynField) -> Box<dyn Scene> {
    Box::new(bsn! {
        Text(value)
        TextRole(role)
        DynText(field)
        template_value(no_wrap_text())
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
            min_width: px(palette.control_height_px * 3.5),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_2()),
        }
        Children [
            ( Text(label) TextRole(Role::Caption) ),
            (
                Text(value)
                TextRole(Role::Mono)
                DynText(DynField::Cpu(field))
                template_value(no_wrap_text())
            ),
        ]
    }
}

fn cpu_device_caption_scene(shell: &ShellApp) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_2()),
            overflow: Overflow::clip_x(),
        }
        Children [
            (
                Text(cpu_field_text(shell, CpuField::Usage))
                TextRole(Role::Mono)
                DynText(DynField::Cpu(CpuField::Usage))
                template_value(no_wrap_text())
            ),
            ( Text(" · ") TextRole(Role::Caption) ),
            (
                Text(cpu_field_text(shell, CpuField::Frequency))
                TextRole(Role::Mono)
                DynText(DynField::Cpu(CpuField::Frequency))
                template_value(no_wrap_text())
            ),
        ]
    })
}

/// A bounded history graph shared by device rows and per-core cells. It is a
/// product accessory, not a second chart authority: samples come from the
/// same shell history window as the hero graph, projected through the SAME
/// gap-aware polyline adapter (one visual grammar across the whole page), and
/// an empty window remains visually empty. The strip is static per sidebar
/// rebuild — the page refresh recreates it with the folded window.
fn activity_graph_scene(
    samples: &[f32],
    color: bevy::color::Color,
    height: f32,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let height = height.max(1.0);
    let width = palette.control_height_px * 2.0;
    let segments = line_segments(samples, width, height, MAX_CHART_POINTS);
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: px(height),
            position_type: PositionType::Relative,
        }
        Children [
            (
                Node {
                    width: percent(100),
                    height: px(height),
                    position_type: PositionType::Relative,
                }
                Children [
                    ( chart_grid_scene(height, palette) ),
                    (
                        Node {
                            width: percent(100),
                            height: percent(100),
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            top: px(0.0),
                            overflow: Overflow::clip_x(),
                        }
                        Children [
                            ( { polyline_scene(&segments, color) } ),
                        ]
                    ),
                ]
            )
        ]
    })
}

fn sidebar_activity_scene(
    icon: taskmanager_ui_contract::IconId,
    samples: &[f32],
    color: bevy::color::Color,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let height = palette.control_height_px * 1.35;
    let graph = activity_graph_scene(samples, color, height, palette);
    Box::new(bsn! {
        Node {
            width: px(palette.control_height_px * 3.0),
            height: px(height),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_2()),
        }
        Children [
            ( { crate::icons::icon_scene(icon, 16.0, palette.dim_color) } ),
            (
                Node {
                    width: px(palette.control_height_px * 2.0),
                    height: px(height),
                }
                Children [
                    ( { graph } ),
                ]
            ),
        ]
    })
}

fn sidebar_curve_scene(
    icon: taskmanager_ui_contract::IconId,
    curve: SystemCurve,
    shell: &ShellApp,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    sidebar_activity_scene(
        icon,
        &curve_samples(shell, curve),
        curve.color(palette),
        palette,
    )
}

fn disk_sidebar_title(disk: &DiskMetrics) -> String {
    if !disk.model.is_empty() {
        disk.model.clone()
    } else if !disk.name.is_empty() {
        disk.name.clone()
    } else {
        disk.device_id.clone()
    }
}

fn disk_sidebar_caption(disk: &DiskMetrics) -> String {
    let rate = |value: Option<u64>| value.map_or_else(missing_value, bytes);
    [
        disk.current_active_time_pct()
            .map_or_else(missing_value, |value| format!("{value:.0}%")),
        rate(disk.current_read_bytes_per_sec()),
        rate(disk.current_write_bytes_per_sec()),
    ]
    .join(" · ")
}

pub(super) fn device_sidebar_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let mut rows: Vec<Box<dyn Scene>> = vec![Box::new(device_button_scene(
        PerformanceDeviceTarget::Cpu,
        Box::new(device_row_with_accessory_scene(
            t("common.cpu").to_owned(),
            sidebar_curve_scene(
                taskmanager_ui_contract::IconId::Cpu,
                SystemCurve::Cpu,
                shell,
                palette,
            ),
            cpu_device_caption_scene(shell),
            true,
            palette,
        )),
    )) as Box<dyn Scene>];
    rows.push(Box::new(device_button_scene(
        PerformanceDeviceTarget::Memory,
        Box::new(device_row_with_accessory_scene(
            t("common.memory").to_owned(),
            sidebar_curve_scene(
                taskmanager_ui_contract::IconId::Memory,
                SystemCurve::Memory,
                shell,
                palette,
            ),
            marked_text_scene(
                summary_value(shell, SummaryField::Memory),
                Role::Caption,
                DynField::Summary(SummaryField::Memory),
            ),
            false,
            palette,
        )),
    )));
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
            rows.push(Box::new(device_button_scene(
                PerformanceDeviceTarget::Disk(disk.device_id.clone()),
                Box::new(device_row_with_accessory_scene(
                    disk_sidebar_title(disk),
                    sidebar_activity_scene(
                        taskmanager_ui_contract::IconId::Disk,
                        &samples,
                        palette.accent,
                        palette,
                    ),
                    Box::new(bsn! {
                        Text(disk_sidebar_caption(disk))
                        TextRole(Role::Mono)
                        template_value(no_wrap_text())
                    }),
                    false,
                    palette,
                )),
            )));
        }
    }
    if let Some(devices) = network_devices(shell) {
        for nic in devices {
            let key = (*nic.device_id).to_owned();
            let samples = shell
                .history
                .network_bytes_per_sec_for(&key, nic.device_generation.get());
            rows.push(Box::new(device_button_scene(
                PerformanceDeviceTarget::Network(key.clone()),
                Box::new(device_row_with_accessory_scene(
                    if nic.interface_name.is_empty() {
                        key.clone()
                    } else {
                        (*nic.interface_name).to_owned()
                    },
                    sidebar_activity_scene(
                        taskmanager_ui_contract::IconId::Network,
                        &samples,
                        palette.accent,
                        palette,
                    ),
                    marked_text_scene(
                        nic_fact_line(nic),
                        Role::Mono,
                        DynField::Device {
                            section: Section::Network,
                            device: key,
                        },
                    ),
                    false,
                    palette,
                )),
            )));
        }
    }
    if let Some(devices) = gpu_devices(shell) {
        for gpu in devices {
            let key = gpu.device_id.clone();
            let samples = shell
                .history
                .gpu_usage_pct_for(&key, gpu.device_generation.get());
            rows.push(Box::new(device_button_scene(
                PerformanceDeviceTarget::Gpu(key.clone()),
                Box::new(device_row_with_accessory_scene(
                    gpu_block_title(gpu),
                    sidebar_activity_scene(
                        taskmanager_ui_contract::IconId::Gpu,
                        &samples,
                        palette.accent,
                        palette,
                    ),
                    marked_text_scene(
                        gpu_fact_line(gpu),
                        Role::Mono,
                        DynField::Device {
                            section: Section::Gpu,
                            device: key,
                        },
                    ),
                    false,
                    palette,
                )),
            )));
        }
    }
    let mut panel_children: Vec<Box<dyn Scene>> = vec![Box::new(bsn! {
        Text(t("sidebar.devices"))
        TextRole(Role::Caption)
    }) as Box<dyn Scene>];
    panel_children.extend(rows);
    let panel = surface_scene(SurfaceTone::Content, panel_children, palette);
    bsn! {
        Node {
            width: px(WIDE_DEVICE_SIDEBAR_WIDTH_PX),
            min_width: px(WIDE_DEVICE_SIDEBAR_WIDTH_PX),
            height: percent(100),
            flex_shrink: 0.0,
            overflow: Overflow::scroll_y(),
        }
        PerformanceDeviceSidebar
        ScrollArea
        Children [
            ( { panel } ),
        ]
    }
}

pub(super) fn stats_rail_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let mut rows = summary_rows(shell);
    for (label, field) in [
        (t("cpu.frequency"), CpuField::Frequency),
        (t("common.temperature"), CpuField::Temperature),
        (t("common.power"), CpuField::Power),
    ] {
        rows.push(Box::new(stat_row_scene(
            label.to_owned(),
            marked_text_scene(
                cpu_field_text(shell, field),
                Role::Mono,
                DynField::Cpu(field),
            ),
            palette,
        )));
    }
    if let Some(cpu) = cpu_metrics(shell) {
        for (label, value) in [
            (
                t("system_about.cores"),
                cpu.physical_cores.map(|cores| cores.to_string()),
            ),
            (
                t("common.logical_cores"),
                cpu.logical_cores.map(|cores| cores.to_string()),
            ),
        ] {
            let display_value = value.unwrap_or_else(missing_value);
            rows.push(Box::new(stat_row_scene(
                label.to_owned(),
                Box::new(bsn! {
                    Text(display_value)
                    TextRole(Role::Mono)
                }),
                palette,
            )));
        }
    }
    // Rail contents end at the measured facts. The former "window"/"source"
    // filler rows were hardcoded English constants — fabricated captions,
    // not facts — and are gone with this cleanup pass.
    let panel = summary_card_scene(rows, palette);
    bsn! {
        Node {
            width: px(WIDE_STATS_WIDTH_PX),
            min_width: px(WIDE_STATS_WIDTH_PX),
            height: percent(100),
            flex_shrink: 0.0,
            overflow: Overflow::scroll_y(),
        }
        PerformanceStatsRail
        ScrollArea
        Children [
            ( { panel } ),
        ]
    }
}

fn summary_card_scene(rows: Vec<Box<dyn Scene>>, palette: &UiPalette) -> impl Scene + use<> {
    surface_scene(SurfaceTone::Elevated, rows, palette)
}

fn metric_selector_button_scene(curve: SystemCurve, palette: &UiPalette) -> impl Scene + use<> {
    let active = curve == SystemCurve::default();
    let pill = pill_scene(curve_selector_label(curve), active, palette);
    bsn! {
        ( { pill } PerformanceFocusButton(curve) on(focus_button_activated) )
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
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_4())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            { buttons },
        ]
    }
}

fn summary_rows(shell: &ShellApp) -> Vec<Box<dyn Scene>> {
    [
        (t("common.cpu"), SummaryField::Cpu),
        (t("common.cores"), SummaryField::Cores),
        (t("common.memory"), SummaryField::Memory),
        (t("mem.swap"), SummaryField::Swap),
        (t("net.receive"), SummaryField::NetReceive),
        (t("net.send"), SummaryField::NetSend),
    ]
    .into_iter()
    .map(|(label, field)| {
        Box::new(fact_row(
            label.to_owned(),
            summary_value(shell, field),
            DynField::Summary(field),
        )) as Box<dyn Scene>
    })
    .collect()
}
