//! Service dependencies panel: the bevy surface for the shell's renderer-neutral
//! service dependencies lifecycle (ADR-027, `ShellApp::request_service_dependencies`).
//!
//! Ownership line: the shell owns `service_dependencies` lifecycle; this module
//! owns ONLY the product surface: the dependencies toolbar button, the panel scene,
//! the Close control, and the fingerprint-gated repaint.

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
    AlignItems, BackgroundColor, BorderRadius, Display, FlexDirection, JustifyContent, Node,
    Overflow, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button, ScrollArea};
use taskmanager_application::ServiceDependenciesLifecycle;
use taskmanager_application::i18n::t;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::services::{ServiceDeps, ServiceRelationKind};
use taskmanager_core::core::target::ServiceId;
use taskmanager_shell::ShellApp;

use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;
use crate::input::PendingEffects;
use crate::palette::{UiPalette, no_wrap_text, space_2, space_4, space_8, space_12};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole, WindowPalette};

// ---- pure view model -------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ServiceDependenciesControlAction {
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DependenciesFingerprint {
    pub(crate) target: Option<ServiceId>,
    pub(crate) is_loading: bool,
    pub(crate) has_deps: bool,
    pub(crate) failure: Option<FailureKind>,
}

const CLOSED_FINGERPRINT: fn() -> DependenciesFingerprint = || DependenciesFingerprint {
    target: None,
    is_loading: false,
    has_deps: false,
    failure: None,
};

#[must_use]
pub(crate) fn dependencies_fingerprint(
    lifecycle: &ServiceDependenciesLifecycle,
) -> DependenciesFingerprint {
    match lifecycle {
        ServiceDependenciesLifecycle::Closed => CLOSED_FINGERPRINT(),
        ServiceDependenciesLifecycle::Loading { target, .. } => DependenciesFingerprint {
            target: Some(target.clone()),
            is_loading: true,
            has_deps: lifecycle.projected().is_some(),
            failure: None,
        },
        ServiceDependenciesLifecycle::Ready {
            target,
            dependencies,
            ..
        } => DependenciesFingerprint {
            target: Some(target.clone()),
            is_loading: false,
            has_deps: !dependencies.relations().is_empty(),
            failure: None,
        },
        ServiceDependenciesLifecycle::Failed {
            target, failure, ..
        } => DependenciesFingerprint {
            target: Some(target.clone()),
            is_loading: false,
            has_deps: lifecycle.projected().is_some(),
            failure: Some(*failure),
        },
    }
}

#[derive(Resource, Default)]
pub(crate) struct ServicesDependenciesRenderState {
    pub(crate) rendered: Option<DependenciesFingerprint>,
}

// ---- scenes ----------------------------------------------------------------

fn relation_section_scene(
    header: &str,
    deps: &ServiceDeps,
    kind: &ServiceRelationKind,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let targets: Vec<String> = deps
        .relation_targets(kind)
        .map(|id| id.as_str().to_owned())
        .collect();

    let count_text = if targets.is_empty() {
        format!("{header}: —")
    } else {
        format!("{header} ({}):", targets.len())
    };

    let target_chips: Vec<Box<dyn Scene>> = targets
        .into_iter()
        .map(|target_id| {
            Box::new(bsn! {
                (
                    Node {
                        height: px(palette.control_height_px),
                        padding: UiRect::horizontal(Val::Px(space_8())),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                    }
                    BackgroundColor({ palette.content_bg })
                    Children [
                        ( Text(target_id) TextRole(Role::Mono) template_value(no_wrap_text()) )
                    ]
                )
            }) as Box<dyn Scene>
        })
        .collect();

    bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::vertical(Val::Px(space_2())),
        }
        Children [
            ( Text(count_text) TextRole(Role::Caption) template_value(no_wrap_text()) ),
            (
                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: bevy::ui::FlexWrap::Wrap,
                    column_gap: Val::Px(space_4()),
                    row_gap: Val::Px(space_4()),
                }
                Children [
                    { target_chips },
                ]
            ),
        ]
    }
}

fn chip_button(
    action: ServiceDependenciesControlAction,
    label: String,
    active: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    Box::new(bsn! {
        (
            Node {
                height: px(palette.control_height_px),
                padding: UiRect::horizontal(Val::Px(space_12())),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
            }
            BackgroundColor({
                if active { palette.nav_active_bg } else { palette.content_bg }
            })
            ControlVisual(ControlTone::Surface, active)
            Button
            on(dependencies_panel_control_activated)
            ServiceDependenciesControlButton(action)
            Children [
                ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) )
            ]
        )
    })
}

