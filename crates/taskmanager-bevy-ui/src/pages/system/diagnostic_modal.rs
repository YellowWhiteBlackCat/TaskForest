//! Diagnostic review, input ownership and correlated background publication.

use bevy::app::{App, PreUpdate, Update};
use bevy::ecs::change_detection::Mut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res};
use bevy::ecs::world::World;
use bevy::input::keyboard::KeyCode;
use bevy::picking::Pickable;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    PositionType, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, ScrollPosition};
use bevy::ui_widgets::{Activate, Button, ScrollArea};
use std::marker::PhantomData;
use taskmanager_app_host::DiagnosticBundleClient;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::i18n::t;
use taskmanager_application::{DiagnosticBundleSession, DiagnosticBundleTarget};
use taskmanager_core::core::diagnostics::DiagnosticBundleError;
use taskmanager_shell::presentation::diagnostics::{
    diagnostic_failure_message, diagnostic_preview_text,
};

use crate::app::FrontendTrack;
use crate::input::ShellInteractionApplied;
use crate::menu_modal::{ActionMenuContext, MenuModal, MenuModalChanged};
use crate::pages::processes::menu::ProcessMenuCtx;
use crate::pages::services::menu::ServiceMenuCtx;
use crate::pages::sessions::menu::SessionMenuCtx;
use crate::pages::startup::menu::StartupMenuCtx;
use crate::palette::{UiPalette, space_8, space_24};
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};

#[derive(Event, Clone, Debug, Default)]
pub(crate) enum DiagnosticCommand {
    Open,
    Failure(DiagnosticBundleError),
    Confirm,
    Retry,
    Scroll(f32),
    #[default]
    Close,
}

#[derive(Resource, Default)]
pub(crate) struct DiagnosticModalState(pub(crate) Option<DiagnosticBundleUiState>);

#[derive(Resource, Default)]
pub(crate) struct DiagnosticRuntime(
    pub(crate) Option<DiagnosticBundleSession<DiagnosticBundleClient>>,
);

impl DiagnosticRuntime {
    pub(crate) fn install(&mut self, client: DiagnosticBundleClient) {
        self.0 = Some(DiagnosticBundleSession::new(client));
    }
    fn close(&mut self) {
        if let Some(session) = &mut self.0 {
            session.close();
        }
    }
}

#[derive(Event)]
struct DiagnosticModalChanged;
#[derive(Component, Clone, Default)]
pub(crate) struct DiagnosticControl(pub(crate) DiagnosticCommand);
#[derive(Component, Clone, Default)]
pub(crate) struct DiagnosticModalOverlay;
#[derive(Component, Clone, Default)]
pub(crate) struct DiagnosticHeading;
#[derive(Component, Clone, Default)]
pub(crate) struct DiagnosticBody;
#[derive(Component, Clone, Default)]
pub(crate) struct DiagnosticFooter;

pub(crate) fn register(app: &mut App) {
    app.init_resource::<DiagnosticModalState>()
        .init_resource::<DiagnosticRuntime>()
        .add_observer(on_command)
        .add_observer(on_changed)
        .add_systems(PreUpdate, drain)
        .add_systems(Update, mount_when_root_ready);
}

fn on_command(command: On<DiagnosticCommand>, mut commands: Commands) {
    let command = command.event().clone();
    commands.queue(move |world: &mut World| apply_command(world, command));
}

fn close_menu<Ctx: ActionMenuContext>(world: &mut World) {
    if world.get_resource::<MenuModal<Ctx>>().is_none() {
        return;
    }
    world.resource_scope(|world, mut menu: Mut<MenuModal<Ctx>>| {
        let mut track = world.non_send_mut::<FrontendTrack>();
        let _ = menu.drive(&mut track.shell, KeyCode::Escape, &mut Vec::new());
    });
    world.trigger(MenuModalChanged::<Ctx>(false, PhantomData));
}

fn apply_command(world: &mut World, command: DiagnosticCommand) {
    if matches!(command, DiagnosticCommand::Retry)
        && !matches!(
            world.resource::<DiagnosticModalState>().0,
            Some(DiagnosticBundleUiState::Failed(_))
        )
    {
        return;
    }
    if let DiagnosticCommand::Scroll(delta) = command {
        let mut query =
            world.query_filtered::<(&ComputedNode, &mut ScrollPosition), With<DiagnosticBody>>();
        for (node, mut scroll) in query.iter_mut(world) {
            let maximum =
                ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.0);
            scroll.0.y = (scroll.0.y + delta).clamp(0.0, maximum);
        }
        return;
    }
    match command {
        DiagnosticCommand::Open | DiagnosticCommand::Retry => {
            world.resource_mut::<DiagnosticRuntime>().close();
            close_menu::<ProcessMenuCtx>(world);
            close_menu::<ServiceMenuCtx>(world);
            close_menu::<StartupMenuCtx>(world);
            close_menu::<SessionMenuCtx>(world);
            let state = {
                let mut track = world.non_send_mut::<FrontendTrack>();
                track.shell.dismiss_overlay();
                track.shell.close_service_log();
                track.shell.dismiss_informational_overlay();
                track.shell.close_search();
                DiagnosticBundleUiState::prepared(
                    track.shell.projection().prepare_diagnostic_bundle(None),
                )
            };
            world.resource_mut::<DiagnosticModalState>().0 = Some(state);
            world.trigger(ShellInteractionApplied);
        }
        DiagnosticCommand::Failure(error) => {
            world.resource_mut::<DiagnosticRuntime>().close();
            world.resource_mut::<DiagnosticModalState>().0 =
                Some(DiagnosticBundleUiState::Failed(error));
        }
        DiagnosticCommand::Confirm => {
            let timestamp = world
                .non_send::<FrontendTrack>()
                .shell
                .projection()
                .snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.timestamp_ms);
            let target = DiagnosticBundleTarget::current_directory(format!(
                "taskmanager-diagnostics-{timestamp}.json"
            ));
            world.resource_scope(|world, mut runtime: Mut<DiagnosticRuntime>| {
                if let Some(state) = &mut world.resource_mut::<DiagnosticModalState>().0 {
                    state.confirm(runtime.0.as_mut(), target);
                }
            });
        }
        DiagnosticCommand::Scroll(_) => {}
        DiagnosticCommand::Close => {
            world.resource_mut::<DiagnosticRuntime>().close();
            world.resource_mut::<DiagnosticModalState>().0 = None;
        }
    }
    world.trigger(DiagnosticModalChanged);
}

