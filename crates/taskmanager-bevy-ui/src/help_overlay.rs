//! Shell-owned keyboard help dialog bound to the shared shortcut catalogs.

use crate::app::FrontendTrack;
use crate::palette::{UiPalette, space_8};
use crate::scene_attach::attach_child;
use crate::window::overlay_root::OverlayRoot;
use crate::window::{Role, TextRole, WindowPalette};
use crate::window_surface::standalone_modal_scene;
use bevy::app::{App, PostStartup, PostUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, NonSend, NonSendMut, Query, Res};
use bevy::picking::Pickable;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderColor, BorderRadius, FlexDirection, JustifyContent, Node,
    Overflow, PositionType, UiRect, Val, percent, px, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_shell::{CommandHelp, PageHelp, command_help, page_help, shell_local_bindings};
use taskmanager_theme::GlassTier;
use taskmanager_theme::tokens;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HelpOverlay;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HelpCloseButton;

#[derive(bevy::ecs::event::Event, Clone, Copy, Debug, Default)]
pub(crate) struct HelpClose;

pub(crate) fn register(app: &mut App) {
    app.add_observer(on_help_close)
        .add_observer(on_activate)
        .add_observer(on_interaction_applied)
        .add_systems(PostStartup, sync_after_mount)
        .add_systems(PostUpdate, sync_help_visibility);
}

fn on_help_close(
    _close: On<HelpClose>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    if track.shell.help_open() {
        track.shell.toggle_help();
        commands.trigger(crate::input::ShellInteractionApplied);
    }
}

fn on_activate(
    activate: On<Activate>,
    buttons: Query<Entity, With<HelpCloseButton>>,
    mut commands: Commands,
) {
    if buttons.get(activate.entity).is_ok() {
        commands.trigger(HelpClose);
    }
}

fn on_interaction_applied(
    _applied: On<crate::input::ShellInteractionApplied>,
    track: NonSend<FrontendTrack>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<OverlayRoot>>,
    overlays: Query<Entity, With<HelpOverlay>>,
    mut commands: Commands,
) {
    show_projection(
        track.shell.help_open(),
        palette.as_deref(),
        &roots,
        &overlays,
        &mut commands,
    );
}

fn sync_after_mount(
    track: NonSend<FrontendTrack>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<OverlayRoot>>,
    overlays: Query<Entity, With<HelpOverlay>>,
    mut commands: Commands,
) {
    show_projection(
        track.shell.help_open(),
        palette.as_deref(),
        &roots,
        &overlays,
        &mut commands,
    );
}

fn sync_help_visibility(
    track: NonSend<FrontendTrack>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<OverlayRoot>>,
    overlays: Query<Entity, With<HelpOverlay>>,
    mut commands: Commands,
) {
    show_projection(
        track.shell.help_open(),
        palette.as_deref(),
        &roots,
        &overlays,
        &mut commands,
    );
}

fn show_projection(
    visible: bool,
    palette: Option<&WindowPalette>,
    roots: &Query<Entity, With<OverlayRoot>>,
    overlays: &Query<Entity, With<HelpOverlay>>,
    commands: &mut Commands,
) {
    let mounted: Vec<Entity> = overlays.iter().collect();
    if visible && !mounted.is_empty() {
        return;
    }
    for entity in mounted {
        commands.entity(entity).despawn();
    }
    if !visible {
        return;
    }
    let (Some(palette), Ok(root)) = (palette, roots.single()) else {
        return;
    };
    let entity = commands.spawn_scene(scene(&palette.inner)).id();
    attach_child(commands, root, entity);
}

