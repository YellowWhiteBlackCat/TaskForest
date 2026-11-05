//! Apply selection to new and retained scenes through explicit component queries.
use super::scene::hover::CurveSeries;
use super::{
    CurveCard, DeviceCategoryKind, DeviceViewCategory, PerformanceDeviceButton,
    PerformanceDeviceFocus, PerformanceDeviceTarget, PerformanceFocus, PerformanceFocusButton,
};
use crate::app::ShellTrack;
use crate::glass::{OpticalBackdropSource, OpticalGlassCard};
use crate::widgets::controls::ControlVisual;
use crate::widgets::scene_paint::ScenePaint;
use bevy::app::{App, PostUpdate};
use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::{Added, Without};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, Res};
use bevy::ui::{Display, Node, UiSystems};

pub(super) fn register(app: &mut App) {
    app.add_systems(
        PostUpdate,
        (sync_curve_selection, sync_device_selection)
            .after(ScenePaint)
            .before(UiSystems::Layout),
    );
}

fn sync_curve_selection(
    track: ShellTrack,
    focus: Res<PerformanceFocus>,
    device_focus: Res<PerformanceDeviceFocus>,
    added: Query<Entity, Added<CurveCard>>,
    mut buttons: Query<(&PerformanceFocusButton, &mut ControlVisual)>,
    mut cards: Query<(&CurveCard, &mut Node, Option<&mut OpticalGlassCard>)>,
    mut optical_sources: Query<(&CurveSeries, &mut OpticalBackdropSource)>,
) {
    if !focus.is_changed() && !device_focus.is_changed() && added.is_empty() {
        return;
    }
    for (button, mut visual) in &mut buttons {
        visual.1 = button.0 == focus.0;
    }
    for (card, mut node, mut optical) in &mut cards {
        let is_focused = card.0 == focus.0;
        node.flex_grow = if is_focused { 2.0 } else { 1.0 };
        node.display = if device_focus.0.curve().is_some()
            && is_focused
            && super::curve_wanted(track.shell(), card.0)
        {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(ref mut optical) = optical {
            optical.enabled = is_focused;
        }
    }
    for (series, mut source) in &mut optical_sources {
        source.enabled = series.0 == focus.0;
    }
}

impl PerformanceDeviceTarget {
    pub(crate) fn category(&self) -> DeviceCategoryKind {
        match self {
            Self::Cpu => DeviceCategoryKind::Cpu,
            Self::Memory => DeviceCategoryKind::Memory,
            Self::Disk(_) => DeviceCategoryKind::Disk,
            Self::Network(_) => DeviceCategoryKind::Network,
            Self::Gpu(_) => DeviceCategoryKind::Gpu,
            Self::Battery(_) => DeviceCategoryKind::Battery,
            Self::Fan(_) => DeviceCategoryKind::Fan,
        }
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceHeaderTarget(pub(crate) PerformanceDeviceTarget);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceStatsRailTarget(pub(crate) PerformanceDeviceTarget);

impl PerformanceDeviceTarget {
    pub(crate) fn matches_target(&self, focus: &PerformanceDeviceTarget) -> bool {
        match (self, focus) {
            (Self::Cpu, Self::Cpu) => true,
            (Self::Memory, Self::Memory) => true,
            (Self::Disk(a), Self::Disk(b)) => a == b || a.is_empty() || b.is_empty(),
            (Self::Network(a), Self::Network(b)) => a == b || a.is_empty() || b.is_empty(),
            (Self::Gpu(a), Self::Gpu(b)) => a == b || a.is_empty() || b.is_empty(),
            (Self::Battery(a), Self::Battery(b)) => a == b || a.is_empty() || b.is_empty(),
            (Self::Fan(a), Self::Fan(b)) => a == b || a.is_empty() || b.is_empty(),
            _ => false,
        }
    }
}

fn sync_device_selection(
    focus: Res<PerformanceDeviceFocus>,
    added: Query<Entity, Added<DeviceViewCategory>>,
    added_rails: Query<Entity, Added<PerformanceStatsRailTarget>>,
    added_headers: Query<Entity, Added<PerformanceHeaderTarget>>,
    mut buttons: Query<(&PerformanceDeviceButton, &mut ControlVisual)>,
    mut categories: Query<
        (&DeviceViewCategory, &mut Node),
        (
            Without<PerformanceDeviceButton>,
            Without<PerformanceStatsRailTarget>,
            Without<PerformanceHeaderTarget>,
        ),
    >,
    mut rail_targets: Query<
        (&PerformanceStatsRailTarget, &mut Node),
        (
            Without<DeviceViewCategory>,
            Without<PerformanceHeaderTarget>,
        ),
    >,
    mut header_targets: Query<
        (&PerformanceHeaderTarget, &mut Node),
        (
            Without<DeviceViewCategory>,
            Without<PerformanceStatsRailTarget>,
        ),
    >,
) {
    if !focus.is_changed() && added.is_empty() && added_rails.is_empty() && added_headers.is_empty()
    {
        return;
    }
    for (button, mut visual) in &mut buttons {
        visual.1 = button.0 == focus.0;
    }
    for (category, mut node) in &mut categories {
        node.display = if category.0 == focus.0.category() {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (rail, mut node) in &mut rail_targets {
        node.display = if rail.0.matches_target(&focus.0) {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (header, mut node) in &mut header_targets {
        node.display = if header.0.matches_target(&focus.0) {
            Display::Flex
        } else {
            Display::None
        };
    }
}
