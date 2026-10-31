//! Diagnostic review, input ownership and correlated background publication.

use bevy::app::{App, PostUpdate, PreUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{Justify, LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, Display, FlexDirection, JustifyContent, Node, Overflow, UiRect,
    Val, percent, px, rem,
};
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, ScrollPosition};
use bevy::ui_widgets::{Activate, Button};
use taskforest_core::core::diagnostics::{DiagnosticBundleError, DiagnosticBundleErrorKind};
use taskmanager_app_host::DiagnosticBundleClient;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::i18n::t;
use taskmanager_application::{DiagnosticBundleSession, DiagnosticBundleTarget};
use taskmanager_shell::presentation::diagnostics::{
    diagnostic_failure_message, diagnostic_preview_text,
};

use crate::palette::{UiPalette, space_8, space_12};
use crate::window::{Role, TextRole};
use crate::window_surface::{
    ModalBody, SurfaceAccess, WindowSurface, WindowSurfaceChanged, WindowSurfaceKind,
    WindowSurfaceState, close, modal_scene, show,
};

#[derive(Event, Clone, Debug, Default)]
pub(crate) enum DiagnosticCommand {
    Open,
    Failure(DiagnosticBundleError),
    Confirm,
    Retry,
    /// Open or close the raw provider report inside the failure surface.
    Details,
    Scroll(f32),
    #[default]
    Close,
}

/// Whether the raw provider report inside the failure surface is open.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RawReportDisclosure {
    /// The raw report stays closed; only the typed reason is stated.
    #[default]
    Closed,
    /// The raw provider report is open beside the typed reason.
    Open,
}

/// The marker on the centred troubleshooting surface of a failed export.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiagnosticFailureGuidance;

/// The marker on the raw provider report inside the troubleshooting surface.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiagnosticRawReport;

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
        .init_resource::<RawReportDisclosure>()
        .add_observer(on_command)
        .add_systems(PreUpdate, drain)
        .add_systems(PostUpdate, apply_raw_report);
}

#[derive(SystemParam)]
struct DiagnosticAccess<'w, 's> {
    surface: SurfaceAccess<'w>,
    bodies: Query<'w, 's, (&'static ComputedNode, &'static mut ScrollPosition), With<ModalBody>>,
    commands: Commands<'w, 's>,
    disclosure: ResMut<'w, RawReportDisclosure>,
}
fn on_command(command: On<DiagnosticCommand>, mut access: DiagnosticAccess) {
    let command = command.event().clone();
    if matches!(command, DiagnosticCommand::Retry)
        && !matches!(
            access.surface.state.diagnostic(),
            Some(DiagnosticBundleUiState::Failed(_))
        )
    {
        return;
    }
    match command {
        DiagnosticCommand::Open | DiagnosticCommand::Retry => {
            let plan = access
                .surface
                .track
                .as_ref()
                .map(|track| track.shell.projection().prepare_diagnostic_bundle(None))
                .unwrap_or_else(|| {
                    Err(DiagnosticBundleError::new(
                        DiagnosticBundleErrorKind::Unavailable,
                    ))
                });
            show(
                &mut access.surface,
                &mut access.commands,
                WindowSurface::Diagnostic(DiagnosticBundleUiState::prepared(plan)),
            );
        }
        DiagnosticCommand::Failure(error) => {
            show(
                &mut access.surface,
                &mut access.commands,
                WindowSurface::Diagnostic(DiagnosticBundleUiState::Failed(error)),
            );
        }
        DiagnosticCommand::Confirm => {
            let timestamp = access
                .surface
                .track
                .as_ref()
                .and_then(|track| track.shell.projection().snapshot.as_ref())
                .map_or(0, |snapshot| snapshot.timestamp_ms);
            let target = DiagnosticBundleTarget::current_directory(format!(
                "taskmanager-diagnostics-{timestamp}.json"
            ));
            let runtime = access
                .surface
                .diagnostic
                .as_mut()
                .and_then(|runtime| runtime.0.as_mut());
            if let Some(state) = access.surface.state.diagnostic_mut() {
                state.confirm(runtime, target);
            }
        }
        DiagnosticCommand::Details => {
            // The raw report belongs to the failure surface alone; a preview
            // or a completed write has no provider diagnostic to disclose.
            if !matches!(
                access.surface.state.diagnostic(),
                Some(DiagnosticBundleUiState::Failed(_))
            ) {
                return;
            }
            *access.disclosure = match *access.disclosure {
                RawReportDisclosure::Closed => RawReportDisclosure::Open,
                RawReportDisclosure::Open => RawReportDisclosure::Closed,
            };
            return;
        }
        DiagnosticCommand::Scroll(delta) => {
            for (node, mut scroll) in &mut access.bodies {
                let maximum = ((node.content_size().y - node.size().y)
                    * node.inverse_scale_factor())
                .max(0.0);
                scroll.0.y = (scroll.0.y + delta).clamp(0.0, maximum);
            }
            return;
        }
        DiagnosticCommand::Close => close(
            &mut access.surface,
            &mut access.commands,
            WindowSurfaceKind::Diagnostic,
        ),
    }
    access.commands.trigger(WindowSurfaceChanged);
}
fn drain(
    mut runtime: ResMut<DiagnosticRuntime>,
    mut surface: ResMut<WindowSurfaceState>,
    mut commands: Commands,
) {
    let completions = runtime
        .0
        .as_mut()
        .map_or_else(Vec::new, DiagnosticBundleSession::drain);
    let mut changed = false;
    for completion in completions {
        if let Some(state) = surface.diagnostic_mut() {
            changed |= state.complete(completion);
        }
    }
    if changed {
        commands.trigger(WindowSurfaceChanged);
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
        Node { min_height: rem(palette.control_height_rem), padding: UiRect::all(Val::Px(space_8())) }
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
        DiagnosticBundleUiState::Failed(error) => {
            let mut actions: Vec<Box<dyn Scene>> = vec![Box::new(action_scene(
                t("first_run.retry"),
                DiagnosticCommand::Retry,
                palette,
            )) as Box<dyn Scene>];
            if error.detail().is_some() {
                actions.push(Box::new(action_scene(
                    t("common.details"),
                    DiagnosticCommand::Details,
                    palette,
                )) as Box<dyn Scene>);
            }
            actions
        }
        DiagnosticBundleUiState::Writing(_) | DiagnosticBundleUiState::Complete(_) => Vec::new(),
    };
    let close_button = action_scene(t("common.close"), DiagnosticCommand::Close, palette);
    let body: Box<dyn Scene> = match state {
        DiagnosticBundleUiState::Failed(error) => failure_guidance_scene(error),
        _ => Box::new(
            bsn! { Text(text) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::AnyCharacter } Node { min_width: px(0.0), width: percent(100), max_width: percent(100) } },
        ),
    };
    let mut actions = action;
    actions.push(Box::new(close_button));
    modal_scene(
        WindowSurfaceKind::Diagnostic,
        t("diagnostics.title"),
        body,
        actions,
        palette,
    )
}

