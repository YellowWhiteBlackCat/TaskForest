//! Dynamic GPU, network, memory, and section scene builders.

use super::*;
use crate::pages::performance::device_curves::storage::StorageExpand;
use crate::pages::performance::device_curves::{
    self, DeviceCurve, DeviceCurveKind, DeviceCurveStatus,
};
use crate::pages::performance::metrics::{
    batteries, battery_fact_line, disks, fan_fact_line, fans, gpu_block_title, gpu_vram_view_model,
    smart_source_guidance, smart_source_status,
};
use crate::palette::space_2;
use crate::widgets::chart::CurveMeasurement;
use crate::widgets::layout::PERFORMANCE_CHART_SLOT_HEIGHT_REM;
use bevy::text::{LineBreak, TextLayout};
use taskforest_core::core::power::BatteryInfo;
use taskforest_core::core::sensors::SensorReading;
use taskmanager_shell::gpu_chart_metric_gate;
use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetricChoiceState;

use super::super::gpu_metric::gpu_metric_button_activated;
use crate::glass::{GlassPanelMaterial, OpticalGlassCard, card_material};
use taskmanager_theme::GlassTier;

pub(crate) mod memory;
use memory::{memory_legend_container_scene, section_header, segment_row_scene, swap_bar_scene};
pub(crate) use memory::{segment_bar_scene, sync_memory_legend_layout};
use taskforest_core::core::percent::ClampedPercent;
use taskforest_core::core::percent::percent_text;

/// The GPU engine row's fixed column widths, in rem.
///
/// The row is three columns — the engine's name, its usage bar and the
/// numeric readout — and every column is a bounded slot, so a long engine
/// name clips instead of pushing the bar out of the card. The slots ride
/// the rem axis with the card around them.
const ENGINE_NAME_SLOT_WIDTH_REM: f32 = 5.0;
const USAGE_BAR_FLOOR_WIDTH_REM: f32 = 3.75;
const USAGE_READOUT_SLOT_WIDTH_REM: f32 = 3.125;

/// The thickness of a usage bar, in rem.
///
/// A usage bar carries a proportion, not a label, so it stays a hairline
/// track — but it still rides the rem axis, so a larger reading size gives
/// it a proportionally thicker track.
const USAGE_BAR_HEIGHT_REM: f32 = 0.375;

/// The shared GPU chart-metric selector (ADR-034): one button per vocabulary
/// family in fixed order, the selected family carrying the active fill and
/// unavailable families staying visible but dimmed. Activation drives the
/// shell's single selection authority.
fn gpu_metric_selector_scene(
    shell: &ShellApp,
    gpu: &GpuMetrics,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let gate = gpu_chart_metric_gate(Some(gpu));
    let projection = shell.gpu_chart_metric_projection(&gate);
    let mut buttons: Vec<Box<dyn Scene>> = Vec::new();
    for choice in projection.choices.iter() {
        let selected = matches!(
            choice.state,
            GpuChartMetricChoiceState::Selected | GpuChartMetricChoiceState::SelectedUnavailable
        );
        let available = !matches!(choice.state, GpuChartMetricChoiceState::Unavailable);
        let fill = if selected {
            palette.nav_active_bg
        } else {
            palette.content_bg
        };
        let label = t(choice.metric.label_key()).to_owned();
        let metric = choice.metric;
        buttons.push(Box::new(bsn! {
            Node {
                padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_2())),
                border_radius: BorderRadius::all(rem(palette.control_radius_rem())),
            }
            BackgroundColor(fill)
            Button GpuMetricButton({ metric }) on(gpu_metric_button_activated)
            Children [ Text(label) TextRole(Role::Caption) ]
        }) as Box<dyn Scene>);
        let _ = available;
    }
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_4()),
            flex_wrap: FlexWrap::Wrap,
        }
        Children [ { buttons } ]
    }
}

