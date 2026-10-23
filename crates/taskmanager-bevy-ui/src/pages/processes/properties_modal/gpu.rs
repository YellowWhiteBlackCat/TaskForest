//! The GPU dedicated-memory and per-engine micro bars of the insights facet.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, Node, Overflow, percent, px,
};
use bevy::ui::widget::Text;

use crate::palette::{UiPalette, space_4, space_8, space_24};
use crate::window::{Role, TextRole};
use taskmanager_application::{ProcessInsightFacetState, ProjectedProcessInsights, i18n::t};
use taskmanager_core::core::process_telemetry::ProcessGpuSnapshot;
use taskmanager_shell::presentation::bytes;

/// What one GPU micro bar measures.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GpuBarKind {
    /// The dedicated memory a device holds for this process.
    #[default]
    DeviceMemory,
    /// One device's own utilization.
    DeviceUsage,
    /// One engine class's load.
    EngineUsage,
}

/// The marker of a rendered GPU micro bar.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuBarMark(pub(crate) GpuBarKind);

/// One measured GPU micro bar.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GpuBar {
    /// What the bar measures.
    pub(crate) kind: GpuBarKind,
    /// The identity of the measured subject.
    pub(crate) subject: String,
    /// The measured readout.
    pub(crate) value: String,
    /// The measured share of the bar, in 0.0..=1.0.
    ///
    /// Absent when the source did not measure the subject: a cold-start
    /// engine has no rate yet, and a device that reports no dedicated
    /// memory has no share. An unmeasured subject draws no bar at all.
    pub(crate) share: Option<f32>,
}

/// The GPU micro bars of one process.
///
/// The dedicated-memory bar measures a device's share of the process's own
/// GPU memory: the per-process projection carries no device-wide VRAM total,
/// so the process's total is the only honest denominator. The engine bar
/// rides the observed single-core-equivalent rate.
pub(super) fn gpu_bars(projection: Option<&ProjectedProcessInsights>) -> Vec<GpuBar> {
    let Some(ProcessInsightFacetState::Current(gpu)) = projection.map(|p| &p.gpu) else {
        return Vec::new();
    };
    gpu_bars_of(gpu)
}

/// The GPU micro bars of one measured GPU snapshot.
fn gpu_bars_of(gpu: &ProcessGpuSnapshot) -> Vec<GpuBar> {
    let total: u64 = gpu
        .devices
        .iter()
        .filter_map(|device| device.memory_bytes)
        .sum();
    let mut bars = Vec::new();
    for device in &gpu.devices {
        let memory = device.memory_bytes;
        bars.push(GpuBar {
            kind: GpuBarKind::DeviceMemory,
            subject: format!("{} · {}", device.device_id, t("proc_insights.vram")),
            value: memory
                .map(bytes)
                .unwrap_or_else(|| t("proc_insights.unknown").to_owned()),
            share: memory.map(|value| {
                if total == 0 {
                    0.0
                } else {
                    value as f32 / total as f32
                }
            }),
        });
        bars.push(GpuBar {
            kind: GpuBarKind::DeviceUsage,
            subject: format!("{} · {}", device.device_id, t("proc_insights.gpu_usage")),
            value: device
                .utilization_pct
                .map(|value| format!("{value:.1}%"))
                .unwrap_or_else(|| t("proc_insights.unknown").to_owned()),
            share: device
                .utilization_pct
                .map(|value| (value / 100.0).clamp(0.0, 1.0)),
        });
    }
    for engine in &gpu.engines.engines {
        let rate = engine.usage_pct.current_value().copied();
        bars.push(GpuBar {
            kind: GpuBarKind::EngineUsage,
            subject: engine.name.clone(),
            value: rate
                .map(|value| format!("{value:.1}%"))
                .unwrap_or_else(|| t("proc_insights.unknown").to_owned()),
            share: rate.map(|value| (value / 100.0).clamp(0.0, 1.0)),
        });
    }
    bars
}

/// The micro bar of one measured GPU subject.
fn bar_scene(bar: &GpuBar, palette: &UiPalette) -> impl Scene + use<> {
    let ink = match bar.kind {
        GpuBarKind::EngineUsage => palette.accent,
        GpuBarKind::DeviceMemory => palette.memory,
        GpuBarKind::DeviceUsage => palette.cpu,
    };
    let kind = bar.kind;
    let share = bar.share.unwrap_or(0.0);
    let fill = Box::new(bsn! {
        Node { flex_grow: share, flex_basis: px(0.0), flex_shrink: 0.0, min_width: px(0.0), height: percent(100) }
        GpuBarMark(kind)
        BackgroundColor({ ink })
    }) as Box<dyn Scene>;
    bsn! {
        Node { width: px(space_24()), height: px(space_8()), flex_direction: FlexDirection::Row,
            overflow: Overflow::clip(), flex_shrink: 0.0,
            border_radius: BorderRadius::all(px(palette.control_radius_px)) }
        BackgroundColor({ palette.content_bg })
        Children [ @{ fill } ]
    }
}

/// One measured GPU row: the subject, its micro bar and its readout.
fn row_scene(bar: &GpuBar, palette: &UiPalette) -> impl Scene + use<> {
    let subject = bar.subject.clone();
    let value = bar.value.clone();
    bsn! {
        Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center, column_gap: px(space_8()), flex_shrink: 0.0 }
        Children [
            Node { flex_grow: 1.0, min_width: px(0.0), overflow: Overflow::clip_x() }
            Children [ Text({ subject }) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } ] --
            @{ bar_scene(bar, palette) } --
            Text({ value }) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    }
}

/// The GPU micro-bar panel of the insights facet.
pub(super) fn gpu_panel_scene(bars: &[GpuBar], palette: &UiPalette) -> impl Scene + use<> {
    let rows: Vec<Box<dyn Scene>> = bars
        .iter()
        .map(|bar| Box::new(row_scene(bar, palette)) as Box<dyn Scene>)
        .collect();
    bsn! {
        Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column,
            row_gap: px(space_4()), flex_shrink: 0.0 }
        Children [
            Text({ t("common.gpu").to_owned() }) TextRole(Role::Caption) --
            Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column,
                row_gap: px(space_4()), flex_shrink: 0.0 }
            Children [{ rows }]
        ]
    }
}
