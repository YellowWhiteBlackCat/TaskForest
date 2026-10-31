//! The storage surface of the Performance page's disk family.
//!
//! The disk family carries facts a caption cannot: the per-partition
//! capacity share (the ring), the device's SMART verdict (the badge), the
//! wear/temperature gauge, the filesystem alarms, and the guidance a missing
//! SMART tool needs. This module renders them and owns their motion.
//!
//! Two laws shape the code:
//!
//! - **the shared chart family owns the drawing**: a ring and a gauge are
//!   one continuous path each, stroked by the owned canvas painter. No second
//!   renderer is grown here.
//! - **absence is honest**: a partition without a measured capacity draws an
//!   empty track, a disk without a SMART verdict draws no plate, and a disk
//!   without wear facts draws no gauge. Nothing is drawn as zero.
//!
//! The surface is a bounded subtree of the disk's own card: it is mounted,
//! refreshed in place and retired by the systems below, never rebuilt for a
//! telemetry tick.

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, ResMut};
use bevy::math::Vec2;
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::widget::Text;
use bevy::ui::{
    AlignItems, BackgroundColor, BorderRadius, Display, FlexDirection, Node, Overflow,
    PositionType, UiRect, Val, percent, px, rem,
};
use bevy::ui_widgets::{Activate, Button};

use crate::icons::icon_scene;
use crate::pages::performance::metrics::{
    MountAlert, NvmeGauge, PartitionDonut, SmartHealthBadge, SmartToolGuidance, WearBand,
    mount_alerts, nvme_gauge_model, partition_donut_models, smart_health_badge, smart_health_label,
    smart_tool_guidance,
};
use crate::palette::{UiPalette, space_2, space_4, space_8};
use crate::text_selection::ClipboardPort;
use crate::widgets::chart::geometry::ChartVertex;
use crate::widgets::chart::{ChartCanvas, ChartContent, ChartPaintState, ChartStyle};
use crate::window::{Role, TextRole};

use taskforest_core::core::metrics::{DiskMetrics, DiskPartition};
use taskforest_core::core::storage_health::FilesystemHealthSnapshot;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::{bytes, missing_value};
use taskmanager_ui_contract::IconId;

/// The ring's design box (chart geometry, never a layout length): the canvas
/// re-projects these points onto the slot the layout gave it.
const RING_DESIGN: f32 = 100.0;
/// The ring's radius inside the design box.
const RING_RADIUS: f32 = 38.0;
/// The ring's stroke width, in px (chart geometry).
const RING_THICKNESS_PX: f32 = 6.0;
/// How many segments a full ring is drawn with.
const RING_SEGMENTS: usize = 48;
/// The angle the capacity ring starts at: the top, the way a dial reads.
const RING_START: f32 = -std::f32::consts::FRAC_PI_2;
/// The gauge's start angle: the left end of the half circle.
const GAUGE_START: f32 = std::f32::consts::PI;
/// The gauge's sweep: half a circle.
const GAUGE_SWEEP: f32 = std::f32::consts::PI;
/// The share of a capacity above which the ring wears the warning ink.
const CAPACITY_WARNING_SHARE: f32 = 0.9;
/// How long the expand/collapse motion takes, in seconds.
const EXPAND_SECONDS: f32 = 0.18;

/// The vertices of an arc of the ring, in the ring's design box.
///
/// `sweep` is the angle the arc covers; a full circle closes on itself. The
/// path is continuous: the ring is one stroked path, never a stack of
/// segments.
fn arc_vertices(start: f32, sweep: f32) -> Vec<ChartVertex> {
    let steps = ((RING_SEGMENTS as f32) * (sweep.abs() / std::f32::consts::TAU))
        .round()
        .max(1.0);
    let steps = steps as usize;
    (0..=steps)
        .map(|index| {
            let angle = start + (index as f32 / steps as f32) * sweep;
            ChartVertex {
                x: RING_DESIGN / 2.0 + RING_RADIUS * angle.cos(),
                y: RING_DESIGN / 2.0 + RING_RADIUS * angle.sin(),
            }
        })
        .collect()
}

/// The ring's canvas: the track path and the used path, stacked.
fn ring_canvas_scene(content: ChartContent, ink: Color, thickness: f32) -> Box<dyn Scene> {
    let style = ChartStyle {
        line: ink,
        thickness,
        fill: None,
        grid: None,
        flow: None,
        format: None,
    };
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            overflow: Overflow::clip(),
        }
        ChartCanvas { content: {content}, style: {style} }
        ChartPaintState
    })
}

