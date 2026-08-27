//! Scene composition for the Performance page — the `bsn!` builders that
//! turn the page's pure view-model resolvers (in the parent module) into
//! the mounted UI tree. Split from the parent to respect the per-file source
//! budget; data folding, marker vocabulary, and the refresh observer stay
//! with the parent. `content` is the page-agent entry [`crate::app`] calls.

use super::*;
use crate::palette::{UiPalette, space_2_quarter, space_4, space_8, space_12};
use crate::widgets::controls::{
    ControlTone, ControlVisual, SurfaceTone, device_row_with_accessory_scene, graph_card_scene,
    per_core_cell_scene, pill_scene, stat_row_scene, surface_scene,
};
use crate::widgets::layout::{
    MAIN_GRAPH_MIN_WIDTH_PX, WIDE_DEVICE_SIDEBAR_WIDTH_PX, WIDE_STATS_WIDTH_PX,
};
use bevy::color::Alpha;
use bevy::scene::on;
use bevy::ui::prelude::{BackgroundColor, BorderRadius, FlexWrap, PositionType};
use bevy::ui_widgets::Button;

/// Bar height from a sparkline fraction against the density-scaled strip
/// height (two standard control heights).
pub(super) fn bar_height(fraction: f32, palette: &UiPalette) -> f32 {
    (fraction * palette.control_height_px * 2.0).max(1.0)
}

pub(super) fn bar_scene(height_px: f32, color: bevy::color::Color) -> impl Scene + use<> {
    let fill = color.with_alpha(0.22);
    bsn! {
        Node {
            flex_grow: 1.0,
            min_width: px(1.0),
            width: px(space_2()),
            height: px(height_px),
            position_type: PositionType::Relative,
        }
        BackgroundColor(fill)
        Children [
            (
                Node {
                    width: percent(100),
                    height: px(2.0),
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    top: px(0.0),
                }
                BackgroundColor(color)
            ),
        ]
    }
}

/// The strip's initial bars, one per warm sample (none while collecting).
fn curve_bars(
    curve: SystemCurve,
    fractions: &[f32],
    palette: &UiPalette,
) -> Vec<impl Scene + use<>> {
    let color = curve.color(palette);
    fractions
        .iter()
        .map(|fraction| bar_scene(bar_height(*fraction, palette), color))
        .collect()
}

fn curve_card_scene(
    curve: SystemCurve,
    shell: &ShellApp,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = curve.title();
    let caption = curve_caption(shell, curve);
    let strip_height = palette.control_height_px * 3.0;
    let samples = curve_samples(shell, curve);
    let fractions = if curve_warm(&samples) {
        bar_fractions(&samples)
    } else {
        Vec::new()
    };
    // GPUI's compact performance surface keeps one selected hero graph in
    // view. The selector swaps this card in place; hidden cards retain their
    // markers and remain cheap to refresh when selected.
    let display = if curve == SystemCurve::default() && curve_wanted(shell, curve) {
        Display::Flex
    } else {
        Display::None
    };
    let bars = curve_bars(curve, &fractions, palette);
    let overlay =
        (!curve_warm(&samples)).then(|| collecting_overlay_scene(caption.clone(), palette));
    let segment_count = line_segments(
        &shell.history.series(curve.series()),
        100.0,
        strip_height,
        MAX_CHART_POINTS,
    )
    .len();
    bsn! {
        Node {
            flex_grow: 1.0,
            min_width: px(palette.control_height_px * 6.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_2())),
            display: display,
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        CurveCard(curve)
        CurveGate(curve)
        Children [
            ( Text(title) TextRole(Role::Caption) ),
            (
                Node {
                    width: percent(100),
                    height: px(strip_height),
                    position_type: PositionType::Relative,
                }
                Children [
                    ( chart_grid_scene(strip_height, palette) ),
                    (
                        Node {
                            width: percent(100),
                            height: percent(100),
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            top: px(0.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::FlexEnd,
                            column_gap: Val::Px(space_2()),
                        }
                        SparkStrip(curve)
                        ChartSurface({ segment_count })
                        Children [
                            { bars },
                        ]
                    ),
                    ( { overlay } ),
                ]
            ),
            (
                Text(caption)
                TextRole(Role::Caption)
                DynText(DynField::CurveCaption(curve))
            ),
        ]
    }
}

fn grid_line_color(palette: &UiPalette) -> bevy::color::Color {
    palette.dim_color.with_alpha(0.18)
}

fn horizontal_grid_line(top: f32, palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            height: px(1.0),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(top),
        }
        BackgroundColor({ grid_line_color(palette) })
    }
}

