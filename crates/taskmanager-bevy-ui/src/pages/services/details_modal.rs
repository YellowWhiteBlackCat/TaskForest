//! Service details modal for Bevy (Services page).
//!
//! Provides a dedicated local surface for viewing complete properties of the
//! selected service (name, ID, load state, active state, sub state, description).

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use bevy::ecs::world::World;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on, template_value};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, PositionType,
    UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_core::core::services::ServiceItem;

use crate::app::FrontendTrack;
use crate::palette::{UiPalette, no_wrap_text, space_4, space_8, space_24};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};

#[derive(Resource, Default)]
pub(crate) struct ServiceDetailsModalState {
    pub(crate) target: Option<ServiceItem>,
}

#[derive(Component, Clone, Default)]
pub(crate) struct ServiceDetailsOverlay;

#[derive(Component, Clone, Default)]
pub(crate) struct ServiceDetailsCloseButton;

#[derive(Component, Clone, Default)]
pub(crate) struct ServiceDetailsOpenButton;

#[derive(Event)]
pub(crate) struct ServiceDetailsRequested;

#[derive(Event)]
pub(crate) struct ServiceDetailsRepaintRequired;

// ---- scenes ---------------------------------------------------------------

fn fact_row_scene(label: &str, value: &str, _palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::vertical(Val::Px(space_4())),
        }
        Children [
            (
                Node { width: px(120.0), flex_shrink: 0.0 }
                Children [
                    ( Text({ label.to_owned() }) TextRole(Role::Caption) template_value(no_wrap_text()) )
                ]
            ),
            (
                Node { flex_grow: 1.0 }
                Children [
                    ( Text({ value.to_owned() }) TextRole(Role::Body) )
                ]
            ),
        ]
    })
}

pub(crate) fn service_details_modal_scene(
    service: &ServiceItem,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let title = format!("{} · {}", t("dialog.service_details"), service.name);
    let id_label = service.id.to_string();

    let rows: Vec<Box<dyn Scene>> = vec![
        fact_row_scene(t("common.name"), &service.name, palette),
        fact_row_scene("ID", &id_label, palette),
        fact_row_scene(t("common.status"), service.status.as_str(), palette),
        fact_row_scene(t("svc.load_state"), &service.load_state, palette),
        fact_row_scene(t("svc.active_state"), &service.active_state, palette),
        fact_row_scene(t("svc.sub_state"), &service.sub_state, palette),
        fact_row_scene(t("common.description"), &service.description, palette),
    ];

    let panel = Box::new(bsn! {
        Node {
            width: px(500.0),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_24())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            ( Text(title) TextRole(Role::Heading) ),
            (
                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_4()),
                    margin: UiRect::vertical(Val::Px(space_8())),
                }
                Children [
                    { rows },
                ]
            ),
            (
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::End,
                    margin: UiRect::top(Val::Px(space_8())),
                }
                Children [
                    (
                        Text({ t("common.close").to_owned() })
                        TextRole(Role::Body)
                        ControlVisual(ControlTone::Surface, true)
                        Button
                        on(on_close_button_activated)
                        ServiceDetailsCloseButton
                    ),
                ]
            ),
        ]
    }) as Box<dyn Scene>;

    let scrim = palette.scrim;
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        BackgroundColor({ scrim })
        ServiceDetailsOverlay
        Children [
            ( { panel } ),
        ]
    }) as Box<dyn Scene>
}

// ---- observers ------------------------------------------------------------

pub(crate) fn on_close_button_activated(
    _activate: On<Activate>,
    mut state: ResMut<ServiceDetailsModalState>,
    mut commands: Commands,
) {
    state.target = None;
    commands.trigger(ServiceDetailsRepaintRequired);
}

pub(crate) fn on_details_button_activated(
    activate: On<Activate>,
    buttons: Query<&ServiceDetailsOpenButton>,
    mut commands: Commands,
) {
    if buttons.get(activate.event().entity).is_ok() {
        commands.trigger(ServiceDetailsRequested);
    }
}

pub(crate) fn on_service_details_requested(
    _request: On<ServiceDetailsRequested>,
    track: NonSendMut<FrontendTrack>,
    selection: Option<Res<super::ServiceSelection>>,
    mut state: ResMut<ServiceDetailsModalState>,
    mut commands: Commands,
) {
    let Some(target) = selection.as_ref().and_then(|s| s.target.as_ref()) else {
        return;
    };
    let Some(service) = track
        .shell
        .sorted_services()
        .into_iter()
        .find(|s| &s.id == target)
    else {
        return;
    };
    state.target = Some(service.clone());
    commands.trigger(ServiceDetailsRepaintRequired);
}

pub(crate) fn on_service_details_repaint_required(
    _repaint: On<ServiceDetailsRepaintRequired>,
    mut commands: Commands,
) {
    commands.queue(paint_service_details_modal);
}

pub(crate) fn paint_service_details_modal(world: &mut World) {
    let palette = world.resource::<WindowPalette>().inner.clone();
    let state = world.resource::<ServiceDetailsModalState>();
    let scene = state
        .target
        .as_ref()
        .map(|service| service_details_modal_scene(service, &palette));

    let roots: Vec<Entity> = world
        .query_filtered::<Entity, With<AppShellRoot>>()
        .iter(world)
        .collect();
    let Some(&root) = roots.first() else {
        return;
    };

    let overlays: Vec<Entity> = world
        .query_filtered::<Entity, With<ServiceDetailsOverlay>>()
        .iter(world)
        .collect();
    let mut commands = world.commands();
    for entity in overlays {
        commands.entity(entity).despawn();
    }

    if let Some(scene) = scene {
        let entity = commands.spawn_scene(scene).id();
        commands.entity(root).add_one_related::<ChildOf>(entity);
    }
}

pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<ServiceDetailsModalState>();
    app.add_observer(on_service_details_requested);
    app.add_observer(on_service_details_repaint_required);
}
