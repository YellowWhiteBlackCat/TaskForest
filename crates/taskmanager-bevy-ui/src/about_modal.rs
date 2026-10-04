//! About reads cached facts and uses the same primary surface owner as setup/export.

use crate::app::FrontendTrack;
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
use bevy::ecs::system::{Commands, Query};
use bevy::ecs::world::World;
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::widget::Text;
use bevy::ui::{BackgroundColor, Node, UiRect, Val, percent, px};
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_assets::product;
use taskmanager_shell::presentation::{duration, missing_value};

#[derive(Event, Clone, Copy, Default)]
pub(crate) enum AboutCommand {
    #[default]
    Open,
    Close,
    Copy,
    Diagnostics,
}
#[derive(Component, Clone, Default)]
pub(crate) struct AboutControl(pub(crate) AboutCommand);
pub(crate) fn register(app: &mut App) {
    app.add_observer(on_command);
}
fn on_command(command: On<AboutCommand>, mut commands: Commands) {
    match command.event() {
        AboutCommand::Open => commands.trigger(WindowSurfaceCommand::About),
        AboutCommand::Close => {
            commands.trigger(WindowSurfaceCommand::Close(WindowSurfaceKind::About))
        }
        AboutCommand::Copy => commands.queue(|world: &mut World| {
            if !matches!(
                world.resource::<WindowSurfaceState>().0,
                Some(WindowSurface::About)
            ) {
                return;
            }
            let payload = about_text(world.non_send::<FrontendTrack>());
            if let Some(mut clipboard) = world.get_resource_mut::<ClipboardPort>() {
                clipboard.request_text(payload, t("about.title"));
            }
        }),
        AboutCommand::Diagnostics => commands.queue(|world: &mut World| {
            if matches!(
                world.resource::<WindowSurfaceState>().0,
                Some(WindowSurface::About)
            ) {
                world.trigger(DiagnosticCommand::Open);
            }
        }),
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
fn about_text(track: &FrontendTrack) -> String {
    let projection = track.shell.projection();
    let hardware = projection.hardware.as_ref();
    let snapshot = projection.snapshot.as_ref();
    let fact = |key, value: Option<&str>| {
        format!(
            "{}: {}",
            t(key),
            value.map_or_else(missing_value, str::to_owned)
        )
    };
    let rows = [
        fact(
            "system.hostname",
            hardware.and_then(|hardware| hardware.hostname.as_deref()),
        ),
        fact(
            "system.field.os_name",
            hardware.and_then(|hardware| hardware.os_name.as_deref()),
        ),
        fact(
            "system.field.os_version",
            hardware.and_then(|hardware| hardware.os_version.as_deref()),
        ),
        fact(
            "system.kernel",
            hardware.and_then(|hardware| hardware.kernel_version.as_deref()),
        ),
        fact(
            "system.field.architecture",
            hardware.and_then(|hardware| hardware.architecture.as_deref()),
        ),
        fact(
            "system.field.motherboard_vendor",
            hardware.and_then(|hardware| hardware.motherboard_vendor.as_deref()),
        ),
        fact(
            "system.field.motherboard_model",
            hardware.and_then(|hardware| hardware.motherboard_model.as_deref()),
        ),
        fact(
            "system.field.firmware_release_date",
            hardware.and_then(|hardware| hardware.firmware_release_date.as_deref()),
        ),
        fact(
            "system.secure_boot",
            hardware
                .and_then(|hardware| hardware.secure_boot)
                .map(|enabled| {
                    if enabled {
                        t("common.enabled")
                    } else {
                        t("common.disabled")
                    }
                }),
        ),
        fact(
            "common.cpu",
            hardware.and_then(|hardware| hardware.cpu_brand.as_deref()),
        ),
        format!(
            "{}: {}",
            t("common.logical_cores"),
            hardware
                .and_then(|hardware| hardware.cpu_cores)
                .map_or_else(missing_value, |value| value.to_string())
        ),
        format!(
            "{}: {}",
            t("system.field.installed_memory"),
            hardware
                .and_then(|hardware| hardware.total_memory_mb)
                .map_or_else(missing_value, |value| format!("{value} MiB"))
        ),
        format!(
            "{}: {}",
            t("common.uptime"),
            snapshot.map_or_else(missing_value, |snapshot| duration(snapshot.uptime_secs))
        ),
    ];
    format!(
        "{} {}\n\n{}",
        product::BEVY_NAME,
        env!("CARGO_PKG_VERSION"),
        rows.join("\n\n")
    )
}
pub(crate) fn surface_scene(track: &FrontendTrack, palette: &UiPalette) -> impl Scene + use<> {
    let text = about_text(track);
    let body = bsn! { Text(text) TextRole(Role::Body) TextLayout { linebreak: LineBreak::AnyCharacter } Node { width: percent(100), min_width: px(0.0), max_width: percent(100) } };
    let actions: Vec<Box<dyn Scene>> = vec![
        Box::new(action_scene(t("common.copy"), AboutCommand::Copy, palette)),
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