/// The identity of one mounted storage surface.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StorageSurface {
    /// The disk the surface renders.
    pub(crate) disk: String,
    /// The structural fingerprint of the rendered facts.
    pub(crate) signature: u64,
}

/// Where the expand/collapse motion is heading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExpandTarget {
    /// The surface grows to its natural height.
    #[default]
    Expanding,
    /// The surface shrinks to nothing and is retired.
    Collapsing,
}

/// The expand/collapse motion of one surface.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct StorageExpand {
    /// The animated share of the surface's natural height (0.0..=1.0).
    pub(crate) progress: f32,
    /// Where the motion is heading.
    pub(crate) target: ExpandTarget,
    /// The measured natural height of the surface, in px.
    pub(crate) natural_height: Option<f32>,
}

/// One partition's used arc: the disk, the partition, and the canvas.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StorageDonutArc {
    /// The disk the partition belongs to.
    pub(crate) disk: String,
    /// The partition's stable key inside the disk.
    pub(crate) partition: String,
}

/// One partition's readout: the disk and the partition it reads.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StorageDonutText {
    /// The disk the partition belongs to.
    pub(crate) disk: String,
    /// The partition's stable key inside the disk.
    pub(crate) partition: String,
}

/// The wear gauge's used arc.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StorageGaugeArc(pub(crate) String);

/// The wear gauge's readout.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StorageGaugeText(pub(crate) String);

/// The command a guidance card copies to the clipboard.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CopyCommand(pub(crate) String);

/// The surfaces that were retired this frame and are despawned at its end.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct StorageRetired(pub(crate) Vec<Entity>);

/// Advance the expand/collapse motion by one frame.
///
/// The motion is a typed state: a caller reads the progress, never a sleep.
pub(crate) fn advance_expand(expand: &mut StorageExpand, delta: f32) {
    let step = (delta / EXPAND_SECONDS).clamp(0.0, 1.0);
    expand.progress = match expand.target {
        ExpandTarget::Expanding => (expand.progress + step).min(1.0),
        ExpandTarget::Collapsing => (expand.progress - step).max(0.0),
    };
}

/// The used share of one partition, or `None` when it published no capacity.
fn partition_fraction(part: &DiskPartition) -> Option<f32> {
    let used = part.current_used_bytes()?;
    let capacity = part.current_capacity_bytes()?;
    if capacity == 0 {
        return None;
    }
    Some((used as f32 / capacity as f32).clamp(0.0, 1.0))
}

/// The partition's used/capacity readout, or the shared dash when the
/// partition published no capacity.
fn partition_readout(part: &DiskPartition) -> String {
    match (part.current_used_bytes(), part.current_capacity_bytes()) {
        (Some(used), Some(capacity)) if capacity > 0 => {
            let share = (used as f32 / capacity as f32).clamp(0.0, 1.0);
            // The share is a FRACTION; the readout states it in percent.
            format!(
                "{} / {} ({:.0}%)",
                bytes(used),
                bytes(capacity),
                share * 100.0
            )
        }
        _ => missing_value(),
    }
}

/// The ring's ink for a used share: the accent, or the warning ink once the
/// partition crosses the shared capacity warning share.
fn share_ink(fraction: Option<f32>, palette: &UiPalette) -> Color {
    match fraction {
        Some(fraction) if fraction >= CAPACITY_WARNING_SHARE => palette.warning_color,
        _ => palette.accent,
    }
}

/// The wear band's ink.
fn band_ink(band: WearBand, palette: &UiPalette) -> Color {
    match band {
        WearBand::Good => palette.success_color,
        WearBand::Warning => palette.warning_color,
        WearBand::Critical => palette.danger_color,
    }
}

/// The filesystem alarms of one partition's mount point.
///
/// The shared alarm model is the authority; an unreported mount raises
/// nothing (an honest absence, never an invented alarm).
fn partition_mount_alerts(
    part: &DiskPartition,
    filesystems: Option<&FilesystemHealthSnapshot>,
) -> Vec<MountAlert> {
    mount_alerts(filesystems, &part.mount_point)
}

