//! Independent, frozen review of cached system-information facts.

use crate::about_modal::{AboutCommand, action_scene};
use crate::palette::{UiPalette, space_8};
use crate::text_selection::ClipboardPort;
use crate::window::{Role, TextRole};
use crate::window_surface::{
    ModalBody, WindowSurface, WindowSurfaceCommand, WindowSurfaceKind, WindowSurfaceState,
    modal_scene,
};
use bevy::app::App;
use bevy::ecs::component::Component;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::Commands;
use bevy::ecs::system::Query;
use bevy::ecs::system::{Res, ResMut};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::widget::Text;
use bevy::ui::{BackgroundColor, UiRect};
use bevy::ui::{ComputedNode, FlexDirection, Node, ScrollPosition, Val, percent, px};
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::system_information::{SystemInformationGroup, copy_all_text};

#[derive(Event, Clone, Copy, Default)]
pub(crate) enum SystemInformationCommand {
    Open,
    #[default]
    Close,
    Copy,
    Scroll(f32),
}

pub(crate) fn register(app: &mut App) {
    app.add_observer(on_command);
}
fn on_command(
    event: On<SystemInformationCommand>,
    state: Res<WindowSurfaceState>,
    mut clipboard: ResMut<ClipboardPort>,
    mut bodies: Query<(&ComputedNode, &mut ScrollPosition), With<ModalBody>>,
    mut commands: Commands,
) {
    let command = *event.event();
    if matches!(command, SystemInformationCommand::Open) {
        commands.trigger(WindowSurfaceCommand::SystemInformation);
        return;
    }
    let Some(WindowSurface::SystemInformation(facts)) = &state.0 else {
        return;
    };
    match command {
        SystemInformationCommand::Open => {}
        SystemInformationCommand::Close => commands.trigger(WindowSurfaceCommand::Close(
            WindowSurfaceKind::SystemInformation,
        )),
        SystemInformationCommand::Copy => {
            clipboard.request_text(copy_all_text(facts), t("system_about.title"))
        }
        SystemInformationCommand::Scroll(delta) => {
            for (node, mut scroll) in &mut bodies {
                let maximum = ((node.content_size().y - node.size().y)
                    * node.inverse_scale_factor())
                .max(0.0);
                scroll.0.y = (scroll.0.y + delta).clamp(0.0, maximum);
            }
        }
    }
}

#[derive(Component, Clone, Default)]
struct SystemInformationControl(SystemInformationCommand);
fn activate(
    activate: On<Activate>,
    controls: Query<&SystemInformationControl>,
    mut commands: Commands,
) {
    if let Ok(control) = controls.get(activate.entity) {
        commands.trigger(control.0);
    }
}
fn control_scene(
    label: &'static str,
    command: SystemInformationCommand,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! { Node { min_height: px(palette.control_height_px), padding: UiRect::all(Val::Px(space_8())) } BackgroundColor({ palette.content_bg }) Button SystemInformationControl({ command }) on(activate)
    Children [ Text(label) TextRole(Role::Body) Pickable::IGNORE ] }
}

pub(crate) fn surface_scene(
    facts: &[SystemInformationGroup],
    palette: &UiPalette,
) -> impl Scene + use<> {
    let text = if facts.is_empty() {
        t("system_about.unavailable").to_owned()
    } else {
        copy_all_text(facts)
    };
    let body = bsn! { Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(space_8()) }
    Children [ Text(text) TextRole(Role::Body) TextLayout { linebreak: LineBreak::AnyCharacter } Node { width: percent(100), min_width: px(0.0), max_width: percent(100) } ] };
    let actions: Vec<Box<dyn Scene>> = vec![
        Box::new(control_scene(
            t("system_about.copy_all"),
            SystemInformationCommand::Copy,
            palette,
        )),
        Box::new(control_scene(
            t("common.close"),
            SystemInformationCommand::Close,
            palette,
        )),
        Box::new(action_scene(t("about.title"), AboutCommand::Open, palette)),
    ];
    modal_scene(
        WindowSurfaceKind::SystemInformation,
        t("system_about.title"),
        Box::new(body),
        actions,
        palette,
    )
}
