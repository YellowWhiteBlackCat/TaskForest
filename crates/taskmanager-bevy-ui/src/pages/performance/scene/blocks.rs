//! Dynamic GPU, network, memory, and section scene builders.

use super::*;
use crate::pages::performance::metrics::{
    batteries, battery_fact_line, disk_partition_view_models, disks, gpu_vram_view_model,
};
use crate::palette::space_2;
use taskmanager_core::core::power::BatteryInfo;

pub(super) fn gpu_block_title(gpu: &GpuMetrics) -> String {
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
                ( Text({ vram.label }) TextRole(Role::Caption) template_value(no_wrap_text()) ),
                (
                    Node {
                        width: percent(100),
                        height: px(6.0),
                        border_radius: BorderRadius::all(Val::Px(space_2())),
                        overflow: Overflow::clip_x(),
                    }
                    BackgroundColor({ palette.panel_fill })
                    Children [
                        (
                            Node {
                                width: percent(vram.pct),
                                height: percent(100.0),
                                border_radius: BorderRadius::all(Val::Px(space_2())),
                            }
                            BackgroundColor({ palette.accent })
                        )
                    ]
                ),
            ]
        }) as Box<dyn Scene>);
    }

    for engine in &gpu.engines {
        let name = engine.name.clone();
        let pct = engine.usage_pct.clamp(0.0, 100.0);
        let usage = format!("{pct:.1}%");
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
                (
                    Node {
                        width: px(80.0),
                        overflow: Overflow::clip_x(),
                    }
                    Children [ ( Text(name) TextRole(Role::Caption) template_value(no_wrap_text()) ) ]
                ),
                (
                    Node {
                        flex_grow: 1.0,
                        min_width: px(60.0),
                        height: px(6.0),
                        border_radius: BorderRadius::all(Val::Px(space_2())),
                        overflow: Overflow::clip_x(),
                    }
                    BackgroundColor({ palette.panel_fill })
                    Children [
                        (
                            Node {
                                width: percent(pct),
                                height: percent(100.0),
                                border_radius: BorderRadius::all(Val::Px(space_2())),
                            }
                            BackgroundColor({ palette.nav_active_bg })
                        )
                    ]
                ),
                (
                    Node {
                        width: px(50.0),
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::FlexEnd,
                    }
                    Children [ ( Text(usage) TextRole(Role::Mono) template_value(no_wrap_text()) ) ]
                ),
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
            ( Text({ gpu_block_title(gpu) }) TextRole(Role::Body) ),
            ( Text({ gpu_fact_line(gpu) }) TextRole(Role::Mono) DynText(field) ),
            { details },
        ]
    }
}

fn nic_block_scene(nic: &NetworkMetrics, palette: &UiPalette) -> impl Scene + use<> {
    let title = if nic.interface_name.is_empty() {
        (*nic.device_id).to_owned()
    } else {
        (*nic.interface_name).to_owned()
    };
    let mut details: Vec<Box<dyn Scene>> = Vec::new();
    if let Some(ipv4) = &nic.ipv4_addr {
        let val = format!("IPv4: {ipv4}");
        details.push(Box::new(bsn! {
            Text(val) TextRole(Role::Caption) template_value(no_wrap_text())
        }) as Box<dyn Scene>);
    }
    if let Some(mac) = &nic.mac_addr {
        let val = format!("MAC: {mac}");
        details.push(Box::new(bsn! {
            Text(val) TextRole(Role::Caption) template_value(no_wrap_text())
        }) as Box<dyn Scene>);
    }
    if let Some(driver) = &nic.driver {
        let val = format!("{}: {driver}", t("common.driver"));
        details.push(Box::new(bsn! {
            Text(val) TextRole(Role::Caption) template_value(no_wrap_text())
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
            ( Text(title) TextRole(Role::Body) ),
            ( Text(nic_fact_line(nic)) TextRole(Role::Mono) DynText(field) ),
            { details },
        ]
    }
}

fn disk_block_scene(disk: &DiskMetrics, palette: &UiPalette) -> impl Scene + use<> {
    let title = if !disk.model.is_empty() {
        disk.model.clone()
    } else if !disk.name.is_empty() {
        disk.name.clone()
    } else {
        disk.device_id.clone()
    };
    let mut partition_rows: Vec<Box<dyn Scene>> = Vec::new();
    for part in disk_partition_view_models(disk) {
        partition_rows.push(Box::new(bsn! {
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(space_2()),
                padding: UiRect::vertical(Val::Px(space_2())),
            }
            Children [
                (
                    Node {
                        width: percent(100),
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                    }
                    Children [
                        ( Text({ part.name }) TextRole(Role::Caption) template_value(no_wrap_text()) ),
                        ( Text({ part.usage_text }) TextRole(Role::Mono) template_value(no_wrap_text()) ),
                    ]
                ),
                (
                    Node {
                        width: percent(100),
                        height: px(6.0),
                        border_radius: BorderRadius::all(Val::Px(space_2())),
                        overflow: Overflow::clip_x(),
                    }
                    BackgroundColor({ palette.panel_fill })
                    Children [
                        (
                            Node {
                                width: percent(part.pct),
                                height: percent(100.0),
                                border_radius: BorderRadius::all(Val::Px(space_2())),
                            }
                            BackgroundColor({ palette.accent })
                        )
                    ]
                ),
            ]
        }) as Box<dyn Scene>);
    }

    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.content_bg })
        DynBlock(Section::Disk, { disk.device_id.clone() })
        Children [
            ( Text(title) TextRole(Role::Body) ),
            ( { super::disk_caption_scene(disk, palette) } ),
            { partition_rows },
        ]
    }
}