/// The selectable middle graph: one curve bound to the shared selected family
/// for this adapter. The label is a dynamic marker so a selection change
/// rewrites it on the next fold; the curve reads the same shared window the
/// selector gate derives from.
fn gpu_metric_curve_scene(
    shell: &ShellApp,
    gpu: &GpuMetrics,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let id = gpu.device_id.clone();
    let generation = gpu.device_generation;
    let label = t(shell.gpu_chart_metric_selected().label_key()).to_owned();
    let status = DeviceCurve {
        kind: DeviceCurveKind::GpuMetric,
        id: id.clone(),
        generation,
        color: palette.accent,
    };
    bsn! {
        Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: Val::Px(space_4()) }
        Children [
            Text(label) TextRole(Role::Caption) DynText(DynField::GpuMetricLabel({ id.clone() })) --
            Node { width: percent(100), height: rem(PERFORMANCE_CHART_SLOT_HEIGHT_REM), flex_shrink: 0.0, overflow: Overflow::clip() }
            DeviceCurve { kind: DeviceCurveKind::GpuMetric, id: { id.clone() }, generation: { generation }, color: { palette.accent } } CurveMeasurement(None) Children [] --
            Text(t("perf.collecting_samples")) TextRole(Role::Caption) DeviceCurveStatus({ status })
        ]
    }
}

fn gpu_block_scene(shell: &ShellApp, gpu: &GpuMetrics, palette: &UiPalette) -> impl Scene + use<> {
    let mut details: Vec<Box<dyn Scene>> = Vec::new();

    if let Some(vram) = gpu_vram_view_model(gpu) {
        details.push(Box::new(bsn! {
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(space_2()),
                padding: UiRect::vertical(Val::Px(space_2())),
            }
            Children [
                 Text({ vram.label }) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } --

                    Node {
                        width: percent(100),
                        height: rem(USAGE_BAR_HEIGHT_REM),
                        border_radius: BorderRadius::all(rem(USAGE_BAR_HEIGHT_REM / 2.0)),
                        overflow: Overflow::clip_x(),
                    }
                    BackgroundColor({ palette.panel_fill })
                    Children [

                            Node {
                                width: percent(vram.pct),
                                height: percent(100.0),
                                border_radius: BorderRadius::all(rem(USAGE_BAR_HEIGHT_REM / 2.0)),
                            }
                            BackgroundColor({ palette.accent })

                    ]

            ]
        }) as Box<dyn Scene>);
    }

    for engine in &gpu.engines {
        let name = engine.name.clone();
        let pct = engine.usage_pct.clamped_percent();
        let usage = percent_text(pct);
        details.push(Box::new(bsn! {
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: Val::Px(space_8()),
                padding: UiRect::vertical(Val::Px(space_2())),
            }
            Children [

                    Node {
                        width: rem(ENGINE_NAME_SLOT_WIDTH_REM),
                        overflow: Overflow::clip_x(),
                    }
                    Children [  Text(name) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }  ]
                --

                    Node {
                        flex_grow: 1.0,
                        min_width: rem(USAGE_BAR_FLOOR_WIDTH_REM),
                        height: rem(USAGE_BAR_HEIGHT_REM),
                        border_radius: BorderRadius::all(rem(USAGE_BAR_HEIGHT_REM / 2.0)),
                        overflow: Overflow::clip_x(),
                    }
                    BackgroundColor({ palette.panel_fill })
                    Children [

                            Node {
                                width: percent(pct),
                                height: percent(100.0),
                                border_radius: BorderRadius::all(rem(USAGE_BAR_HEIGHT_REM / 2.0)),
                            }
                            BackgroundColor({ palette.nav_active_bg })

                    ]
                --

                    Node {
                        width: rem(USAGE_READOUT_SLOT_WIDTH_REM),
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::FlexEnd,
                    }
                    Children [  Text(usage) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::NoWrap }  ]

            ]
        }) as Box<dyn Scene>);
    }

    let field = DynField::Device {
        section: Section::Gpu,
        device: gpu.device_id.clone(),
    };

    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.content_bg })
        DynBlock(Section::Gpu, { gpu.device_id.clone() })
        Children [
             Text({ gpu_block_title(gpu) }) TextRole(Role::Body) --
             Text({ gpu_fact_line(gpu) }) TextRole(Role::Mono) DynText(field) --
             @gpu_metric_selector_scene(shell, gpu, palette) --
             @gpu_metric_curve_scene(shell, gpu, palette) --
            { details }
        ]
    }
}

