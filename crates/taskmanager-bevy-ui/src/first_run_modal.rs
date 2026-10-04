//! The First-Run setup modal for the Bevy frontend.

use bevy::app::App;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, Query, Res};
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, PositionType,
    UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;

use crate::palette::{UiPalette, space_8, space_24};
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};

#[derive(Event, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FirstRunModalChanged(pub(crate) bool);

#[derive(Component, Clone, Default)]
pub(crate) struct FirstRunModalOverlay;

pub(crate) fn register(app: &mut App) {
    app.add_observer(on_first_run_modal_changed);
}

fn on_first_run_modal_changed(
    changed: On<FirstRunModalChanged>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<AppShellRoot>>,
    overlays: Query<Entity, With<FirstRunModalOverlay>>,
    mut commands: Commands,
) {
    for entity in &overlays {
        commands.entity(entity).despawn();
    }
    if !changed.event().0 {
        return;
    }
    let Some(palette) = palette else {
        return;
    };
    let Ok(root) = roots.single() else {
        return;
    };
    let overlay = commands
        .spawn_scene(first_run_overlay_scene(&palette.inner))
        .id();
    commands.entity(root).add_one_related::<ChildOf>(overlay);
}

fn first_run_overlay_scene(palette: &UiPalette) -> impl Scene + use<> {
    let panel = panel_scene(palette);
    let scrim = palette.scrim;
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        BackgroundColor({ scrim })
        FirstRunModalOverlay
        Children [
            @{ panel }
        ]
    }
}

fn panel_scene(palette: &UiPalette) -> impl Scene + use<> {
    let title = t("first_run.title").to_owned();
    let desc = t("first_run.description").to_owned();
    let hint = t("first_run.restart_required").to_owned();
    let radius = palette.panel_radius_px;
    bsn! {
        Node {
            width: px(500.0),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_24())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            Text(title) TextRole(Role::Heading) --
            Text(desc) TextRole(Role::Body) --
            Text(hint) TextRole(Role::Caption)
        ]
    }
}
