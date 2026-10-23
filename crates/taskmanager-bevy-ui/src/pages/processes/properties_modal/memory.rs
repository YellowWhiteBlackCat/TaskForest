//! The segmented physical/swap memory comparison bar of the overview.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, FlexWrap, Node, Overflow, percent, px,
};
use bevy::ui::widget::Text;

use crate::palette::{UiPalette, space_4, space_8, space_12};
use crate::window::{Role, TextRole};
use taskmanager_application::i18n::t;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_shell::presentation::bytes;

/// One measured segment of the memory comparison bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MemorySegment {
    /// The process's unique (private) physical memory.
    #[default]
    Unique,
    /// The process's proportional share of shared physical memory.
    Shared,
    /// The process's swapped-out memory.
    Swap,
}

/// The marker of a rendered memory segment.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MemorySegmentMark(pub(crate) MemorySegment);

/// The measured memory segments of one process.
///
/// Every segment is an observation, never a derived zero: a segment the
/// platform did not measure stays absent and is simply not drawn, so the bar
/// never claims a share the kernel did not report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MemorySegments {
    /// The unique (private) physical bytes.
    pub(crate) unique_bytes: Option<u64>,
    /// The proportional share of shared physical bytes.
    pub(crate) shared_bytes: Option<u64>,
    /// The swapped-out bytes.
    pub(crate) swap_bytes: Option<u64>,
}

impl MemorySegments {
    /// The measured bytes of one segment.
    pub(crate) fn bytes(&self, segment: MemorySegment) -> Option<u64> {
        match segment {
            MemorySegment::Unique => self.unique_bytes,
            MemorySegment::Shared => self.shared_bytes,
            MemorySegment::Swap => self.swap_bytes,
        }
    }

    /// The measured total of the present segments, in bytes.
    pub(crate) fn total_bytes(&self) -> u64 {
        self.unique_bytes
            .unwrap_or(0)
            .saturating_add(self.shared_bytes.unwrap_or(0))
            .saturating_add(self.swap_bytes.unwrap_or(0))
    }

    /// Whether at least one segment was measured.
    pub(crate) fn is_measured(&self) -> bool {
        self.unique_bytes.is_some() || self.shared_bytes.is_some() || self.swap_bytes.is_some()
    }

    /// The share of one measured segment, in 0.0..=1.0.
    ///
    /// The denominator is the measured total, so the segments of one bar
    /// always add up to the whole bar.
    pub(crate) fn share(&self, segment: MemorySegment) -> Option<f32> {
        let value = self.bytes(segment)?;
        let total = self.total_bytes();
        if total == 0 {
            return Some(0.0);
        }
        Some(value as f32 / total as f32)
    }
}

/// The measured memory segments of one process.
///
/// The unique set is the process's private physical memory; the shared share
/// is the proportional part of the resident set that other processes also
/// map; the swap is the process's swapped-out memory.
pub(super) fn memory_segments(process: &ProcessItem) -> MemorySegments {
    MemorySegments {
        unique_bytes: process.current_memory_uss_bytes(),
        shared_bytes: process.current_memory_proportional_shared_bytes(),
        swap_bytes: process.current_swap_bytes(),
    }
}

/// The ink one memory segment paints.
fn segment_ink(segment: MemorySegment, palette: &UiPalette) -> bevy::color::Color {
    match segment {
        MemorySegment::Unique => palette.memory,
        MemorySegment::Shared => palette.dim_color,
        MemorySegment::Swap => palette.warning_color,
    }
}

/// The label one memory segment carries.
fn segment_label(segment: MemorySegment) -> &'static str {
    match segment {
        MemorySegment::Unique => t("proc.uss"),
        MemorySegment::Shared => t("proc.shared"),
        MemorySegment::Swap => t("proc.swap"),
    }
}

/// The segmented bar of the measured memory segments.
///
/// Every segment claims its exact share of the bar: the flex growth is the
/// measured fraction, so the drawn proportions are the measured ones.
fn bar_scene(segments: MemorySegments, palette: &UiPalette) -> impl Scene + use<> {
    let mut parts: Vec<Box<dyn Scene>> = Vec::new();
    for segment in [
        MemorySegment::Unique,
        MemorySegment::Shared,
        MemorySegment::Swap,
    ] {
        if segments.bytes(segment).is_none() {
            continue;
        }
        let share = segments.share(segment).unwrap_or(0.0);
        let ink = segment_ink(segment, palette);
        parts.push(Box::new(bsn! {
            Node { flex_grow: share, flex_basis: px(0.0), flex_shrink: 0.0, min_width: px(0.0), height: percent(100) }
            MemorySegmentMark(segment)
            BackgroundColor({ ink })
        }));
    }
    bsn! {
        Node { width: percent(100), height: px(space_12()), flex_direction: FlexDirection::Row,
            overflow: Overflow::clip(), border_radius: BorderRadius::all(px(palette.control_radius_px)) }
        BackgroundColor({ palette.content_bg })
        Children [{ parts }]
    }
}

/// The legend of the comparison bar: one chip, label and readout per segment.
fn legend_scene(segments: MemorySegments, palette: &UiPalette) -> Vec<Box<dyn Scene>> {
    [
        MemorySegment::Unique,
        MemorySegment::Shared,
        MemorySegment::Swap,
    ]
    .into_iter()
    .filter_map(|segment| {
        let value = segments.bytes(segment)?;
        let ink = segment_ink(segment, palette);
        let label = segment_label(segment).to_owned();
        Some(Box::new(bsn! {
            Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center,
                column_gap: px(space_4()), flex_shrink: 0.0 }
            Children [
                Node { width: px(space_8()), height: px(space_8()), flex_shrink: 0.0 }
                BackgroundColor({ ink }) --
                Text({ label }) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } --
                Text({ bytes(value) }) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap }
            ]
        }) as Box<dyn Scene>)
    })
    .collect()
}

/// The memory comparison panel: the segmented bar over its legend.
pub(super) fn memory_panel_scene(
    segments: MemorySegments,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let legend = legend_scene(segments, palette);
    bsn! {
        Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column,
            row_gap: px(space_4()), flex_shrink: 0.0 }
        Children [
            @{ bar_scene(segments, palette) } --
            Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap,
                row_gap: px(space_8()), column_gap: px(space_8()), flex_shrink: 0.0 }
            Children [{ legend }]
        ]
    }
}
