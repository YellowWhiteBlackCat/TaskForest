//! Memory composition bar and segment row scene builders.

use super::super::super::metrics::{segment_bytes_value, segment_key};
use super::super::super::{DynField, Section};
use crate::palette::{UiPalette, space_2, space_4, space_8};
use crate::window::{Role, TextRole};
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::system::Query;
use bevy::scene::{Scene, bsn};
use bevy::ui::BackgroundColor;
use bevy::ui::prelude::{
    AlignItems, BorderRadius, Display, FlexDirection, JustifyContent, Node, Overflow, Val, percent,
    px, rem,
};
use bevy::ui::widget::Text;
use bevy::window::{PrimaryWindow, Window};
use taskforest_core::core::metrics::MemoryMetrics;
use taskmanager_shell::ShellApp;
use taskmanager_shell::memory::{MemSegment, MemSegmentKind, memory_segments};
use taskmanager_ui_contract::IconId;

pub(super) const LEGEND_SWATCH_SIZE_REM: f32 = 0.625;
pub(super) const SEGMENT_BAR_HEIGHT_REM: f32 = 0.875;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceOptionalMemoryLegend;

pub(crate) fn sync_memory_legend_layout(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut legends: Query<&mut Node, With<PerformanceOptionalMemoryLegend>>,
) {
    let height = windows.iter().next().map_or(780.0, Window::height);
    let display = if crate::widgets::layout::memory_legend_visible(height) {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut legends {
        if node.display != display {
            node.display = display;
        }
    }
}

pub(super) fn memory_legend_container_scene(rows: Vec<Box<dyn Scene>>) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            display: Display::None,
        }
        PerformanceOptionalMemoryLegend
        Children [ { rows } ]
    })
}

pub(super) fn section_header(
    section: Section,
    title: &'static str,
    shell: &ShellApp,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    if section == Section::MemorySegments
        && let Some(summary) = crate::pages::performance::metrics::memory_section_summary(shell)
    {
        let icon = crate::icons::icon_scene(IconId::Performance, palette, palette.dim_color);
        return Box::new(bsn! {
            Node { width: percent(100), flex_direction: FlexDirection::Row, align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween }
            Children [
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(space_4()) }
                Children [ @{ icon } -- Text(title) TextRole(Role::Caption) ] --
                Text(summary) TextRole(Role::Caption)
            ]
        });
    }
    Box::new(bsn! { Text(title) TextRole(Role::Caption) })
}

pub(super) fn segment_row_scene(
    shell: &ShellApp,
    segment: &MemSegment,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let key = segment_key(segment.kind);
    let label = segment.label.to_owned();
    let memory = crate::pages::performance::metrics::memory_segments_total(shell);
    let segment_percent = memory
        .filter(|total| *total > 0)
        .map(|total| (segment.bytes as f32 / total as f32 * 100.0).clamp(0.0, 100.0))
        .unwrap_or(0.0);
    let value = segment_bytes_value(shell, segment.kind);
    let kind = segment.kind;
    let color = segment_color(kind, palette);
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        crate::pages::performance::DynBlock(Section::MemorySegments, key)
        Children [
            Node {
                width: rem(LEGEND_SWATCH_SIZE_REM),
                height: rem(LEGEND_SWATCH_SIZE_REM),
                border_radius: BorderRadius::all(rem(LEGEND_SWATCH_SIZE_REM / 2.0)),
            }
            BackgroundColor(color)
            --
            Text(label) TextRole(Role::Caption) --
            Node { flex_grow: 1.0 } --
            Node {
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(space_8()),
            }
            Children [
                Node { width: rem(2.75), flex_shrink: 0.0 } Children [ Text(format!("{segment_percent:>4.0}%")) TextRole(Role::Caption) ] --
                Text(value) TextRole(Role::Mono) crate::pages::performance::DynText(DynField::Segment(kind))
            ]
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SegmentSpan {
    pub(crate) bytes: u64,
    pub(crate) fraction: f32,
}

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

fn segment_color(kind: MemSegmentKind, palette: &UiPalette) -> bevy::color::Color {
    match kind {
        MemSegmentKind::Active | MemSegmentKind::InUse => palette.memory,
        // The cache wears a tinted disk fill and the ARC a dimmer one, so the
        // two reclaimable legends never blur into a single entry.
        MemSegmentKind::Cache => crate::palette::with_alpha(palette.disk, 0.85),
        MemSegmentKind::ZfsArc => crate::palette::with_alpha(palette.disk, 0.55),
        MemSegmentKind::Inactive => crate::palette::with_alpha(palette.accent, 0.55),
        MemSegmentKind::Free | MemSegmentKind::Available => {
            crate::palette::with_alpha(palette.dim_color, 0.30)
        }
        MemSegmentKind::Other => palette.shade,
    }
}

pub(crate) fn segment_bar_scene(memory: &MemoryMetrics, palette: &UiPalette) -> impl Scene + use<> {
    let segments = memory_segments(memory);
    let visible: Vec<_> = segment_bar_layout(&segments)
        .into_iter()
        .zip(segments.iter())
        .filter(|(span, _)| span.bytes > 0)
        .collect();
    let spans: Vec<Box<dyn Scene>> = visible
        .iter()
        .enumerate()
        .map(|(index, (span, segment))| {
            let color = segment_color(segment.kind, palette);
            let start = if index == 0 {
                rem(SEGMENT_BAR_HEIGHT_REM / 2.0)
            } else {
                px(0.0)
            };
            let end = if index + 1 == visible.len() {
                rem(SEGMENT_BAR_HEIGHT_REM / 2.0)
            } else {
                px(0.0)
            };
            Box::new(bsn! {
                Node {
                    width: percent(span.fraction * 100.0),
                    height: percent(100.0),
                    border_radius: BorderRadius::new(start, end, end, start),
                }
                BackgroundColor(color)
            }) as Box<dyn Scene>
        })
        .collect();
    bsn! {
        Node {
            width: percent(100),
            height: rem(SEGMENT_BAR_HEIGHT_REM),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(0.0),
            overflow: Overflow::clip_x(),
            border_radius: BorderRadius::all(rem(SEGMENT_BAR_HEIGHT_REM / 2.0)),
        }
        BackgroundColor({ palette.content_bg })
        Children [
            { spans }
        ]
    }
}

pub(crate) fn swap_bar_scene(shell: &ShellApp, palette: &UiPalette) -> Option<Box<dyn Scene>> {
    let (used_fraction, free_fraction) =
        crate::pages::performance::metrics::swap_bar_fractions(shell)?;
    Some(Box::new(bsn! {
        Node {
            width: percent(100),
            height: rem(0.5),
            flex_direction: FlexDirection::Row,
            overflow: Overflow::clip_x(),
            border_radius: BorderRadius::all(rem(0.25)),
        }
        BackgroundColor({ crate::palette::with_alpha(palette.dim_color, 0.22) })
        Children [
            Node { width: percent(used_fraction * 100.0), height: percent(100) }
            BackgroundColor({ crate::palette::with_alpha(palette.network, 0.9) })
            --
            Node { width: percent(free_fraction * 100.0), height: percent(100) }
            BackgroundColor({ crate::palette::with_alpha(palette.dim_color, 0.22) })
        ]
    }))
}