fn vertical_grid_line(left: f32, palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            width: px(1.0),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: percent(left),
            top: px(0.0),
        }
        BackgroundColor({ grid_line_color(palette) })
    }
}

fn chart_grid_scene(height: f32, palette: &UiPalette) -> impl Scene + use<> {
    let horizontal: Vec<Box<dyn Scene>> = (1..=4)
        .map(|index| {
            Box::new(horizontal_grid_line(height * index as f32 / 5.0, palette)) as Box<dyn Scene>
        })
        .collect();
    let vertical: Vec<Box<dyn Scene>> = (1..=5)
        .map(|index| {
            Box::new(vertical_grid_line(index as f32 / 6.0 * 100.0, palette)) as Box<dyn Scene>
        })
        .collect();
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
        }
        Children [
            { horizontal },
            { vertical },
        ]
    }
}

fn collecting_overlay_scene(caption: String, palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        }
        Children [
            (
                Node {
                    padding: UiRect::horizontal(Val::Px(space_8())),
                    height: px(palette.control_height_px),
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BackgroundColor({ palette.content_bg })
                Children [
                    ( Text(caption) TextRole(Role::Caption) ),
                ]
            ),
        ]
    })
}

/// One label→value line; the value is the rewritable fact.
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
            ( Node { width: px(100.0) } Children [ ( Text(label) TextRole(Role::Caption) ) ] ),
            ( Text(value) TextRole(Role::Mono) DynText(field) ),
        ]
    }
}

fn marked_text_scene(value: String, role: Role, field: DynField) -> Box<dyn Scene> {
    Box::new(bsn! {
        Text(value)
        TextRole(role)
        DynText(field)
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
        }
        Children [
            (
                Text(cpu_field_text(shell, CpuField::Usage))
                TextRole(Role::Mono)
                DynText(DynField::Cpu(CpuField::Usage))
            ),
            ( Text(" · ") TextRole(Role::Caption) ),
            (
                Text(cpu_field_text(shell, CpuField::Frequency))
                TextRole(Role::Mono)
                DynText(DynField::Cpu(CpuField::Frequency))
            ),
        ]
    })
}

/// A bounded history graph shared by device rows and per-core cells. It is a
/// product accessory, not a second chart authority: samples come from the
/// same shell history window as the hero graph and an empty window remains
/// visually empty.
fn activity_graph_scene(
    samples: &[f32],
    color: bevy::color::Color,
    height: f32,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let height = height.max(1.0);
    let fractions = bar_fractions(samples);
    let start = fractions.len().saturating_sub(14);
    let bars: Vec<Box<dyn Scene>> = fractions[start..]
        .iter()
        .map(|fraction| {
            Box::new(bsn! {
                Node {
                    width: px(space_2_quarter()),
                    height: px((fraction * height).max(1.0)),
                }
                BackgroundColor(color)
            }) as Box<dyn Scene>
        })
        .collect();
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
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::FlexEnd,
                            column_gap: Val::Px(space_2_quarter()),
                        }
                        Children [
                            { bars },
                        ]
                    ),
                ]
            )
        ]
    })
}