fn battery_block_scene(
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
    device_block(
        Section::Battery,
        battery.id.clone(),
        title,
        battery_fact_line(battery),
        palette,
    )
}

fn segment_row_scene(
    shell: &ShellApp,
    segment: &MemSegment,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let key = segment_key(segment.kind);
    let label = segment.label.to_owned();
    let value = segment_value(shell, segment.kind);
    let kind = segment.kind;
    let color = segment_color(kind, palette);
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        DynBlock(Section::MemorySegments, key)
        Children [
            (
                Node {
                    width: px(10.0),
                    height: px(10.0),
                    border_radius: BorderRadius::all(Val::Px(space_2())),
                }
                BackgroundColor(color)
            ),
            ( Text(label) TextRole(Role::Caption) ),
            ( Node { flex_grow: 1.0 } ),
            ( Text(value) TextRole(Role::Mono) DynText(DynField::Segment(kind)) ),
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
            .map(|gpu| Box::new(gpu_block_scene(gpu, palette)) as Box<dyn Scene>),
        Section::Network => network_devices(shell)?
            .iter()
            .find(|nic| &*nic.device_id == key)
            .map(|nic| Box::new(nic_block_scene(nic, palette)) as Box<dyn Scene>),
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
            .map(|disk| Box::new(disk_block_scene(disk, palette)) as Box<dyn Scene>),
        Section::Battery => batteries(shell)?
            .iter()
            .enumerate()
            .find(|(_, b)| b.id == key)
            .map(|(idx, b)| Box::new(battery_block_scene(b, idx, palette)) as Box<dyn Scene>),
    }
}

fn section_title(section: Section) -> &'static str {
    match section {
        Section::Gpu => t("common.gpu"),
        Section::Network => t("sidebar.network"),
        Section::MemorySegments => t("mem.composition"),
        Section::Disk => t("common.disk"),
        Section::Battery => t("common.battery"),
    }
}

pub(super) fn section_scene(
    section: Section,
    shell: &ShellApp,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = section_title(section).to_owned();
    let mut children: Vec<Box<dyn Scene>> = Vec::new();
    if section == Section::MemorySegments
        && let Some(memory) = memory_metrics(shell)
    {
        children.push(Box::new(segment_bar_scene(memory, palette)) as Box<dyn Scene>);
    }
    children.extend(
        section_keys(shell, section)
            .iter()
            .filter_map(|key| block_scene(section, key, shell, palette)),
    );
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
            { children },
        ]
    }
}

/// One stacked-bar span: byte count plus the resolved fraction of the total.
/// Pure; headless tests pin the math.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SegmentSpan {
    pub(crate) bytes: u64,
    pub(crate) fraction: f32,
}

/// Fractions across the composition segments, in shell order. A zero total
/// (nothing measured yet) yields an empty layout — never NaN widths.
#[must_use]
pub(crate) fn segment_bar_layout(segments: &[MemSegment]) -> Vec<SegmentSpan> {
    let total: u64 = segments.iter().map(|segment| segment.bytes).sum();
    if total == 0 {
        return Vec::new();
    }
    segments
        .iter()
        .map(|segment| SegmentSpan {
            bytes: segment.bytes,
            fraction: segment.bytes as f32 / total as f32,
        })
        .collect()
}

/// The semantic token for one segment role. Roles map onto the palette's
/// semantic surfaces — no literal product colors (the theme owns every ink).
fn segment_color(kind: MemSegmentKind, palette: &UiPalette) -> bevy::color::Color {
    match kind {
        MemSegmentKind::Active | MemSegmentKind::InUse => palette.accent,
        MemSegmentKind::Cache | MemSegmentKind::ZfsArc => palette.nav_active_bg,
        MemSegmentKind::Inactive => palette.selection_bg,
        MemSegmentKind::Free | MemSegmentKind::Available => palette.content_bg,
        MemSegmentKind::Other => palette.hover_bg,
    }
}

/// The stacked composition bar: one flex-weighted span per segment, in shell
/// order, filling the full width. Zero measured bytes render an empty track.
pub(crate) fn segment_bar_scene(memory: &MemoryMetrics, palette: &UiPalette) -> impl Scene + use<> {
    let segments = memory_segments(memory);
    let spans: Vec<Box<dyn Scene>> = segment_bar_layout(&segments)
        .iter()
        .zip(segments.iter())
        .map(|(span, segment)| {
            let color = segment_color(segment.kind, palette);
            Box::new(bsn! {
                Node {
                    width: percent(span.fraction * 100.0),
                    height: percent(100.0),
                }
                BackgroundColor(color)
            }) as Box<dyn Scene>
        })
        .collect();
    bsn! {
        Node {
            width: percent(100),
            height: px(14.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(0.0),
            overflow: Overflow::clip_x(),
            border_radius: BorderRadius::all(Val::Px(space_4())),
        }
        BackgroundColor({ palette.content_bg })
        Children [
            { spans },
        ]
    }
}
