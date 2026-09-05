//! Process CPU affinity editor modal overlay (Applications page).
//!
//! Ownership line: the shell owns `process_affinity_state` and the requests
//! `request_process_affinity` / `request_process_affinity_control_for`;
//! this module owns the Bevy-local modal surface, the multi-core selection grid,
//! the Apply and Cancel controls, and the event/repaint hooks.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Query, ResMut};
use bevy::ecs::world::World;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on, template_value};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    PositionType, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button, ScrollArea};
use taskmanager_application::i18n::t;
use taskmanager_application::{PlatformEffect, ProcessAffinityState};
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_shell::ShellApp;

use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;
use crate::input::PendingEffects;
use crate::palette::{UiPalette, no_wrap_text, space_8, space_12, space_24};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AffinitySession {
    pub(crate) target: FrozenProcessIdentity,
    pub(crate) logical_cpu_count: usize,
    pub(crate) selected_mask: Vec<u32>,
    pub(crate) mask_observed: bool,
}

#[derive(Resource, Default)]
pub(crate) struct ProcessAffinityModalState {
    pub(crate) session: Option<AffinitySession>,
}

impl ProcessAffinityModalState {
    #[allow(dead_code)]
    pub(crate) fn is_open(&self) -> bool {
        self.session.is_some()
    }

    pub(crate) fn open(&mut self, target: FrozenProcessIdentity, logical_cpu_count: usize) {
        let count = logical_cpu_count.max(1);
        let default_mask = (0..count as u32).collect();
        self.session = Some(AffinitySession {
            target,
            logical_cpu_count: count,
            selected_mask: default_mask,
            mask_observed: false,
        });
    }

    pub(crate) fn close(&mut self) {
        self.session = None;
    }

    pub(crate) fn toggle_cpu(&mut self, cpu: u32) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if let Some(pos) = session.selected_mask.iter().position(|&c| c == cpu) {
            if session.selected_mask.len() > 1 {
                session.selected_mask.remove(pos);
            }
        } else {
            session.selected_mask.push(cpu);
            session.selected_mask.sort_unstable();
        }
    }

    pub(crate) fn toggle_all(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.selected_mask.len() == session.logical_cpu_count {
            session.selected_mask = vec![0];
        } else {
            session.selected_mask = (0..session.logical_cpu_count as u32).collect();
        }
    }

    pub(crate) fn apply(&mut self, shell: &mut ShellApp) -> Option<PlatformEffect> {
        let session = self.session.take()?;
        shell.request_process_affinity_control_for(session.target, session.selected_mask)
    }
}

// ---- markers and events ---------------------------------------------------

#[derive(Component, Clone, Default)]
pub(crate) struct ProcessAffinityOverlay;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AffinityCpuButton(pub(crate) u32);

#[derive(Component, Clone, Copy, Default)]
pub(crate) struct AffinityToggleAllButton;

#[derive(Component, Clone, Copy, Default)]
pub(crate) struct AffinityApplyButton;

#[derive(Component, Clone, Copy, Default)]
pub(crate) struct AffinityCancelButton;

#[derive(Event)]
pub(crate) struct ProcessAffinityRequested(pub(crate) FrozenProcessIdentity, pub(crate) usize);

#[derive(Event)]
pub(crate) struct ProcessAffinityRepaintRequired;

// ---- scenes ---------------------------------------------------------------

