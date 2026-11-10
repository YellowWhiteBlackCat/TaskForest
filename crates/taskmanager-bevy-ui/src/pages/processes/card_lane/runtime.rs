//! Observers and measured systems for the Applications card lane.
//!
//! These systems stay inside Bevy's ECS: they read typed resources and
//! queries, never the raw `World`. Shell mutations go through the shell's
//! reducers, and a changed measured budget publishes the shared interaction
//! seam so the same virtual window rebuilds in the active presentation.

use bevy::app::{App, PostUpdate};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use bevy::ui::UiSystems;
use bevy::ui::prelude::ComputedNode;
use bevy::ui::widget::Text;
use bevy::ui_widgets::Activate;
use taskmanager_ui_contract::IconId;

use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;
use crate::icons::{IconInk, InlineIcon};
use crate::input::ShellInteractionApplied;
use crate::palette::UiPalette;
use crate::window::WindowPalette;

use super::{
    CardFlowBudget, ProcessFocusSortDir, ProcessFocusValue, ProcessGroupChip, ProcessViewMode,
    ProcessViewModeButton, ProcessViewModeChanged, card_flow_budget, focus_projection,
};
use crate::pages::processes::ProcessPageRoot;

/// Chip activation resolves the canonical filter from the activated entity,
/// then asks the shell to reduce the projection. The fold and interaction
/// seams re-materialize the same virtual window.
pub(crate) fn on_group_chip_activated(
    activate: On<Activate>,
    chips: Query<&ProcessGroupChip>,
    mut track: Option<NonSendMut<FrontendTrack>>,
    mut commands: Commands,
) {
    let Ok(chip) = chips
        .get(activate.entity)
        .or_else(|_| chips.get(activate.event().entity))
    else {
        return;
    };
    let Some(track) = track.as_deref_mut() else {
        return;
    };
    track.shell.set_process_status_filter(chip.0);
    commands.trigger(ShellProjectionFolded);
    commands.trigger(ShellInteractionApplied);
}

/// A mode button publishes a typed intent; the observer below owns the single
/// presentation resource.
pub(crate) fn on_view_mode_activated(
    activate: On<Activate>,
    buttons: Query<&ProcessViewModeButton>,
    mut commands: Commands,
) {
    let Ok(button) = buttons
        .get(activate.entity)
        .or_else(|_| buttons.get(activate.event().entity))
    else {
        return;
    };
    commands.trigger(ProcessViewModeChanged(button.0));
}

fn on_view_mode_changed(
    changed: On<ProcessViewModeChanged>,
    mut mode: ResMut<ProcessViewMode>,
    mut commands: Commands,
) {
    let next = changed.event().0;
    if *mode == next {
        return;
    }
    *mode = next;
    commands.trigger(ShellProjectionFolded);
}

/// Refresh the dashboard's typed value leaves from the same shell projection.
/// The optical root, its fallback binding and the content node survive.
fn sync_focus_dashboard(
    track: Option<bevy::ecs::system::NonSend<FrontendTrack>>,
    mode: Option<Res<ProcessViewMode>>,
    palette: Option<Res<WindowPalette>>,
    mut values: Query<(&ProcessFocusValue, &mut Text)>,
    mut directions: Query<(&mut ProcessFocusSortDir, &mut InlineIcon, &mut IconInk)>,
) {
    if mode.as_deref().copied().unwrap_or(ProcessViewMode::Dense) != ProcessViewMode::Cards {
        return;
    }
    let (Some(track), Some(palette)) = (track, palette) else {
        return;
    };
    let focus = focus_projection(&track.shell);
    let UiPalette { dim_color, .. } = palette.inner.clone();
    for (field, mut text) in &mut values {
        let next = focus.value(field.0);
        if text.0 != next {
            text.0 = next;
        }
    }
    let icon = match focus.sort_descending {
        true => IconId::NavigateDown,
        false => IconId::NavigateUp,
    };
    for (mut direction, mut inline, mut ink) in &mut directions {
        if direction.0 != focus.sort_descending {
            direction.0 = focus.sort_descending;
            inline.0 = icon;
            ink.0 = dim_color;
        }
    }
}

/// Measure the page slot after layout, then spend the shared whole-group
/// admission contract. A changed verdict publishes the interaction seam so
/// the measured window rebuilds once — no polling of hidden card-flow nodes.
pub(crate) fn sync_card_flow_budget(
    pages: Query<&ComputedNode, With<ProcessPageRoot>>,
    palette: Option<Res<WindowPalette>>,
    mut budget: ResMut<CardFlowBudget>,
    mut commands: Commands,
) {
    let Some(palette) = palette else {
        return;
    };
    let Ok(page) = pages.single() else {
        return;
    };
    let size = page.size() * page.inverse_scale_factor();
    if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let next = card_flow_budget(size.x, size.y, &palette.inner);
    if *budget == next {
        return;
    }
    *budget = next;
    commands.trigger(ShellInteractionApplied);
}

/// Register the lane's observers and its post-layout systems. Display
/// authority stays in the page's own responsive system.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<ProcessViewMode>()
        .init_resource::<CardFlowBudget>()
        .add_observer(on_group_chip_activated)
        .add_observer(on_view_mode_activated)
        .add_observer(on_view_mode_changed)
        .add_systems(
            PostUpdate,
            (sync_focus_dashboard, sync_card_flow_budget).after(UiSystems::PostLayout),
        );
}