fn nic_block_scene(
    shell: &ShellApp,
    nic: &NetworkMetrics,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = if nic.interface_name.is_empty() {
        (*nic.device_id).to_owned()
    } else {
        (*nic.interface_name).to_owned()
    };
    let mut details: Vec<Box<dyn Scene>> = Vec::new();
    if let Some(ipv4) = &nic.ipv4_addr {
        let val = format!("IPv4: {ipv4}");
        details.push(Box::new(bsn! {
            Text(val) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
        }) as Box<dyn Scene>);
    }
    if let Some(mac) = &nic.mac_addr {
        let val = format!("MAC: {mac}");
        details.push(Box::new(bsn! {
            Text(val) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
        }) as Box<dyn Scene>);
    }
    if let Some(driver) = &nic.driver {
        let val = format!("{}: {driver}", t("common.driver"));
        details.push(Box::new(bsn! {
            Text(val) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
        }) as Box<dyn Scene>);
    }

    let field = DynField::Device {
        section: Section::Network,
        device: (*nic.device_id).to_owned(),
    };

    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.content_bg })
        DynBlock(Section::Network, { (*nic.device_id).to_owned() })
        Children [
             Text(title) TextRole(Role::Body) --
             Text(nic_fact_line(nic)) TextRole(Role::Mono) DynText(field) --
             @device_curves::scene(DeviceCurveKind::NetworkThroughput, &nic.device_id, nic.device_generation, shell, palette) --
            { details }
        ]
    }
}

fn disk_block_scene(
    shell: &ShellApp,
    disk: &DiskMetrics,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = if !disk.model.is_empty() {
        disk.model.clone()
    } else if !disk.name.is_empty() {
        disk.name.clone()
    } else {
        disk.device_id.clone()
    };
    // The list item rides the SAME typed motion the storage surface owns:
    // a hot-plugged disk slides open, and a disk that leaves slides shut.
    // The progress is a component field advanced by one per-frame system —
    // never a sleep.
    let expand_progress = 0.0_f32;
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
            // The item clips its own content while it slides: a collapsing
            // row never leaves its children hanging outside its frame.
            overflow: Overflow::clip_y(),
        }
        BackgroundColor({ palette.content_bg })
        DynBlock(Section::Disk, { disk.device_id.clone() })
        StorageExpand { progress: { expand_progress } }
        Children [
             Text(title) TextRole(Role::Body) --
             Text(smart_source_status(disk)) TextRole(Role::Caption)
             DynText(DynField::SmartStatus({disk.device_id.clone()}))
             TextLayout { linebreak: LineBreak::WordBoundary } --
             Text(smart_source_guidance(disk)) TextRole(Role::Caption)
             DynText(DynField::SmartGuidance({disk.device_id.clone()}))
             TextLayout { linebreak: LineBreak::WordBoundary } --
             @{ super::disk_caption_scene(disk, palette) } --
             @device_curves::scene(DeviceCurveKind::DiskThroughput, &disk.device_id, disk.device_generation, shell, palette) --
             @device_curves::scene(DeviceCurveKind::DiskActive, &disk.device_id, disk.device_generation, shell, palette)
        ]
    }
}

fn battery_block_scene(
    shell: &ShellApp,
    battery: &BatteryInfo,
    index: usize,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = if !battery.model_name.trim().is_empty() {
        battery.model_name.trim().to_string()
    } else if !battery.display_name.trim().is_empty() {
        battery.display_name.trim().to_string()
    } else {
        format!("{} {index}", t("common.battery"))
    };
    let fan_rpm = shell.projection().sensors.as_ref().and_then(|s| {
        s.readings
            .iter()
            .find(|r| r.id().contains("fan") || r.label().to_lowercase().contains("fan"))
            .and_then(|r| r.current_number().map(|n| n as u64))
    });
    let mut fact = battery_fact_line(battery);
    if let Some(rpm) = fan_rpm {
        fact.push_str(&format!(" · {}: {rpm} RPM", t("fan.rpm")));
    }
    bsn! {
        Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: Val::Px(space_4()) }
        DynBlock(Section::Battery, { battery.id.clone() })
        Children [
            Text(title) TextRole(Role::Body) --
            Text(fact) TextRole(Role::Mono) DynText(DynField::Device { section: Section::Battery, device: { battery.id.clone() } }) --
            @device_curves::scene(DeviceCurveKind::BatteryCharge, &battery.id, battery.device_generation, shell, palette) --
            @device_curves::scene(DeviceCurveKind::BatteryPower, &battery.id, battery.device_generation, shell, palette)
        ]
    }
}