pub(crate) fn affinity_modal_scene(
    session: &AffinitySession,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let title = format!("{} · {}", t("dialog.cpu_affinity"), session.target.name);
    let subtitle = format!(
        "PID: {} · {}/{} CPUs",
        session.target.pid,
        session.selected_mask.len(),
        session.logical_cpu_count
    );

    let mut cpu_buttons: Vec<Box<dyn Scene>> = Vec::with_capacity(session.logical_cpu_count);
    for cpu in 0..session.logical_cpu_count as u32 {
        let selected = session.selected_mask.contains(&cpu);
        let label = format!("CPU {cpu}");
        let fill = if selected {
            palette.nav_active_bg
        } else {
            palette.content_bg
        };

        cpu_buttons.push(Box::new(bsn! {
            (
                Node {
                    width: px(88.0),
                    height: px(palette.control_height_px),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BackgroundColor(fill)
                ControlVisual(ControlTone::Surface, selected)
                Button
                on(on_cpu_button_activated)
                AffinityCpuButton(cpu)
                Children [
                    ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) )
                ]
            )
        }) as Box<dyn Scene>);
    }

    let grid = Box::new(bsn! {
        Node {
            width: percent(100.0),
            height: px(200.0),
            flex_direction: FlexDirection::Row,
            flex_wrap: bevy::ui::FlexWrap::Wrap,
            column_gap: Val::Px(space_8()),
            row_gap: Val::Px(space_8()),
            overflow: Overflow::scroll_y(),
        }
        ScrollArea
        Children [
            { cpu_buttons },
        ]
    }) as Box<dyn Scene>;

    let panel = Box::new(bsn! {
        Node {
            width: px(460.0),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_24())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            ( Text(title) TextRole(Role::Heading) ),
            ( Text(subtitle) TextRole(Role::Caption) ),
            ( { grid } ),
            (
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::End,
                    column_gap: Val::Px(space_12()),
                    margin: UiRect::top(Val::Px(space_8())),
                }
                Children [
                    (
                        Text({ t("common.all").to_owned() })
                        TextRole(Role::Caption)
                        ControlVisual(ControlTone::Surface, false)
                        Button
                        on(on_toggle_all_activated)
                        AffinityToggleAllButton
                    ),
                    (
                        Text({ t("common.apply").to_owned() })
                        TextRole(Role::Body)
                        ControlVisual(ControlTone::Surface, true)
                        Button
                        on(on_apply_activated)
                        AffinityApplyButton
                    ),
                    (
                        Text({ t("common.cancel").to_owned() })
                        TextRole(Role::Caption)
                        ControlVisual(ControlTone::Surface, false)
                        Button
                        on(on_cancel_activated)
                        AffinityCancelButton
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
        ProcessAffinityOverlay
        Children [
            ( { panel } ),
        ]
    }) as Box<dyn Scene>
}

// ---- observers ------------------------------------------------------------

pub(crate) fn on_cpu_button_activated(
    activate: On<Activate>,
    buttons: Query<&AffinityCpuButton>,
    mut state: ResMut<ProcessAffinityModalState>,
    mut commands: Commands,
) {
    let Ok(button) = buttons.get(activate.event().entity) else {
        return;
    };
    state.toggle_cpu(button.0);
    commands.trigger(ProcessAffinityRepaintRequired);
}

pub(crate) fn on_toggle_all_activated(
    _activate: On<Activate>,
    mut state: ResMut<ProcessAffinityModalState>,
    mut commands: Commands,
) {
    state.toggle_all();
    commands.trigger(ProcessAffinityRepaintRequired);
}

pub(crate) fn on_apply_activated(
    _activate: On<Activate>,
    mut state: ResMut<ProcessAffinityModalState>,
    mut track: NonSendMut<FrontendTrack>,
    mut pending: ResMut<PendingEffects>,
    mut commands: Commands,
) {
    if let Some(effect) = state.apply(&mut track.shell) {
        pending.0.push(effect);
    }
    commands.trigger(ProcessAffinityRepaintRequired);
}

pub(crate) fn on_cancel_activated(
    _activate: On<Activate>,
    mut state: ResMut<ProcessAffinityModalState>,
    mut commands: Commands,
) {
    state.close();
    commands.trigger(ProcessAffinityRepaintRequired);
}

pub(crate) fn on_affinity_requested(
    request: On<ProcessAffinityRequested>,
    mut state: ResMut<ProcessAffinityModalState>,
    mut pending: ResMut<PendingEffects>,
    mut commands: Commands,
) {
    let target = request.event().0.clone();
    let cpu_count = request.event().1;
    state.open(target.clone(), cpu_count);
    pending.0.push(PlatformEffect::ProcessAffinity(
        taskmanager_application::ProcessAffinityRequest { target },
    ));
    commands.trigger(ProcessAffinityRepaintRequired);
}

pub(crate) fn on_affinity_fold_sync(
    _fold: On<ShellProjectionFolded>,
    track: crate::app::ShellTrack,
    mut state: ResMut<ProcessAffinityModalState>,
    mut commands: Commands,
) {
    let Some(session) = state.session.as_mut() else {
        return;
    };
    if let ProcessAffinityState::Ready(ready) = track.shell().process_affinity_state()
        && ready.target == session.target
        && !session.mask_observed
    {
        session.selected_mask = ready.cpus.clone();
        session.mask_observed = true;
        commands.trigger(ProcessAffinityRepaintRequired);
    }
}

pub(crate) fn on_affinity_repaint_required(
    _repaint: On<ProcessAffinityRepaintRequired>,
    mut commands: Commands,
) {
    commands.queue(paint_affinity_modal);
}

pub(crate) fn paint_affinity_modal(world: &mut World) {
    let palette = world.resource::<WindowPalette>().inner.clone();
    let modal_state = world.resource::<ProcessAffinityModalState>();
    let scene = modal_state
        .session
        .as_ref()
        .map(|session| affinity_modal_scene(session, &palette));

    let roots: Vec<Entity> = world
        .query_filtered::<Entity, With<AppShellRoot>>()
        .iter(world)
        .collect();
    let Some(&root) = roots.first() else {
        return;
    };

    let overlays: Vec<Entity> = world
        .query_filtered::<Entity, With<ProcessAffinityOverlay>>()
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
    app.init_resource::<ProcessAffinityModalState>();
    app.add_observer(on_affinity_requested);
    app.add_observer(on_affinity_repaint_required);
    app.add_observer(on_affinity_fold_sync);
}
