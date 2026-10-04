//! Permission-center scenes and actions over the shared immutable row fold.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::system::{NonSend, Query, ResMut};
use bevy::scene::{Scene, bsn, on};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, FlexDirection, Node, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::privilege_center::{
    PrivilegeAction, PrivilegeCenterInputs, PrivilegeRow,
};

use crate::app::FrontendTrack;
use crate::input::PendingEffects;
use crate::palette::{UiPalette, space_2, space_4, space_8};
use crate::window::{Role, TextRole};

#[derive(Component, Clone, Default)]
pub(crate) struct PrivilegeControl(pub(crate) Option<PrivilegeAction>);

fn authorize(
    activate: On<Activate>,
    controls: Query<&PrivilegeControl>,
    track: NonSend<FrontendTrack>,
    mut pending: ResMut<PendingEffects>,
) {
    let Ok(control) = controls.get(activate.entity) else {
        return;
    };
    let Some(action) = control.0.as_ref() else {
        return;
    };
    if PrivilegeCenterInputs::from_shell(&track.shell)
        .rows()
        .iter()
        .any(|row| row.action.as_ref() == Some(action))
    {
        pending.0.push(action.effect());
    }
}

pub(crate) fn privileges_section_scene(shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    let rows: Vec<Box<dyn Scene>> = PrivilegeCenterInputs::from_shell(shell)
        .rows()
        .into_iter()
        .map(|row| Box::new(privilege_row_scene(row, palette)) as Box<dyn Scene>)
        .collect();
    Box::new(bsn! {
        Node { width: percent(100), flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()), margin: UiRect::top(Val::Px(space_8())) }
        Children [
            Text(t("settings.privileges")) TextRole(Role::Heading) --
            Text(t("settings.privileges_hint")) TextRole(Role::Caption) --
            { rows }
        ]
    })
}

fn privilege_row_scene(row: PrivilegeRow, palette: &UiPalette) -> impl Scene + use<> {
    let line = format!("{}: {}", t(row.label_key), t(row.state.label_key()));
    let action: Vec<Box<dyn Scene>> = row
        .action
        .into_iter()
        .map(|action| {
            Box::new(bsn! {
                Node { padding: UiRect::all(Val::Px(space_4())) }
                Button
                PrivilegeControl({ Some(action) })
                on(authorize)
                Children [ Text(t("settings.privileges_authorize")) TextRole(Role::Body) ]
            }) as Box<dyn Scene>
        })
        .collect();
    bsn! {
        Node { width: percent(100), min_height: px(palette.control_height_px),
            flex_direction: FlexDirection::Row, align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()), padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_2())) }
        BackgroundColor({ palette.content_bg })
        Children [
            Node { flex_grow: 1.0, min_width: px(0.0) }
            Children [ Text(line) TextRole(Role::Body) ] --
            { action }
        ]
    }
}