fn fan_block_scene(
    shell: &ShellApp,
    fan: &SensorReading,
    index: usize,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = if !fan.label().trim().is_empty() {
        fan.label().trim().to_string()
    } else {
        format!("{} {index}", t("common.fan"))
    };
    let fact = fan_fact_line(fan);
    let id = fan.id().to_owned();
    let generation = fan.device_generation();
    bsn! {
        Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: Val::Px(space_4()) }
        DynBlock(Section::Fan, { id.clone() })
        Children [
            Text(title) TextRole(Role::Body) --
            Text(fact) TextRole(Role::Mono) DynText(DynField::Device { section: Section::Fan, device: { id.clone() } }) --
            @device_curves::scene(DeviceCurveKind::FanRpm, &id, generation, shell, palette)
        ]
    }
}

/// One block for mount or refresh, keyed by the section's stable identity.
/// `None` when the key is no longer in the projection (a race the caller's
/// desired list makes unreachable).
pub(crate) fn block_scene(
    section: Section,
    key: &str,
    shell: &ShellApp,
    palette: &UiPalette,
) -> Option<Box<dyn Scene>> {
    match section {
        Section::Gpu => gpu_devices(shell)?
            .iter()
            .find(|gpu| gpu.device_id == key)
            .map(|gpu| Box::new(gpu_block_scene(shell, gpu, palette)) as Box<dyn Scene>),
        Section::Network => network_devices(shell)?
            .iter()
            .find(|nic| &*nic.device_id == key)
            .map(|nic| Box::new(nic_block_scene(shell, nic, palette)) as Box<dyn Scene>),
        Section::MemorySegments => memory_metrics(shell).and_then(|memory| {
            memory_segments(memory)
                .iter()
                .find(|segment| segment_key(segment.kind) == key)
                .map(|segment| {
                    Box::new(segment_row_scene(shell, segment, palette)) as Box<dyn Scene>
                })
        }),
        Section::Disk => disks(shell)?
            .iter()
            .find(|disk| disk.device_id == key)
            .map(|disk| Box::new(disk_block_scene(shell, disk, palette)) as Box<dyn Scene>),
        Section::Battery => batteries(shell)?
            .iter()
            .enumerate()
            .find(|(_, b)| b.id == key)
            .map(|(idx, b)| {
                Box::new(battery_block_scene(shell, b, idx, palette)) as Box<dyn Scene>
            }),
        Section::Fan => fans(shell)?
            .into_iter()
            .enumerate()
            .find(|(_, fan)| fan.id() == key)
            .map(|(idx, fan)| {
                Box::new(fan_block_scene(shell, fan, idx, palette)) as Box<dyn Scene>
            }),
    }
}

fn section_title(section: Section) -> &'static str {
    match section {
        Section::Gpu => t("common.gpu"),
        Section::Network => t("sidebar.network"),
        Section::MemorySegments => t("mem.composition"),
        Section::Disk => t("common.disk"),
        Section::Battery => t("common.battery"),
        Section::Fan => t("common.fan"),
    }
}

pub(super) fn section_scene(
    section: Section,
    shell: &ShellApp,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = section_title(section);
    let header = section_header(section, title, shell, palette);
    let mut children: Vec<Box<dyn Scene>> = Vec::new();
    if section == Section::MemorySegments
        && let Some(memory) = memory_metrics(shell)
    {
        children.push(Box::new(segment_bar_scene(memory, palette)) as Box<dyn Scene>);
    }
    let block_rows: Vec<Box<dyn Scene>> = section_keys(shell, section)
        .iter()
        .filter_map(|key| block_scene(section, key, shell, palette))
        .collect();
    if section == Section::MemorySegments {
        children.push(memory_legend_container_scene(block_rows));
    } else {
        children.extend(block_rows);
    }
    if section == Section::MemorySegments
        && let Some(swap_bar) = swap_bar_scene(shell, palette)
    {
        children.push(swap_bar);
    }
    let GlassPanelMaterial {
        fill,
        edge,
        shadows,
        edge_width_rem,
        radius_rem,
        grain,
    } = card_material(palette);
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Rem(palette.panel_content_inset_rem())),
            border: UiRect::all(Val::Rem(edge_width_rem)),
            border_radius: BorderRadius::all(Val::Rem(radius_rem)),
        }
        BackgroundColor({ fill })
        BorderColor::all(edge)
        bevy::ui::BoxShadow({ shadows })
        OpticalGlassCard {
            tier: {GlassTier::Card},
            enabled: true,
        }
        DynSection(section)
        Children [
             @{ grain } --
             @{ header } --
            { children }
        ]
    }
}
