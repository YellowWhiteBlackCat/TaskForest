//! Apply selection to new and retained scenes through explicit component queries.
use super::{
    CurveCard, DeviceCategoryKind, DeviceViewCategory, PerformanceDeviceButton,
    PerformanceDeviceFocus, PerformanceDeviceTarget, PerformanceFocus, PerformanceFocusButton,
};
use crate::app::ShellTrack;
use crate::widgets::controls::ControlVisual;
use crate::widgets::scene_paint::ScenePaint;
use bevy::app::{App, PostUpdate};
use bevy::ecs::change_detection::DetectChanges;
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
    mut cards: Query<(&CurveCard, &mut Node)>,
) {
    if !focus.is_changed() && !device_focus.is_changed() && added.is_empty() {
        return;
    }
    for (button, mut visual) in &mut buttons {
        visual.1 = button.0 == focus.0;
    }
    for (card, mut node) in &mut cards {
        node.flex_grow = if card.0 == focus.0 { 2.0 } else { 1.0 };
        node.display = if device_focus.0.curve().is_some()
            && card.0 == focus.0
            && super::curve_wanted(track.shell(), card.0)
        {
            Display::Flex
        } else {
            Display::None
        };
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
        }
    }
}

fn sync_device_selection(
    focus: Res<PerformanceDeviceFocus>,
    added: Query<Entity, Added<DeviceViewCategory>>,
    mut buttons: Query<(&PerformanceDeviceButton, &mut ControlVisual)>,
    mut categories: Query<(&DeviceViewCategory, &mut Node), Without<PerformanceDeviceButton>>,
) {
    if !focus.is_changed() && added.is_empty() {
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
}