fn row_scene(
    shortcut: &str,
    label: &str,
    description: &str,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let description = Box::new(bsn! {
        Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), min_width: px(0.0), max_width: percent(100), overflow: Overflow::clip_x() }
        Children [ Text({ description.to_owned() }) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } ]
    });
    let label = Box::new(bsn! {
        Node { flex_shrink: 0.0, min_width: px(0.0), overflow: Overflow::clip_x() }
        Children [ Text({ label.to_owned() }) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap } ]
    });
    let shortcut_width_rem =
        palette.rem_ratio(tokens::SPACE_48.0 + tokens::SPACE_24.0 + tokens::SPACE_16.0);
    let shortcut = Box::new(bsn! {
        Node { width: rem(shortcut_width_rem), flex_shrink: 0.0, min_width: px(0.0), overflow: Overflow::clip_x() }
        Children [ Text({ shortcut.to_owned() }) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::NoWrap } ]
    });
    let gap = palette.rem_ratio(space_8());
    let padding = palette.rem_ratio(space_8());
    let radius = palette.control_radius_rem();
    let fill = palette.content_bg;
    let edge = palette.glass_edge(GlassTier::Card);
    Box::new(bsn! {
        Node { width: percent(100), min_width: px(0.0), max_width: percent(100), flex_shrink: 0.0,
            flex_direction: FlexDirection::Row, align_items: AlignItems::Center,
            column_gap: Val::Rem(gap), padding: UiRect::all(Val::Rem(padding)),
            border_radius: BorderRadius::all(Val::Rem(radius)) }
        BackgroundColor(fill)
        BorderColor::all(edge)
        Children [ @{ shortcut } -- @{ label } -- @{ description } ]
    })
}

fn section_scene(
    title: &'static str,
    rows: Vec<Box<dyn Scene>>,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let gap = palette.rem_ratio(space_8());
    Box::new(bsn! {
        Node { width: percent(100), min_width: px(0.0), max_width: percent(100), flex_direction: FlexDirection::Column, row_gap: Val::Rem(gap) }
        Children [ Text(title) TextRole(Role::Heading) -- { rows } ]
    })
}

fn body_scene(palette: &UiPalette) -> Box<dyn Scene> {
    let pages = page_help();
    let commands = command_help();
    let declaration = crate::bindings::binding_declaration();
    let mut rows: Vec<Box<dyn Scene>> = pages
        .iter()
        .map(|page: &PageHelp| row_scene(page.shortcut, page.label, "", palette))
        .collect();
    let pages = section_scene(t("settings.keys_pages"), rows, palette);
    rows = Vec::new();
    for entry in declaration.entries {
        let Some(shortcut) = entry.binding.key_token() else {
            continue;
        };
        let Some(help) = commands
            .iter()
            .find(|help: &&CommandHelp| help.command == entry.command)
        else {
            continue;
        };
        rows.push(row_scene(shortcut, help.label, help.description, palette));
    }
    let local = shell_local_bindings()
        .iter()
        .find(|binding| binding.shortcut == "?")
        .map(|binding| row_scene("F1 / ?", binding.label, "", palette));
    rows.extend(local);
    let all = section_scene(t("common.all"), rows, palette);
    let gap = palette.rem_ratio(space_8() * 2.0);
    Box::new(bsn! {
        Node { width: percent(100), min_width: px(0.0), max_width: percent(100), flex_direction: FlexDirection::Column, row_gap: Val::Rem(gap) }
        Children [ @{ pages } -- @{ all } ]
    })
}

fn close_scene(palette: &UiPalette) -> Box<dyn Scene> {
    let label = t("common.close");
    let height = palette.control_height_rem;
    let radius = palette.control_radius_px;
    let fill = palette.content_bg;
    Box::new(bsn! {
        Node { min_height: rem(height), min_width: rem(height),
            padding: UiRect::horizontal(px(space_8())), flex_shrink: 0.0,
            border_radius: BorderRadius::all(Val::Px(radius)),
            justify_content: JustifyContent::Center, align_items: AlignItems::Center }
        BackgroundColor(fill)
        Button HelpCloseButton on(on_activate)
        Children [ Text(label) TextRole(Role::Caption) Pickable::IGNORE ]
    })
}

fn scene(palette: &UiPalette) -> Box<dyn Scene> {
    let body = body_scene(palette);
    let actions: Vec<Box<dyn Scene>> = vec![close_scene(palette)];
    let overlay = standalone_modal_scene(t("settings.keyboard"), body, actions, palette);
    Box::new(bsn! {
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100) }
        HelpOverlay
        Children [ @{ overlay } ]
    })
}

#[cfg(test)]
#[path = "../tests/headless/help_overlay.rs"]
mod tests;
