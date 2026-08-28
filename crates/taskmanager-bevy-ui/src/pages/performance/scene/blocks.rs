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

pub(super) fn section_scene(
    section: Section,
    shell: &ShellApp,
    palette: &UiPalette,
) -> impl Scene + use<> {
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