pub(crate) fn service_dependencies_panel_scene(
    shell: &ShellApp,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let lifecycle = &shell.service_dependencies;
    let target_id = match lifecycle.target() {
        Some(id) => id.to_string(),
        None => {
            return Box::new(bsn! {
                Node { display: Display::None }
            });
        }
    };

    let title = format!("{} — {target_id}", t("svc.dependencies"));

    let body_scene: Box<dyn Scene> = if lifecycle.is_loading() {
        Box::new(bsn! {
            Node { width: percent(100.0) }
            Children [
                ( Text(t("svc.details_loading")) TextRole(Role::Caption) )
            ]
        })
    } else if let Some(failure) = lifecycle.failure() {
        let msg = format!("Failed to load dependencies: {failure:?}");
        Box::new(bsn! {
            Node { width: percent(100.0) }
            Children [
                ( Text(msg) TextRole(Role::Caption) )
            ]
        })
    } else if let Some(deps) = lifecycle.projected() {
        let req = Box::new(relation_section_scene(
            t("svc.requires"),
            deps,
            &ServiceRelationKind::Requires,
            palette,
        )) as Box<dyn Scene>;
        let wants = Box::new(relation_section_scene(
            t("svc.wants"),
            deps,
            &ServiceRelationKind::Wants,
            palette,
        )) as Box<dyn Scene>;
        let wanted_by = Box::new(relation_section_scene(
            t("svc.wanted_by"),
            deps,
            &ServiceRelationKind::WantedBy,
            palette,
        )) as Box<dyn Scene>;
        let after = Box::new(relation_section_scene(
            t("svc.after"),
            deps,
            &ServiceRelationKind::After,
            palette,
        )) as Box<dyn Scene>;

        Box::new(bsn! {
            Node {
                width: percent(100.0),
                height: px(240.0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
            }
            ScrollArea
            Children [
                ( { req } ),
                ( { wants } ),
                ( { wanted_by } ),
                ( { after } ),
            ]
        })
    } else {
        Box::new(bsn! {
            Node { width: percent(100.0) }
            Children [
                ( Text(t("svc.details_loading")) TextRole(Role::Caption) )
            ]
        })
    };

    Box::new(bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_12())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        ServicesDependenciesPanelRoot
        Children [
            (
                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                }
                Children [
                    (
                        Text(title)
                        TextRole(Role::Body)
                        template_value(no_wrap_text())
                    ),
                    ( Node { flex_grow: 1.0 } ),
                    ( { chip_button(ServiceDependenciesControlAction::Close, t("common.close").to_owned(), false, palette) } ),
                ]
            ),
            (
                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                Children [
                    ( { body_scene } ),
                ]
            ),
        ]
    })
}

// ---- events, markers, observers -------------------------------------------

#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ServicesDependenciesPanelRoot;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ServiceDependenciesControlButton(pub(crate) ServiceDependenciesControlAction);

impl Default for ServiceDependenciesControlButton {
    fn default() -> Self {
        Self(ServiceDependenciesControlAction::Close)
    }
}

#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ServicesDependenciesPanelSlot;

#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ServicesDependenciesOpenButton;

#[derive(Event)]
pub(crate) struct ServiceDependenciesRequested;

#[derive(Event)]
pub(crate) struct DependenciesPanelRepaintRequired;

pub(crate) fn dependencies_panel_control_activated(
    activate: On<Activate>,
    buttons: Query<&ServiceDependenciesControlButton>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    let Ok(button) = buttons.get(activate.event().entity) else {
        return;
    };
    match button.0 {
        ServiceDependenciesControlAction::Close => {
            track.shell.service_dependencies.close();
        }
    }
    commands.trigger(DependenciesPanelRepaintRequired);
}

pub(crate) fn services_dependencies_button_activated(
    activate: On<Activate>,
    buttons: Query<&ServicesDependenciesOpenButton>,
    mut commands: Commands,
) {
    if buttons.get(activate.event().entity).is_ok() {
        commands.trigger(ServiceDependenciesRequested);
    }
}

pub(crate) fn on_services_dependencies_requested(
    _request: On<ServiceDependenciesRequested>,
    selection: Option<Res<super::ServiceSelection>>,
    mut pending: ResMut<PendingEffects>,
    mut commands: Commands,
) {
    let Some(target) = selection.as_ref().and_then(|state| state.target.as_ref()) else {
        return;
    };
    pending
        .0
        .push(ShellApp::request_service_dependencies(target.clone()));
    commands.trigger(DependenciesPanelRepaintRequired);
}

pub(crate) fn on_services_fold_dependencies_gate(
    _fold: On<ShellProjectionFolded>,
    track: crate::app::ShellTrack,
    rendered: Res<ServicesDependenciesRenderState>,
    mut commands: Commands,
) {
    let fingerprint = dependencies_fingerprint(&track.shell().service_dependencies);
    if rendered.rendered.as_ref() != Some(&fingerprint) {
        commands.trigger(DependenciesPanelRepaintRequired);
    }
}

pub(crate) fn on_dependencies_panel_repaint_required(
    _repaint: On<DependenciesPanelRepaintRequired>,
    mut commands: Commands,
) {
    commands.queue(paint_dependencies_panel);
}

pub(crate) fn on_dependencies_panel_slot_added(
    _added: On<bevy::ecs::lifecycle::Add, ServicesDependenciesPanelSlot>,
    mut commands: Commands,
) {
    commands.queue(paint_dependencies_panel);
}

pub(crate) fn paint_dependencies_panel(world: &mut World) {
    let palette = world.resource::<WindowPalette>().inner.clone();
    let fingerprint =
        dependencies_fingerprint(&world.non_send::<FrontendTrack>().shell.service_dependencies);
    let scene = {
        let track = world.non_send::<FrontendTrack>();
        if track.shell.service_dependencies.target().is_some() {
            Some(service_dependencies_panel_scene(&track.shell, &palette))
        } else {
            None
        }
    };
    let slot = world
        .query_filtered::<Entity, With<ServicesDependenciesPanelSlot>>()
        .iter(world)
        .next();
    let Some(slot) = slot else {
        return;
    };
    let stale: Vec<Entity> = world
        .get::<bevy::ecs::hierarchy::Children>(slot)
        .map(|children| children.iter().copied().collect())
        .unwrap_or_default();
    let mut commands = world.commands();
    for entity in stale {
        commands.entity(entity).despawn();
    }
    if let Some(scene) = scene {
        let entity = commands.spawn_scene(scene).id();
        commands.entity(slot).add_one_related::<ChildOf>(entity);
    }
    world
        .resource_mut::<ServicesDependenciesRenderState>()
        .rendered = Some(fingerprint);
}
