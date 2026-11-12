//! Observers and measured systems for the Applications card lane.
//!
//! These systems stay inside Bevy's ECS: they read typed resources and
//! queries, never the raw `World`. Shell mutations go through the shell's
//! reducers, and a changed measured budget publishes the shared interaction
//! seam so the same virtual window rebuilds in the active presentation.

use bevy::app::{App, PostUpdate};
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Has, With};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use bevy::picking::hover::PickingInteraction;
use bevy::ui::Pressed;
use bevy::ui::UiSystems;
use bevy::ui::prelude::{BackgroundColor, BorderColor, ComputedNode};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Activate;
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::presentation::card_flow::CardFlowAdmission;
use taskmanager_ui_contract::presentation::control_ink::ControlInkState;

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
use crate::widgets::controls::{ControlVisual, control_ink_priority};

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
    lanes: Query<&ComputedNode, With<super::ProcessCardLaneViewport>>,
    mut dashboards: Query<
        (&ComputedNode, &mut bevy::ui::prelude::Node),
        With<super::ProcessFocusDashboard>,
    >,
    mode: Option<Res<ProcessViewMode>>,
    palette: Option<Res<WindowPalette>>,
    mut budget: ResMut<CardFlowBudget>,
    mut commands: Commands,
) {
    let Some(palette) = palette else {
        return;
    };
    let Ok(lane) = lanes.single() else {
        return;
    };
    let size = lane.size() * lane.inverse_scale_factor();
    if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let Ok((dashboard_geometry, mut dashboard_node)) = dashboards.single_mut() else {
        return;
    };
    let dashboard_size = dashboard_geometry.size().y * dashboard_geometry.inverse_scale_factor();
    let dashboard_height = if dashboard_size.is_finite() && dashboard_size > 0.0 {
        dashboard_size
    } else {
        budget.dashboard_height_px
    };
    if !dashboard_height.is_finite() || dashboard_height <= 0.0 {
        return;
    }
    let next = card_flow_budget(size.x, size.y, dashboard_height, &palette.inner);
    let view_mode = mode.as_deref().copied().unwrap_or(ProcessViewMode::Dense);
    let previous_admission = budget.effective_admission(view_mode);
    let next_admission = next.effective_admission(view_mode);
    let should_hide_dashboard = matches!(next_admission, CardFlowAdmission::FocusCard { .. });
    let dashboard_display = if should_hide_dashboard {
        bevy::ui::Display::None
    } else {
        bevy::ui::Display::Flex
    };
    let dashboard_visibility_changed = dashboard_node.display != dashboard_display;
    if dashboard_visibility_changed {
        dashboard_node.display = dashboard_display;
    }
    let budget_changed = *budget != next;
    if budget_changed {
        *budget = next;
        commands.trigger(ShellInteractionApplied);
    }
    if previous_admission != next_admission || dashboard_visibility_changed {
        commands.trigger(ShellProjectionFolded);
    }
}

/// Resolve one card's state ink and edge from the shared control verdict.
///
/// The state overlay is transparent while idle; every active state receives
/// one theme-owned wash plus a stronger selected accent on its border. The
/// edge adds a bounded, opaque reading of the state without replacing the
/// glass material or introducing a second palette.
#[must_use]
pub(crate) fn card_state_ink(
    visual: &ControlVisual,
    interaction: PickingInteraction,
    pressed: bool,
    palette: &UiPalette,
) -> (Color, Color) {
    let priority = control_ink_priority(visual, interaction, pressed);
    let state = priority.state();
    let (wash, edge) = match state {
        ControlInkState::Idle => (Color::NONE, Color::NONE),
        ControlInkState::Hovered | ControlInkState::Focused => (palette.hover_bg, palette.accent),
        ControlInkState::Pressed | ControlInkState::Selected | ControlInkState::Disabled => {
            (palette.selection_bg, palette.focus_ring)
        }
    };
    let edge = if matches!(state, ControlInkState::Hovered) && priority.selected {
        palette.focus_ring
    } else {
        edge
    };
    (wash, edge)
}

/// Repaint the card's state tint overlay.
///
/// An optical card owns no painted fill, so the hover, pressed and selected
/// ink cannot replace its surface. The overlay rides ABOVE the glass instead,
/// which keeps the state cue visible without taking the material's job.
pub(crate) fn sync_card_state_overlays(
    palette: Option<Res<WindowPalette>>,
    cards: Query<(&ControlVisual, Option<&PickingInteraction>, Has<Pressed>)>,
    overlays: Query<(Entity, &ProcessCardStateOverlay, &ChildOf)>,
    mut fills: Query<(&mut BackgroundColor, &mut BorderColor)>,
) {
    let Some(palette) = palette else {
        return;
    };
    for (overlay, _, parent) in &overlays {
        let Ok((visual, interaction, pressed)) = cards.get(parent.parent()) else {
            continue;
        };
        let Ok((mut fill, mut border)) = fills.get_mut(overlay) else {
            continue;
        };
        let (next_fill, next_edge) = card_state_ink(
            visual,
            interaction.copied().unwrap_or_default(),
            pressed,
            &palette.inner,
        );
        if fill.0 != next_fill {
            fill.0 = next_fill;
        }
        let next_border = BorderColor::all(next_edge);
        if *border != next_border {
            *border = next_border;
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
