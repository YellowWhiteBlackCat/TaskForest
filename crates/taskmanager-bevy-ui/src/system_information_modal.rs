//! Independent, frozen review of cached system-information facts.
//!
//! GPUI renders the shell-owned facts as named groups with bounded
//! label/value rows. Bevy now consumes the same groups instead of one long
//! copy blob: the copy action remains complete, while each visible value
//! stays inside its own clipped slot.

use crate::about_modal::{AboutCommand, action_scene};
use crate::palette::{UiPalette, space_4, space_8};
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
use bevy::ui::{
    AlignItems, BackgroundColor, BorderRadius, ComputedNode, FlexDirection, JustifyContent, Node,
    Overflow, ScrollPosition, UiRect, Val, percent, px, rem,
};
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::system_information::{
    SystemInformationGroup, SystemInformationRow, copy_all_text,
};

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
    let Some(WindowSurface::SystemInformation(facts)) = state.active() else {
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
    control_height_rem: f32,
    content_bg: bevy::color::Color,
) -> impl Scene + use<> {
    bsn! { Node { min_height: rem(control_height_rem), padding: UiRect::all(Val::Px(space_8())) } BackgroundColor({ content_bg }) Button SystemInformationControl({ command }) on(activate)
    Children [ Text(label) TextRole(Role::Body) Pickable::IGNORE ] }
}

pub(crate) fn surface_scene(
    facts: &[SystemInformationGroup],
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let groups = grouped_body_scenes(facts, palette);
    let body = bsn! {
        Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(space_8()) }
        Children [ { groups } ]
    };
    let actions: Vec<Box<dyn Scene>> = vec![
        Box::new(control_scene(
            t("system_about.copy_all"),
            SystemInformationCommand::Copy,
            palette.control_height_rem,
            palette.content_bg,
        )),
        Box::new(control_scene(
            t("common.close"),
            SystemInformationCommand::Close,
            palette.control_height_rem,
            palette.content_bg,
        )),
        Box::new(action_scene(t("about.title"), AboutCommand::Open, palette)),
    ];
    Box::new(modal_scene(
        WindowSurfaceKind::SystemInformation,
        t("system_about.title"),
        Box::new(body),
        actions,
        palette,
    ))
}

/// One bounded label/value slot. The label may shrink, the value keeps a
/// rem floor, and neither intrinsic text can push the other past the rail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SystemInformationRowSlots {
    pub(crate) label_grow: f32,
    pub(crate) label_shrink: f32,
    pub(crate) value_grow: f32,
    pub(crate) value_shrink: f32,
    pub(crate) value_min_width_rem: f32,
}

/// Pure slot geometry for the headless bounds contract.
#[must_use]
pub(crate) const fn system_information_row_slots() -> SystemInformationRowSlots {
    SystemInformationRowSlots {
        label_grow: 1.0,
        label_shrink: 1.0,
        value_grow: 1.0,
        value_shrink: 0.0,
        value_min_width_rem: 8.0,
    }
}

/// Marker on one mounted label/value row.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct SystemInformationRowMarker;

fn grouped_body_scenes(
    facts: &[SystemInformationGroup],
    palette: &UiPalette,
) -> Vec<Box<dyn Scene>> {
    if facts.is_empty() {
        return vec![Box::new(bsn! {
            Text(t("system_about.unavailable")) TextRole(Role::Body)
            TextLayout { linebreak: LineBreak::WordBoundary }
            Node { width: percent(100), min_width: px(0.0), max_width: percent(100) }
        })];
    }
    let mut groups = Vec::with_capacity(facts.len());
    let control_radius_px = palette.control_radius_px;
    let content_bg = palette.content_bg;
    for group in facts {
        let title = t(group.title_key);
        let mut rows = Vec::with_capacity(group.rows.len());
        for row in &group.rows {
            rows.push(information_row_scene(row));
        }
        groups.push(Box::new(bsn! {
            Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(space_8()) }
            Children [
                Text(title) TextRole(Role::Caption) --
                Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column, border_radius: BorderRadius::all(Val::Px(control_radius_px)), overflow: Overflow::clip() }
                BackgroundColor({ content_bg })
                Children [ { rows } ]
            ]
        }) as Box<dyn Scene>);
    }
    groups
}

fn information_row_scene(row: &SystemInformationRow) -> Box<dyn Scene> {
    let slots = system_information_row_slots();
    let label = t(row.label_key).to_owned();
    let value = row.value.clone();
    Box::new(bsn! {
        Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Row, align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, column_gap: Val::Px(space_8()), padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_4())) }
        SystemInformationRowMarker
        Children [
            Node { flex_grow: { slots.label_grow }, flex_shrink: { slots.label_shrink }, flex_basis: Val::Px(0.0), min_width: px(0.0), overflow: Overflow::clip_x() }
            Children [ Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE ] --
            Node { flex_grow: { slots.value_grow }, flex_shrink: { slots.value_shrink }, flex_basis: Val::Px(0.0), min_width: rem(slots.value_min_width_rem), overflow: Overflow::clip_x() }
            Children [ Text(value) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE ]
        ]
    })
}

#[cfg(test)]
#[path = "../tests/headless/system_information_modal.rs"]
mod tests;
