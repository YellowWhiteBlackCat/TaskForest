//! Diagnostic review, input ownership and correlated background publication.

use bevy::app::{App, PreUpdate};
use bevy::ecs::change_detection::Mut;
use bevy::ecs::component::Component;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query};
use bevy::ecs::world::World;
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{BackgroundColor, Node, UiRect, Val, percent, px};
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, ScrollPosition};
use bevy::ui_widgets::{Activate, Button};
use taskmanager_app_host::DiagnosticBundleClient;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::i18n::t;
use taskmanager_application::{DiagnosticBundleSession, DiagnosticBundleTarget};
use taskmanager_core::core::diagnostics::DiagnosticBundleError;
use taskmanager_shell::presentation::diagnostics::{
    diagnostic_failure_message, diagnostic_preview_text,
};

use crate::app::FrontendTrack;
use crate::palette::{UiPalette, space_8};
use crate::window::{Role, TextRole};
use crate::window_surface::{
    ModalBody, WindowSurface, WindowSurfaceChanged, WindowSurfaceKind, WindowSurfaceState, close,
    modal_scene, show,
};

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
pub(crate) struct DiagnosticRuntime(
    pub(crate) Option<DiagnosticBundleSession<DiagnosticBundleClient>>,
);

impl DiagnosticRuntime {
    pub(crate) fn install(&mut self, client: DiagnosticBundleClient) {
        self.0 = Some(DiagnosticBundleSession::new(client));
    }
    pub(crate) fn close(&mut self) {
        if let Some(session) = &mut self.0 {
            session.close();
        }
    }
}

#[derive(Component, Clone, Default)]
pub(crate) struct DiagnosticControl(pub(crate) DiagnosticCommand);
pub(crate) fn register(app: &mut App) {
    app.init_resource::<WindowSurfaceState>()
        .init_resource::<DiagnosticRuntime>()
        .add_observer(on_command)
        .add_systems(PreUpdate, drain);
}

fn on_command(command: On<DiagnosticCommand>, mut commands: Commands) {
    let command = command.event().clone();
    commands.queue(move |world: &mut World| apply_command(world, command));
}

fn apply_command(world: &mut World, command: DiagnosticCommand) {
    if matches!(command, DiagnosticCommand::Retry)
        && !matches!(
            world.resource::<WindowSurfaceState>().diagnostic(),
            Some(DiagnosticBundleUiState::Failed(_))
        )
    {
        return;
    }
    if let DiagnosticCommand::Scroll(delta) = command {
        let mut query =
            world.query_filtered::<(&ComputedNode, &mut ScrollPosition), With<ModalBody>>();
        for (node, mut scroll) in query.iter_mut(world) {
            let maximum =
                ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.0);
            scroll.0.y = (scroll.0.y + delta).clamp(0.0, maximum);
        }
        return;
    }
    match command {
        DiagnosticCommand::Open | DiagnosticCommand::Retry => {
            let state = DiagnosticBundleUiState::prepared(
                world
                    .non_send::<FrontendTrack>()
                    .shell
                    .projection()
                    .prepare_diagnostic_bundle(None),
            );
            show(world, WindowSurface::Diagnostic(state));
        }
        DiagnosticCommand::Failure(error) => {
            world.resource_mut::<DiagnosticRuntime>().close();
            show(
                world,
                WindowSurface::Diagnostic(DiagnosticBundleUiState::Failed(error)),
            );
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
                if let Some(state) = world.resource_mut::<WindowSurfaceState>().diagnostic_mut() {
                    state.confirm(runtime.0.as_mut(), target);
                }
            });
        }
        DiagnosticCommand::Scroll(_) => {}
        DiagnosticCommand::Close => {
            world.resource_mut::<DiagnosticRuntime>().close();
            close(world, WindowSurfaceKind::Diagnostic);
        }
    }
    world.trigger(WindowSurfaceChanged);
}

fn drain(world: &mut World) {
    let completions = world
        .resource_mut::<DiagnosticRuntime>()
        .0
        .as_mut()
        .map_or_else(Vec::new, DiagnosticBundleSession::drain);
    let mut changed = false;
    for completion in completions {
        if let Some(state) = world.resource_mut::<WindowSurfaceState>().diagnostic_mut() {
            changed |= state.complete(completion);
        }
    }
    if changed {
        world.trigger(WindowSurfaceChanged);
    }
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

pub(crate) fn overlay_scene(
    state: &DiagnosticBundleUiState,
    palette: &UiPalette,
) -> impl Scene + use<> {
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
    let close_button = action_scene(t("common.close"), DiagnosticCommand::Close, palette);
    let body = bsn! { Text(text) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::AnyCharacter } Node { min_width: px(0.0), width: percent(100), max_width: percent(100) } };
    let mut actions = action;
    actions.push(Box::new(close_button));
    modal_scene(
        WindowSurfaceKind::Diagnostic,
        t("diagnostics.title"),
        Box::new(body),
        actions,
        palette,
    )
}

#[cfg(test)]
#[path = "../../../tests/headless/pages/diagnostic_modal.rs"]
mod tests;
