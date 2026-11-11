//! Startup status filter controls and filtered row projection.

use bevy::app::App;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, ResMut};
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, UiRect, Val,
    percent, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_shell::{ShellApp, StartupStatusFilter, matches_startup_status};

use super::StartupRowModel;
use crate::palette::{UiPalette, space_8, space_12};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupStatusFilterState {
    pub(crate) selected: StartupStatusFilter,
}

#[derive(Component, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct StartupFilterChoice(pub(crate) StartupStatusFilter);

pub(crate) fn startup_rows_for(
    shell: &ShellApp,
    filter: StartupStatusFilter,
) -> Vec<StartupRowModel> {
    filter_startup_rows(super::startup_rows(shell), filter)
}

pub(crate) fn filter_startup_rows(
    rows: Vec<StartupRowModel>,
    filter: StartupStatusFilter,
) -> Vec<StartupRowModel> {
    rows.into_iter()
        .filter(|row| matches_startup_status(row.enabled, filter))
        .collect()
}

fn capsule_scene(
    choice: StartupStatusFilter,
    selected: StartupStatusFilter,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let active = choice == selected;
    Box::new(bsn! {
        Node {
            height: rem(palette.control_height_rem),
            flex_shrink: 0.0,
            padding: UiRect::horizontal(Val::Px(space_12())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({
            if active { palette.nav_active_bg } else { palette.content_bg }
        })
        ControlVisual(ControlTone::Surface, active)
        Button
        on(on_startup_filter_activated)
        StartupFilterChoice(choice)
        Children [
             Text({ choice.label().to_owned() }) TextRole(Role::Caption)
             TextLayout { linebreak: LineBreak::NoWrap }
        ]
    })
}

pub(crate) fn scene(selected: StartupStatusFilter, palette: &UiPalette) -> Box<dyn Scene> {
    let capsules: Vec<Box<dyn Scene>> = StartupStatusFilter::ALL
        .into_iter()
        .map(|choice| capsule_scene(choice, selected, palette))
        .collect();
    Box::new(bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
            { capsules }
        ]
    })
}

fn on_startup_filter_activated(
    activate: On<Activate>,
    choices: Query<&StartupFilterChoice>,
    mut filter: ResMut<StartupStatusFilterState>,
    mut commands: Commands,
) {
    let Ok(choice) = choices.get(activate.event().entity) else {
        return;
    };
    if filter.selected == choice.0 {
        return;
    }
    filter.selected = choice.0;
    commands.trigger(super::paint::RepaintRequested);
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<StartupStatusFilterState>()
        .add_observer(on_startup_filter_activated);
}

#[cfg(test)]
#[path = "../../../tests/headless/pages/startup_status_filter.rs"]
mod tests;