/// The SMART health badge: the plate and the shared status label.
///
/// A disk without a verdict draws no plate at all: an unavailable provider
/// is not a healthy disk.
fn badge_scene(disk: &DiskMetrics, palette: &UiPalette) -> Box<dyn Scene> {
    let badge = smart_health_badge(disk);
    let label = smart_health_label(disk);
    let (display, ink, icon) = match badge {
        SmartHealthBadge::Healthy => (Display::Flex, palette.success_color, IconId::CircleCheck),
        SmartHealthBadge::Warning => (Display::Flex, palette.warning_color, IconId::TriangleAlert),
        SmartHealthBadge::Unavailable => (Display::None, palette.dim_color, IconId::CircleCheck),
    };
    let plate = icon_scene(icon, palette, ink);
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
        }
        Children [
            Node { width: rem(palette.icon_rem), height: rem(palette.icon_rem), flex_shrink: 0.0, display: { display } }
            Children [ @{ plate } ] --
            Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    })
}

/// One partition's capacity ring: the track, the used arc, and the readout.
fn donut_scene(model: &PartitionDonut, disk: &str, palette: &UiPalette) -> Box<dyn Scene> {
    let partition = model.key.clone();
    let label = model.name.clone();
    let readout = model.usage_text.clone();
    let fraction = model.used_fraction;
    let track = ring_canvas_scene(
        ChartContent::Points {
            vertices: arc_vertices(RING_START, std::f32::consts::TAU),
            source: Vec2::splat(RING_DESIGN),
        },
        palette.dim_color,
        RING_THICKNESS_PX,
    );
    let used_content = ChartContent::Points {
        vertices: arc_vertices(RING_START, std::f32::consts::TAU * fraction.unwrap_or(0.0)),
        source: Vec2::splat(RING_DESIGN),
    };
    let used_style = ChartStyle {
        line: share_ink(fraction, palette),
        thickness: RING_THICKNESS_PX,
        fill: None,
        grid: None,
        flow: None,
        format: None,
    };
    let used = Box::new(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            overflow: Overflow::clip(),
        }
        ChartCanvas { content: {used_content}, style: {used_style} }
        ChartPaintState
        StorageDonutArc { disk: { disk.to_owned() }, partition: { partition.clone() } }
    });
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
            Node {
                width: rem(palette.control_height_rem * 1.6),
                height: rem(palette.control_height_rem * 1.6),
                flex_shrink: 0.0,
            }
            Children [ @{ track } -- @{ used } ] --
            Node { flex_grow: 1.0, min_width: px(0.0), overflow: Overflow::clip_x(), flex_direction: FlexDirection::Column, row_gap: Val::Px(space_2()) }
            Children [
                Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } --
                Text(readout) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::NoWrap } StorageDonutText { disk: { disk.to_owned() }, partition: { partition } }
            ]
        ]
    })
}

/// The wear/temperature gauge: the half ring, the used arc, and the readout.
fn gauge_scene(gauge: &NvmeGauge, disk: &str, palette: &UiPalette) -> Box<dyn Scene> {
    let used_share = gauge
        .used_percent
        .map(|used| (used / 100.0).clamp(0.0, 1.0));
    let track = ring_canvas_scene(
        ChartContent::Points {
            vertices: arc_vertices(GAUGE_START, GAUGE_SWEEP),
            source: Vec2::splat(RING_DESIGN),
        },
        palette.dim_color,
        RING_THICKNESS_PX,
    );
    let used_content = ChartContent::Points {
        vertices: arc_vertices(GAUGE_START, GAUGE_SWEEP * used_share.unwrap_or(0.0)),
        source: Vec2::splat(RING_DESIGN),
    };
    let used_style = ChartStyle {
        line: band_ink(gauge.band, palette),
        thickness: RING_THICKNESS_PX,
        fill: None,
        grid: None,
        flow: None,
        format: None,
    };
    let used = Box::new(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            overflow: Overflow::clip(),
        }
        ChartCanvas { content: {used_content}, style: {used_style} }
        ChartPaintState
        StorageGaugeArc({ disk.to_owned() })
    });
    let readout = gauge_readout(gauge);
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
            Node {
                width: rem(palette.control_height_rem * 2.0),
                height: rem(palette.control_height_rem * 1.2),
                flex_shrink: 0.0,
            }
            Children [ @{ track } -- @{ used } ] --
            Node { flex_grow: 1.0, min_width: px(0.0), overflow: Overflow::clip_x() }
            Children [ Text(readout) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::NoWrap } StorageGaugeText({ disk.to_owned() }) ]
        ]
    })
}

