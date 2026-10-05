//! Explicit optional setup review, native actions and shared progress facts.

use crate::input::PendingEffects;
use crate::pages::settings::SettingsBody;
use crate::palette::{UiPalette, space_8};
use crate::text_selection::ClipboardPort;
use crate::window::{Role, TextRole, WindowPalette};
use crate::window_surface::{
    ModalBody, WindowSurface, WindowSurfaceCommand, WindowSurfaceKind, WindowSurfaceState,
    modal_scene,
};
use bevy::app::{App, Update};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use bevy::picking::Pickable;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{BackgroundColor, FlexDirection, Node, UiRect, Val, percent, px};
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, ScrollPosition};
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::first_run::{FirstRunController, FirstRunPhase, FirstRunUiState};
use taskmanager_application::i18n::t;
use taskmanager_application::{PlatformEffect, SetupScriptRequest, UrlOpenRequest};
use taskmanager_assets::product::REPOSITORY_URL;
use taskmanager_core::core::setup::SetupScriptAction;
use taskmanager_shell::presentation::first_run::first_run_phase_key;

#[derive(Resource, Default)]
pub(crate) struct SetupState(pub(crate) FirstRunController);
#[derive(Event, Clone, Debug, Default)]
pub(crate) enum FirstRunCommand {
    #[default]
    Open,
    Close,
    Action(SetupScriptAction),
    Documentation,
    Copy(u8),
    Scroll(f32),
}
#[derive(Component, Clone, Default)]
pub(crate) struct SetupControl(pub(crate) FirstRunCommand);
pub(crate) fn register(app: &mut App) {
    app.init_resource::<SetupState>()
        .add_observer(on_command)
        .add_systems(Update, sync_setup_entry);
}
#[derive(Component, Clone, Default)]
struct SetupEntry;
fn sync_setup_entry(
    state: Res<SetupState>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<SettingsBody>>,
    entries: Query<Entity, With<SetupEntry>>,
    mut commands: Commands,
) {
    if state.0.view().info.is_none() || roots.is_empty() {
        for entity in &entries {
            commands.entity(entity).despawn();
        }
        return;
    }
    if !entries.is_empty() {
        return;
    }
    let (Some(palette), Ok(root)) = (palette, roots.single()) else {
        return;
    };
    let button = action_scene(
        t("settings.additional_setup_open"),
        FirstRunCommand::Open,
        &palette.inner,
    );
    let entry = commands.spawn_scene(bsn! { Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(space_8()) } SetupEntry
        Children [ Text(t("settings.additional_setup")) TextRole(Role::Heading) -- Text(t("settings.additional_setup_detail")) TextRole(Role::Caption) -- @{ button } ] }).id();
    commands.entity(root).add_one_related::<ChildOf>(entry);
}

#[derive(SystemParam)]
struct SetupCommandAccess<'w, 's> {
    surface: Res<'w, WindowSurfaceState>,
    setup: Res<'w, SetupState>,
    pending: ResMut<'w, PendingEffects>,
    clipboard: Option<ResMut<'w, ClipboardPort>>,
    bodies: Query<'w, 's, (&'static ComputedNode, &'static mut ScrollPosition), With<ModalBody>>,
    commands: Commands<'w, 's>,
}
fn on_command(command: On<FirstRunCommand>, mut access: SetupCommandAccess) {
    let command = command.event().clone();
    if matches!(command, FirstRunCommand::Open) {
        access.commands.trigger(WindowSurfaceCommand::FirstRun);
        return;
    }
    if !matches!(access.surface.0, Some(WindowSurface::FirstRun)) {
        return;
    }
    match command {
        FirstRunCommand::Open => {}
        FirstRunCommand::Close => access
            .commands
            .trigger(WindowSurfaceCommand::Close(WindowSurfaceKind::FirstRun)),
        FirstRunCommand::Action(action) => access
            .pending
            .0
            .push(PlatformEffect::SetupScript(SetupScriptRequest { action })),
        FirstRunCommand::Documentation => {
            access
                .pending
                .0
                .push(PlatformEffect::OpenUrl(UrlOpenRequest {
                    url: REPOSITORY_URL.into(),
                }))
        }
        FirstRunCommand::Copy(field) => {
            let payload = access
                .setup
                .0
                .view()
                .info
                .as_ref()
                .and_then(|info| match field {
                    0 => Some(info.path.display().to_string()),
                    1 => Some(info.run_command.clone()),
                    2 => Some(info.revert_command.clone()),
                    _ => None,
                });
            if let Some(payload) = payload
                && let Some(clipboard) = access.clipboard.as_mut()
            {
                clipboard.request_text(payload, t("first_run.title"));
            }
        }
        FirstRunCommand::Scroll(delta) => {
            for (node, mut scroll) in &mut access.bodies {
                let maximum = ((node.content_size().y - node.size().y)
                    * node.inverse_scale_factor())
                .max(0.0);
                scroll.0.y = (scroll.0.y + delta).clamp(0.0, maximum);
            }
        }
    }
}

