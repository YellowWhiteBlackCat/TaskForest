//! Dynamic GPU, network, memory, and section scene builders.

use super::*;

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

fn segment_row_scene(shell: &ShellApp, segment: &MemSegment) -> impl Scene + use<> {
    let key = segment_key(segment.kind);
    let label = segment.label.to_owned();
    // The legend line is folded by the data layer (`segment_value`), the same
    // read the fold observer replays into this row — never a scene-local copy.
    let value = segment_value(shell, segment.kind);
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
                .map(|segment| Box::new(segment_row_scene(shell, segment)) as Box<dyn Scene>)
        }),
    }
}

fn section_title(section: Section) -> &'static str {
    match section {
        Section::Gpu => t("common.gpu"),
        Section::Network => t("sidebar.network"),
        Section::MemorySegments => t("mem.composition"),
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
                    height: px(6.0),
                }
                BackgroundColor(color)
            }) as Box<dyn Scene>
        })
        .collect();
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(0.0),
            overflow: Overflow::clip_x(),
        }
        Children [
            { spans },
        ]
    }
}
