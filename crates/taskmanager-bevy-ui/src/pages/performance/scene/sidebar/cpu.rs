//! CPU-focused Performance scene builders.

use super::super::blocks::section_scene;
use super::super::chart::curve_card_scene;
use super::*;

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

pub(super) fn device_button_scene(
    target: PerformanceDeviceTarget,
    row: Box<dyn Scene>,
) -> impl Scene + use<> {
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

pub(crate) fn cpu_main_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
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