fn sidebar_activity_scene(
    icon: &'static str,
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
            ( Text(icon) TextRole(Role::Caption) ),
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
    icon: &'static str,
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

fn device_sidebar_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let mut rows: Vec<Box<dyn Scene>> = vec![Box::new(device_button_scene(
        PerformanceDeviceTarget::Cpu,
        Box::new(device_row_with_accessory_scene(
            t("common.cpu").to_owned(),
            sidebar_curve_scene("◔", SystemCurve::Cpu, shell, palette),
            cpu_device_caption_scene(shell),
            true,
            palette,
        )),
    )) as Box<dyn Scene>];
    rows.push(Box::new(device_button_scene(
        PerformanceDeviceTarget::Memory,
        Box::new(device_row_with_accessory_scene(
            t("common.memory").to_owned(),
            sidebar_curve_scene("▤", SystemCurve::Memory, shell, palette),
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
                    sidebar_activity_scene("▤", &samples, palette.accent, palette),
                    Box::new(bsn! {
                        Text(disk_sidebar_caption(disk))
                        TextRole(Role::Mono)
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
                    sidebar_activity_scene("⌁", &samples, palette.accent, palette),
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
                    sidebar_activity_scene("◈", &samples, palette.accent, palette),
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

fn stats_rail_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
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
    rows.push(Box::new(stat_row_scene(
        t("perf.window").to_owned(),
        Box::new(bsn! {
            Text("60 seconds")
            TextRole(Role::Body)
        }),
        palette,
    )));
    rows.push(Box::new(stat_row_scene(
        t("health.source").to_owned(),
        Box::new(bsn! {
            Text("Live projection")
            TextRole(Role::Body)
        }),
        palette,
    )));
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

fn cpu_header_scene(shell: &ShellApp) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            (
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                }
                Children [
                    ( Text(t("common.cpu")) TextRole(Role::Heading) ),
                    ( Node { flex_grow: 1.0 } ),
                    (
                        Text(cpu_field_text(shell, CpuField::Brand))
                        TextRole(Role::Body)
                        DynText(DynField::Cpu(CpuField::Brand))
                    ),
                ]
            ),
            ( Text(t("cpu.utilization_over_60s")) TextRole(Role::Caption) ),
        ]
    }
}

fn cpu_metric_strip_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let metrics: Vec<Box<dyn Scene>> = [
        (t("common.utilization"), CpuField::Usage),
        (t("cpu.frequency"), CpuField::Frequency),
        (t("common.temperature"), CpuField::Temperature),
        (t("common.power"), CpuField::Power),
    ]
    .into_iter()
    .map(|(label, field)| {
        Box::new(cpu_metric_cell_scene(
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
            { metrics },
        ]
    }
}

fn core_value_scene(value: Box<dyn Scene>, samples: &[f32], palette: &UiPalette) -> Box<dyn Scene> {
    let graph = activity_graph_scene(
        samples,
        palette.accent,
        palette.control_height_px * 1.15,
        palette,
    );
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            ( { value } ),
            ( { graph } ),
        ]
    })
}

fn cpu_core_grid_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let core_history = shell.history.per_core_usage_series();
    let cells: Vec<Box<dyn Scene>> = cpu_metrics(shell)
        .map(|cpu| {
            (0..cpu.current_core_usage_len())
                .map(|index| {
                    let field = CpuField::Core(index);
                    let samples = core_history
                        .get(index)
                        .map_or(&[][..], |samples| samples.as_slice());
                    Box::new(per_core_cell_scene(
                        format!("Core {:02}", index + 1),
                        core_value_scene(
                            marked_text_scene(
                                cpu_field_text(shell, field),
                                Role::Mono,
                                DynField::Cpu(field),
                            ),
                            samples,
                            palette,
                        ),
                        palette,
                    )) as Box<dyn Scene>
                })
                .collect()
        })
        .unwrap_or_default();
    let cells = if cells.is_empty() {
        vec![Box::new(per_core_cell_scene(
            "Core".to_owned(),
            core_value_scene(
                Box::new(bsn! {
                    Text(missing_value())
                    TextRole(Role::Mono)
                }),
                &[],
                palette,
            ),
            palette,
        )) as Box<dyn Scene>]
    } else {
        cells
    };
    let body = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(space_2()),
            row_gap: Val::Px(space_2()),
        }
        Children [
            { cells },
        ]
    };
    graph_card_scene(
        t("common.cores").to_owned(),
        t("cpu.utilization_by_core").to_owned(),
        Box::new(body),
        palette,
    )
}

fn device_pill_scene(
    target: PerformanceDeviceTarget,
    label: String,
    active: bool,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            min_width: px(palette.control_height_px * 2.8),
            height: px(palette.control_height_px),
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
            ( Text(label) TextRole(Role::Body) ),
        ]
    }
}

fn device_button_scene(target: PerformanceDeviceTarget, row: Box<dyn Scene>) -> impl Scene + use<> {
    bsn! {
        ( { row } PerformanceDeviceButton(target) on(device_button_activated) )
    }
}

fn compact_device_pills_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
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
    let pills: Vec<Box<dyn Scene>> = labels
        .into_iter()
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
            { pills },
        ]
    }
}

fn cpu_main_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let cards: Vec<Box<dyn Scene>> = SystemCurve::STRIP
        .iter()
        .map(|&curve| Box::new(curve_card_scene(curve, shell, palette)) as Box<dyn Scene>)
        .collect();
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
            overflow: Overflow::scroll_y(),
        }
        ScrollArea
        Children [
            ( compact_device_pills_scene(shell, palette) ),
            ( cpu_header_scene(shell) ),
            ( cpu_metric_strip_scene(shell, palette) ),
            ( metric_selector_scene(shell, palette) ),
            (
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                Children [
                    { cards },
                ]
            ),
            ( cpu_core_grid_scene(shell, palette) ),
            ( section_scene(Section::MemorySegments, shell, palette) ),
            ( section_scene(Section::Gpu, shell, palette) ),
            ( section_scene(Section::Network, shell, palette) ),
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

fn gpu_block_title(gpu: &GpuMetrics) -> String {
    let identity = gpu_display_identity(gpu);
    match (identity.headline, identity.qualifier) {
        (Some(headline), Some(qualifier)) => format!("{headline} ({qualifier})"),
        (Some(headline), None) => headline.to_owned(),
        (None, Some(qualifier)) => qualifier.to_owned(),
        (None, None) => gpu.device_id.clone(),
    }
}

/// One device block: identity line over the joined live fact line, keyed by
/// the stable device id the shell projection assigns.
fn device_block(
    section: Section,
    key: String,
    title: String,
    value: String,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let field = DynField::Device {
        section,
        device: key.clone(),
    };
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        BackgroundColor({ palette.content_bg })
        DynBlock(section, key)
        Children [
            ( Text(title) TextRole(Role::Body) ),
            ( Text(value) TextRole(Role::Mono) DynText(field) ),
        ]
    }
}

