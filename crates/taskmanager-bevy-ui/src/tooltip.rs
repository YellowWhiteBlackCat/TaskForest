//! Pointer-hover explanation tooltip overlay surface.
//!
//! Delivers true parity with GPUI's `TooltipHost` and Iced's `widget::tooltip`:
//! interactive controls and key metrics can attach a [`TooltipText`] component
//! so hovering reveals an explanation card anchored to the root view.
//!
//! # Architecture
//!
//! Governed by an explicit discrete state machine ([`TooltipState`]):
//! `None` when inactive, `Some(session)` when an interactive element is hovered.
//! State transitions emit [`TooltipStateChanged`] so the observer updates the
//! overlay without frame-by-frame polling.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::{Changed, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, JustifyContent, Node, PositionType, UiRect, Val, px,
};
use bevy::ui::widget::Text;

use crate::palette::{UiPalette, space_4, space_8};
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};

/// Active session of an element hover tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TooltipSession {
    pub(crate) target: Entity,
    pub(crate) text: String,
}

/// Explicit state machine resource for tooltip display:
/// `None` when hidden, `Some(session)` when visible.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TooltipState {
    pub(crate) session: Option<TooltipSession>,
}

impl TooltipState {
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn is_visible(&self) -> bool {
        self.session.is_some()
    }

    pub(crate) fn show(&mut self, target: Entity, text: String) {
        self.session = Some(TooltipSession { target, text });
    }

    pub(crate) fn hide(&mut self) {
        self.session = None;
    }
}

/// Event triggered when tooltip session opens or closes.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct TooltipStateChanged;

/// Component attached to interactive entities that offer a hover tooltip.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TooltipText(pub(crate) String);

/// Marker component on the mounted tooltip visual card.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct TooltipOverlay;

/// Scene for the hovering explanation card.
pub(crate) fn tooltip_card_scene(text: &str, palette: &UiPalette) -> impl Scene + use<> {
    let radius = palette.control_radius_px;
    let label = text.to_string();
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            bottom: px(34.0),
            left: px(24.0),
            padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_4())),
            border_radius: BorderRadius::all(Val::Px(radius)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        }
        BackgroundColor({ palette.panel_fill })
        TooltipOverlay
        Children [
            ( Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } ),
        ]
    }
}

/// System tracking hover interactions on entities with [`TooltipText`].
pub(crate) fn tooltip_hover_interaction_system(
    mut state: ResMut<TooltipState>,
    query: Query<(Entity, &TooltipText, &PickingInteraction), Changed<PickingInteraction>>,
    mut commands: Commands,
) {
    let mut changed = false;
    for (entity, text, interaction) in &query {
        match interaction {
            PickingInteraction::Hovered => {
                state.show(entity, text.0.clone());
                changed = true;
            }
            PickingInteraction::None | PickingInteraction::Pressed => {
                if state.session.as_ref().map(|s| s.target) == Some(entity) {
                    state.hide();
                    changed = true;
                }
            }
        }
    }
    if changed {
        commands.trigger(TooltipStateChanged);
    }
}

fn on_tooltip_state_changed(
    _changed: On<TooltipStateChanged>,
    state: Option<Res<TooltipState>>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<AppShellRoot>>,
    overlays: Query<Entity, With<TooltipOverlay>>,
    mut commands: Commands,
) {
    for entity in &overlays {
        commands.entity(entity).despawn();
    }
    let Some(state) = state else {
        return;
    };
    let Some(ref session) = state.session else {
        return;
    };
    let Some(palette) = palette else {
        return;
    };
    let Ok(root) = roots.single() else {
        return;
    };
    let scene = tooltip_card_scene(&session.text, &palette.inner);
    let overlay = commands.spawn_scene(scene).id();
    commands.entity(root).add_one_related::<ChildOf>(overlay);
}

/// Register tooltip state, systems, and observer.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<TooltipState>();
    app.add_observer(on_tooltip_state_changed);
    app.add_systems(bevy::app::Update, tooltip_hover_interaction_system);
}

#[cfg(test)]
#[path = "../tests/headless/tooltip.rs"]
mod tests;