fn drain(world: &mut World) {
    let completions = world
        .resource_mut::<DiagnosticRuntime>()
        .0
        .as_mut()
        .map_or_else(Vec::new, DiagnosticBundleSession::drain);
    let mut changed = false;
    for completion in completions {
        if let Some(state) = &mut world.resource_mut::<DiagnosticModalState>().0 {
            changed |= state.complete(completion);
        }
    }
    if changed {
        world.trigger(DiagnosticModalChanged);
    }
}

/// Startup commands may arrive before the root scene. Visibility remains in
/// the state owner; mounting waits for the root rather than losing the event.
fn mount_when_root_ready(
    state: Res<DiagnosticModalState>,
    roots: Query<Entity, With<AppShellRoot>>,
    overlays: Query<Entity, With<DiagnosticModalOverlay>>,
    mut commands: Commands,
) {
    if state.0.is_some() && !roots.is_empty() && overlays.is_empty() {
        commands.trigger(DiagnosticModalChanged);
    }
}

fn on_changed(
    _changed: On<DiagnosticModalChanged>,
    state: Res<DiagnosticModalState>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<AppShellRoot>>,
    overlays: Query<Entity, With<DiagnosticModalOverlay>>,
    mut commands: Commands,
) {
    for entity in &overlays {
        commands.entity(entity).despawn();
    }
    let (Some(state), Some(palette), Ok(root)) = (&state.0, palette, roots.single()) else {
        return;
    };
    let overlay = commands
        .spawn_scene(overlay_scene(state, &palette.inner))
        .id();
    commands.entity(root).add_one_related::<ChildOf>(overlay);
}

fn activate(activate: On<Activate>, controls: Query<&DiagnosticControl>, mut commands: Commands) {
    if let Ok(control) = controls.get(activate.entity) {
        commands.trigger(control.0.clone());
    }
}

pub(crate) fn diagnostic_button_scene(palette: &UiPalette) -> impl Scene + use<> {
    action_scene(t("diagnostics.action"), DiagnosticCommand::Open, palette)
}

fn action_scene(
    label: &'static str,
    command: DiagnosticCommand,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node { min_height: px(palette.control_height_px), padding: UiRect::all(Val::Px(space_8())) }
        BackgroundColor({ palette.content_bg })
        Button
        DiagnosticControl({ command })
        on(activate)
        Children [ Text(label) TextRole(Role::Body) Pickable::IGNORE ]
    }
}

fn overlay_scene(state: &DiagnosticBundleUiState, palette: &UiPalette) -> impl Scene + use<> {
    let text = match state {
        DiagnosticBundleUiState::Preview(plan) => diagnostic_preview_text(plan.preview()),
        DiagnosticBundleUiState::Writing(preview) => format!(
            "{}\n{}",
            t("diagnostics.writing"),
            diagnostic_preview_text(preview)
        ),
        DiagnosticBundleUiState::Complete(path) => {
            t("diagnostics.complete").replace("{path}", &path.display().to_string())
        }
        DiagnosticBundleUiState::Failed(error) => diagnostic_failure_message(error),
    };
    let action: Vec<Box<dyn Scene>> = match state {
        DiagnosticBundleUiState::Preview(_) => vec![Box::new(action_scene(
            t("diagnostics.export"),
            DiagnosticCommand::Confirm,
            palette,
        ))],
        DiagnosticBundleUiState::Failed(_) => vec![Box::new(action_scene(
            t("first_run.retry"),
            DiagnosticCommand::Retry,
            palette,
        ))],
        DiagnosticBundleUiState::Writing(_) | DiagnosticBundleUiState::Complete(_) => Vec::new(),
    };
    let close = action_scene(t("common.close"), DiagnosticCommand::Close, palette);
    bsn! {
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center, align_items: AlignItems::Center }
        BackgroundColor({ palette.scrim })
        DiagnosticModalOverlay
        Children [
            Node { width: percent(90), max_width: px(680.0), height: percent(90), max_height: percent(90), min_height: px(0.0), min_width: px(0.0),
                flex_direction: FlexDirection::Column, row_gap: Val::Px(space_8()), padding: UiRect::all(Val::Px(space_24())),
                border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)) }
            BackgroundColor({ palette.panel_fill })
            Children [
                Node { min_height: px(palette.control_height_px), flex_shrink: 0.0 }
                DiagnosticHeading
                Children [ Text(t("diagnostics.title")) TextRole(Role::Heading) ] --
                Node { flex_grow: 1.0, flex_basis: px(0.0), min_height: px(0.0), min_width: px(0.0), overflow: Overflow::scroll_y() }
                ScrollArea
                DiagnosticBody
                Children [ Text(text) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::AnyCharacter } Node { min_width: px(0.0), width: percent(100), max_width: percent(100) } ] --
                Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(space_8()), min_height: px(palette.control_height_px), flex_shrink: 0.0 }
                DiagnosticFooter
                Children [ { action } -- @{ close } ]
            ]
        ]
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/pages/diagnostic_modal.rs"]
mod tests;