fn gpu_block_scene(gpu: &GpuMetrics, palette: &UiPalette) -> impl Scene + use<> {
    device_block(
        Section::Gpu,
        gpu.device_id.clone(),
        gpu_block_title(gpu),
        gpu_fact_line(gpu),
        palette,
    )
}

fn nic_block_scene(nic: &NetworkMetrics, palette: &UiPalette) -> impl Scene + use<> {
    // Identity is the interface name; a stable device id backs it up when
    // the projection has not resolved a name yet.
    let title = if nic.interface_name.is_empty() {
        (*nic.device_id).to_owned()
    } else {
        (*nic.interface_name).to_owned()
    };
    device_block(
        Section::Network,
        (*nic.device_id).to_owned(),
        title,
        nic_fact_line(nic),
        palette,
    )
}

fn segment_row_scene(segment: &MemSegment, memory: &MemoryMetrics) -> impl Scene + use<> {
    let key = segment_key(segment.kind);
    let label = segment.label.to_owned();
    let value = segment_line(segment, memory.current_total_bytes());
    let kind = segment.kind;
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_2()),
        }
        DynBlock(Section::MemorySegments, key)
        Children [
            ( Text(label) TextRole(Role::Caption) ),
            ( Text(value) TextRole(Role::Mono) DynText(DynField::Segment(kind)) ),
        ]
    }
}

/// One block for mount or refresh, keyed by the section's stable identity.
/// `None` when the key is no longer in the projection (a race the caller's
/// desired list makes unreachable).
pub(super) fn block_scene(
    section: Section,
    key: &str,
    shell: &ShellApp,
    palette: &UiPalette,
) -> Option<Box<dyn Scene>> {
    match section {
        Section::Gpu => gpu_devices(shell)?
            .iter()
            .find(|gpu| gpu.device_id == key)
            .map(|gpu| Box::new(gpu_block_scene(gpu, palette)) as Box<dyn Scene>),
        Section::Network => network_devices(shell)?
            .iter()
            .find(|nic| &*nic.device_id == key)
            .map(|nic| Box::new(nic_block_scene(nic, palette)) as Box<dyn Scene>),
        Section::MemorySegments => {
            let memory = memory_metrics(shell)?;
            memory_segments(memory)
                .iter()
                .find(|segment| segment_key(segment.kind) == key)
                .map(|segment| Box::new(segment_row_scene(segment, memory)) as Box<dyn Scene>)
        }
    }
}

fn section_title(section: Section) -> &'static str {
    match section {
        Section::Gpu => t("common.gpu"),
        Section::Network => t("sidebar.network"),
        Section::MemorySegments => t("mem.composition"),
    }
}

fn section_scene(section: Section, shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let title = section_title(section).to_owned();
    let blocks: Vec<Box<dyn Scene>> = section_keys(shell, section)
        .iter()
        .filter_map(|key| block_scene(section, key, shell, palette))
        .collect();
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_2())),
        }
        BackgroundColor({ palette.panel_fill })
        DynSection(section)
        Children [
            ( Text(title) TextRole(Role::Caption) ),
            { blocks },
        ]
    }
}

/// Content-region scene for the Performance page.
pub(crate) fn content(context: &PageContext<'_>) -> impl Scene + use<> {
    let shell = context.shell;
    let palette = context.palette;
    let devices = device_sidebar_scene(shell, palette);
    let main = cpu_main_scene(shell, palette);
    let stats = stats_rail_scene(shell, palette);
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Row,
            row_gap: Val::Px(space_2()),
            column_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_2())),
            overflow: Overflow::scroll_y(),
        }
        PerformancePageRoot
        ScrollArea
        Children [
            ( { devices } ),
            ( { main } ),
            ( { stats } ),
        ]
    }
}
