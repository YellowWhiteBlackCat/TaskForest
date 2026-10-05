//! About reads cached facts and uses the same primary surface owner as setup/export.

use crate::input::PendingEffects;
use crate::pages::system::diagnostic_modal::DiagnosticCommand;
use crate::palette::{UiPalette, space_8};
use crate::text_selection::ClipboardPort;
use crate::window::{Role, TextRole};
use crate::window_surface::{
    WindowSurface, WindowSurfaceCommand, WindowSurfaceKind, WindowSurfaceState, modal_scene,
};
use bevy::app::App;
use bevy::ecs::component::Component;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::widget::Text;
use bevy::ui::{BackgroundColor, Node, UiRect, Val, percent, px};
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_application::{PlatformEffect, UrlOpenRequest};
use taskmanager_assets::product;
use taskmanager_assets::product::REPOSITORY_URL;
use taskmanager_shell::presentation::about::metadata;

#[derive(Event, Clone, Copy, Default)]
pub(crate) enum AboutCommand {
    #[default]
    Open,
    Close,
    Copy,
    Diagnostics,
    Repository,
    SystemInformation,
}
#[derive(Component, Clone, Default)]
pub(crate) struct AboutControl(pub(crate) AboutCommand);
pub(crate) fn register(app: &mut App) {
    app.add_observer(on_command);
}
fn on_command(
    command: On<AboutCommand>,
    state: Res<WindowSurfaceState>,
    mut clipboard: Option<ResMut<ClipboardPort>>,
    mut pending: Option<ResMut<PendingEffects>>,
    mut commands: Commands,
) {
    let about_open = matches!(state.0, Some(WindowSurface::About));
    match command.event() {
        AboutCommand::Open => commands.trigger(WindowSurfaceCommand::About),
        AboutCommand::Close => {
            commands.trigger(WindowSurfaceCommand::Close(WindowSurfaceKind::About))
        }
        AboutCommand::Copy if about_open => {
            if let Some(clipboard) = clipboard.as_mut() {
                clipboard.request_text(about_text(), t("about.title"));
            }
        }
        AboutCommand::Repository if about_open => {
            if let Some(pending) = pending.as_mut() {
                pending.0.push(PlatformEffect::OpenUrl(UrlOpenRequest {
                    url: REPOSITORY_URL.into(),
                }));
            }
        }
        AboutCommand::SystemInformation => {
            commands.trigger(WindowSurfaceCommand::SystemInformation)
        }
        AboutCommand::Diagnostics if about_open => commands.trigger(DiagnosticCommand::Open),
        _ => {}
    }
}

fn activate(activate: On<Activate>, controls: Query<&AboutControl>, mut commands: Commands) {
    if let Ok(control) = controls.get(activate.entity) {
        commands.trigger(control.0);
    }
}
pub(crate) fn action_scene(
    label: &'static str,
    command: AboutCommand,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! { Node { min_height: px(palette.control_height_px), padding: UiRect::all(Val::Px(space_8())) }
    BackgroundColor({ palette.content_bg }) Button AboutControl({ command }) on(activate)
    Children [ Text(label) TextRole(Role::Body) Pickable::IGNORE ] }
}
fn about_text() -> String {
    metadata(
        env!("CARGO_PKG_VERSION"),
        product::LICENSE_SPDX,
        product::REPOSITORY_URL,
    )
    .details_text()
}
pub(crate) fn surface_scene(palette: &UiPalette) -> impl Scene + use<> {
    let text = about_text();
    let body = bsn! { Text(text) TextRole(Role::Body) TextLayout { linebreak: LineBreak::AnyCharacter } Node { width: percent(100), min_width: px(0.0), max_width: percent(100) } };
    let actions: Vec<Box<dyn Scene>> = vec![
        Box::new(action_scene(
            t("about.open_repository"),
            AboutCommand::Repository,
            palette,
        )),
        Box::new(action_scene(
            t("system_about.title"),
            AboutCommand::SystemInformation,
            palette,
        )),
        Box::new(action_scene(
            t("about.copy_details"),
            AboutCommand::Copy,
            palette,
        )),
        Box::new(action_scene(
            t("diagnostics.action"),
            AboutCommand::Diagnostics,
            palette,
        )),
        Box::new(action_scene(
            t("common.close"),
            AboutCommand::Close,
            palette,
        )),
    ];
    modal_scene(
        WindowSurfaceKind::About,
        t("about.title"),
        Box::new(body),
        actions,
        palette,
    )
}

#[cfg(test)]
#[path = "../tests/headless/about_modal.rs"]
mod tests;