/// The centred troubleshooting surface of a failed diagnostic export.
///
/// The single-line error is gone. The surface states the failure, the stable
/// code a maintainer can quote, and — behind the typed disclosure — the raw
/// provider report; the retry and close actions keep the shared typed path.
fn failure_guidance_scene(error: &DiagnosticBundleError) -> Box<dyn Scene> {
    let reason = diagnostic_failure_message(error);
    let code = error.kind().stable_code();
    let mut children: Vec<Box<dyn Scene>> = vec![
        Box::new(bsn! {
            Text({ t("diagnostics.failed").to_owned() }) TextRole(Role::Heading)
            TextLayout { justify: Justify::Center, linebreak: LineBreak::NoWrap }
        }) as Box<dyn Scene>,
        Box::new(bsn! {
            Text({ reason }) TextRole(Role::Body)
            TextLayout { justify: Justify::Center, linebreak: LineBreak::WordBoundary }
            Node { width: percent(100), min_width: px(0.0), overflow: Overflow::clip_x() }
        }) as Box<dyn Scene>,
        Box::new(bsn! {
            Text({ code.to_owned() }) TextRole(Role::Mono)
            TextLayout { justify: Justify::Center, linebreak: LineBreak::NoWrap }
        }) as Box<dyn Scene>,
    ];
    if let Some(detail) = error.detail() {
        children.push(Box::new(bsn! {
            Node { width: percent(100), min_width: px(0.0), display: Display::None }
            DiagnosticRawReport
            Children [
                Text({ detail.to_owned() }) TextRole(Role::Mono)
                TextLayout { justify: Justify::Center, linebreak: LineBreak::AnyCharacter }
            ]
        }));
    }
    Box::new(bsn! {
        Node {
            width: percent(100),
            min_width: px(0.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_12())),
        }
        DiagnosticFailureGuidance
        Children [ { children } ]
    })
}

/// Apply the raw-report disclosure to the troubleshooting surface.
///
/// The disclosure is a typed state, not a scene rebuild: the surface keeps
/// its measured geometry while the raw report opens and closes inside it.
fn apply_raw_report(
    disclosure: Res<RawReportDisclosure>,
    mut reports: Query<&mut Node, With<DiagnosticRawReport>>,
) {
    let display = match *disclosure {
        RawReportDisclosure::Closed => Display::None,
        RawReportDisclosure::Open => Display::Flex,
    };
    for mut node in &mut reports {
        if node.display != display {
            node.display = display;
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/pages/diagnostic_modal.rs"]
mod tests;