fn activate(activate: On<Activate>, controls: Query<&SetupControl>, mut commands: Commands) {
    if let Ok(control) = controls.get(activate.entity) {
        commands.trigger(control.0.clone());
    }
}
pub(crate) fn action_scene(
    label: &'static str,
    command: FirstRunCommand,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! { Node { min_height: px(palette.control_height_px), padding: UiRect::all(Val::Px(space_8())) }
    BackgroundColor({ palette.content_bg }) Button SetupControl({ command }) on(activate)
    Children [ Text(label) TextRole(Role::Body) Pickable::IGNORE ] }
}
pub(crate) fn surface_scene(view: &FirstRunUiState, palette: &UiPalette) -> impl Scene + use<> {
    let mut body: Vec<Box<dyn Scene>> = vec![Box::new(
        bsn! { Text(t("first_run.description")) TextRole(Role::Body) },
    )];
    if let Some(info) = &view.info {
        for (label, value, copy, index) in [
            (
                t("first_run.location"),
                info.path.display().to_string(),
                t("first_run.copy_location"),
                0,
            ),
            (
                t("first_run.run_command"),
                info.run_command.clone(),
                t("first_run.copy_command"),
                1,
            ),
            (
                t("first_run.revert_command"),
                info.revert_command.clone(),
                t("first_run.copy_revert_command"),
                2,
            ),
        ] {
            let copy = action_scene(copy, FirstRunCommand::Copy(index), palette);
            body.push(Box::new(bsn! { Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column }
                Children [ Text(label) TextRole(Role::Caption) -- Text(value) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::AnyCharacter } Node { min_width: px(0.0), max_width: percent(100) } -- @{ copy } ] }));
        }
    }
    if let Some(key) = first_run_phase_key(&view.phase) {
        body.push(Box::new(bsn! { Text(t(key)) TextRole(Role::Body) }));
    }
    let body = bsn! { Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(space_8()) } Children [ { body } ] };
    let mut actions: Vec<Box<dyn Scene>> = Vec::new();
    if !view.action_pending() {
        for (label, command) in [
            (t("first_run.open_docs"), FirstRunCommand::Documentation),
            (
                t("first_run.view_script"),
                FirstRunCommand::Action(SetupScriptAction::View),
            ),
            (
                t("first_run.run_setup"),
                FirstRunCommand::Action(SetupScriptAction::Run),
            ),
            (
                t("first_run.revert_setup"),
                FirstRunCommand::Action(SetupScriptAction::Revert),
            ),
        ] {
            actions.push(Box::new(action_scene(label, command, palette)));
        }
        if view.phase == FirstRunPhase::RestartRequired {
            actions.push(Box::new(action_scene(
                t("first_run.restart"),
                FirstRunCommand::Action(SetupScriptAction::Restart),
                palette,
            )));
        }
        if matches!(view.phase, FirstRunPhase::Failed(_))
            && let Some(action @ (SetupScriptAction::Run | SetupScriptAction::Revert)) =
                view.last_action
        {
            actions.push(Box::new(action_scene(
                t("first_run.retry"),
                FirstRunCommand::Action(action),
                palette,
            )));
        }
    }
    actions.push(Box::new(action_scene(
        t("common.close"),
        FirstRunCommand::Close,
        palette,
    )));
    modal_scene(
        WindowSurfaceKind::FirstRun,
        t("first_run.title"),
        Box::new(body),
        actions,
        palette,
    )
}

#[cfg(test)]
#[path = "../tests/headless/first_run_modal.rs"]
mod tests;
