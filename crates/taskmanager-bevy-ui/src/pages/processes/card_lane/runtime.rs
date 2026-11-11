//! Observers and measured systems for the Applications card lane.
//!
//! These systems stay inside Bevy's ECS: they read typed resources and
//! queries, never the raw `World`. Shell mutations go through the shell's
//! reducers, and a changed measured budget publishes the shared interaction
//! seam so the same virtual window rebuilds in the active presentation.

use bevy::app::{App, PostUpdate};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Has, With};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use bevy::picking::hover::PickingInteraction;
use bevy::ui::Pressed;
use bevy::ui::UiSystems;
use bevy::ui::prelude::{BackgroundColor, ComputedNode};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Activate;
use taskmanager_theme::GlassTier;
use taskmanager_ui_contract::IconId;

use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;
use crate::icons::{IconInk, InlineIcon};
use crate::input::ShellInteractionApplied;
use crate::palette::UiPalette;
use crate::window::WindowPalette;

use super::{
    CardFlowBudget, CardFlowWindow, ProcessCardStateOverlay, ProcessFocusSortDir,
    ProcessFocusValue, ProcessGroupOverflow, ProcessViewMode, ProcessViewModeButton,
    ProcessViewModeChanged, card_flow_budget, focus_projection,
};
use crate::widgets::controls::{ControlVisual, control_background};

/// The group header's overflow action: reduce through the shell's canonical
/// status filter and hand the page to its dense table.
///
/// The bucket's admitted row is bounded by the measured column count, so the
/// rows it could not hold would otherwise be summarized away. This action
/// is how every row of the bucket stays reachable, and it uses the SAME
/// filter the page chrome's chips use — one fact, one owner.
pub(crate) fn on_group_overflow_activated(
    activate: On<Activate>,
    overflows: Query<&ProcessGroupOverflow>,
    mut track: Option<NonSendMut<FrontendTrack>>,
    mut mode: Option<ResMut<ProcessViewMode>>,
    mut commands: Commands,
) {
    let Ok(overflow) = overflows
        .get(activate.entity)
        .or_else(|_| overflows.get(activate.event().entity))
    else {
        return;
    };
    let Some(track) = track.as_deref_mut() else {
        return;
    };
    track.shell.set_process_status_filter(overflow.0);
    if let Some(mode) = mode.as_deref_mut() {
        *mode = ProcessViewMode::Dense;
    }
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
    flows: Query<&ComputedNode, With<super::ProcessCardFlow>>,
    mode: Option<Res<ProcessViewMode>>,
    palette: Option<Res<WindowPalette>>,
    mut budget: ResMut<CardFlowBudget>,
    mut commands: Commands,
) {
    let Some(palette) = palette else {
        return;
    };
    let Ok(flow) = flows.single() else {
        return;
    };
    let size = flow.size() * flow.inverse_scale_factor();
    if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let next = card_flow_budget(size.x, size.y, &palette.inner);
    if *budget == next {
        return;
    }
    let view_mode = mode.as_deref().copied().unwrap_or(ProcessViewMode::Dense);
    let presentation_changed = next.effective_mode(view_mode) != budget.effective_mode(view_mode);
    *budget = next;
    if presentation_changed {
        commands.trigger(ShellProjectionFolded);
    }
    commands.trigger(ShellInteractionApplied);
}

/// Repaint the card's state tint overlay.
///
/// An optical card owns no painted fill, so the hover, pressed and selected
/// ink cannot replace its surface. The overlay rides ABOVE the glass instead,
/// which keeps the state cue visible without taking the material's job.
fn sync_card_state_overlays(
    palette: Option<Res<WindowPalette>>,
    cards: Query<(&ControlVisual, Option<&PickingInteraction>, Has<Pressed>)>,
    overlays: Query<(Entity, &ProcessCardStateOverlay, &ChildOf)>,
    mut fills: Query<&mut BackgroundColor>,
) {
    let Some(palette) = palette else {
        return;
    };
    for (overlay, _, parent) in &overlays {
        let Ok((visual, interaction, pressed)) = cards.get(parent.parent()) else {
            continue;
        };
        let Ok(mut fill) = fills.get_mut(overlay) else {
            continue;
        };
        let next = control_background(
            visual,
            interaction.copied().unwrap_or_default(),
            pressed,
            &palette.inner,
        );
        let next = if next == palette.inner.glass_fill(GlassTier::Card) {
            bevy::color::Color::NONE
        } else {
            next
        };
        if fill.0 != next {
            fill.0 = next;
        }
    }
}

/// Register the lane's observers and its post-layout systems. Display
/// authority stays in the page's own responsive system.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<ProcessViewMode>()
        .init_resource::<CardFlowBudget>()
        .init_resource::<CardFlowWindow>()
        .add_observer(on_group_overflow_activated)
        .add_observer(on_view_mode_activated)
        .add_observer(on_view_mode_changed)
        .add_systems(
            PostUpdate,
            (sync_focus_dashboard, sync_card_flow_budget).after(UiSystems::PostLayout),
        )
        .add_systems(
            PostUpdate,
            sync_card_state_overlays.after(UiSystems::PostLayout),
        );
}