/// The gauge's readout: the endurance and the temperature, each only when it
/// was measured.
fn gauge_readout(gauge: &NvmeGauge) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(used) = gauge.used_percent {
        parts.push(format!("{} {used:.0}%", t("disk.endurance_used")));
    }
    if let Some(temperature) = gauge.temperature_c {
        parts.push(format!(
            "{} {temperature:.0}\u{b0}C",
            t("common.temperature")
        ));
    }
    if parts.is_empty() {
        return missing_value();
    }
    parts.join(" · ")
}

/// The missing-tool guidance card: the shared copy, the command, and the
/// one-click copy action.
fn guidance_scene(guidance: &SmartToolGuidance, palette: &UiPalette) -> Box<dyn Scene> {
    let title = guidance.title.clone();
    let hint = guidance.hint.clone();
    let command = guidance.command.clone();
    let copied = command.clone();
    let label = t("common.copy_command").to_owned();
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.content_bg })
        Children [
            Text(title) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } --
            Text(hint) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::WordBoundary } --
            Node { width: percent(100), flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(space_8()) }
            Children [
                Node { flex_grow: 1.0, min_width: px(0.0), overflow: Overflow::clip_x() }
                Children [ Text(command) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::NoWrap } ] --
                Node {
                    min_height: rem(palette.control_height_rem),
                    padding: UiRect::all(Val::Px(space_4())),
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BackgroundColor({ palette.panel_fill })
                Button CopyCommand({ copied }) on(copy_command_activated)
                Children [ Text(label) TextRole(Role::Caption) Pickable::IGNORE ]
            ]
        ]
    })
}

/// The copy action: the guidance card's command reaches the system
/// clipboard through the shared port.
fn copy_command_activated(
    activate: On<Activate>,
    commands: Query<&CopyCommand>,
    mut clipboard: Option<ResMut<ClipboardPort>>,
) {
    let Ok(command) = commands.get(activate.event().entity) else {
        return;
    };
    let Some(clipboard) = clipboard.as_mut() else {
        return;
    };
    clipboard.request_text(command.0.clone(), t("common.copy_command"));
}

/// The storage surface of one disk: the badge, the gauge, the partition
/// rings, the filesystem alarms, and the missing-tool guidance.
pub(crate) fn storage_surface_scene(
    disk: &DiskMetrics,
    filesystems: Option<&FilesystemHealthSnapshot>,
    palette: &UiPalette,
    signature: u64,
    progress: f32,
) -> Box<dyn Scene> {
    let mut children: Vec<Box<dyn Scene>> = vec![badge_scene(disk, palette)];
    if let Some(gauge) = nvme_gauge_model(disk) {
        children.push(gauge_scene(&gauge, &disk.device_id, palette));
    }
    for model in partition_donut_models(disk) {
        children.push(donut_scene(&model, &disk.device_id, palette));
    }
    for part in &disk.partitions {
        for alert in partition_mount_alerts(part, filesystems) {
            children.push(alarm::mount_alert_scene(&alert, &part.mount_point, palette));
        }
    }
    if let Some(guidance) = smart_tool_guidance(disk) {
        children.push(guidance_scene(&guidance, palette));
    }
    let id = disk.device_id.clone();
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
            overflow: Overflow::clip_y(),
        }
        BackgroundColor({ palette.panel_fill })
        StorageSurface { disk: { id }, signature: { signature } }
        StorageExpand { progress: { progress } }
        Children [ { children } ]
    })
}

/// The structural fingerprint of one disk's storage surface.
///
/// Only the facts that change the surface's STRUCTURE belong here: a changed
/// number is written into the mounted surface in place, so a telemetry tick
/// never rebuilds the card.
pub(crate) fn storage_signature(
    disk: &DiskMetrics,
    filesystems: Option<&FilesystemHealthSnapshot>,
) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut mix = |value: u64| {
        hash ^= value;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    };
    mix(disk.device_id.len() as u64);
    mix(smart_health_badge(disk) as u64);
    mix(smart_tool_guidance(disk).is_some() as u64);
    mix(nvme_gauge_model(disk).is_some() as u64);
    for part in &disk.partitions {
        mix(part.name.len() as u64);
        mix(part
            .name
            .as_bytes()
            .iter()
            .map(|byte| u64::from(*byte))
            .sum());
        for alert in partition_mount_alerts(part, filesystems) {
            mix(alert as u64);
        }
    }
    hash
}

pub(crate) mod alarm;
pub(crate) mod sync;
pub(crate) use sync::register;

#[cfg(test)]
#[path = "../../../../tests/headless/pages/storage_parity.rs"]
mod storage_parity_tests;
