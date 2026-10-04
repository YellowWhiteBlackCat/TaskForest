//! Shared history action group for both native review surfaces.

use crate::pages::history::control::HistoryCommand;
use crate::palette::{UiPalette, space_8};
use crate::window::{Role, TextRole};
use bevy::ecs::{
    component::Component,
    hierarchy::Children,
    observer::On,
    system::{Commands, Query},
};
use bevy::scene::{Scene, bsn, on};
use bevy::ui::prelude::{BackgroundColor, FlexDirection, FlexWrap, Node, UiRect, percent, px};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_core::core::history::HistoryWindow;

#[derive(Component, Clone, Default)]
pub(crate) struct HistoryControl(pub(crate) Option<HistoryCommand>);
#[derive(Component, Clone, Default)]
pub(crate) struct HistoryActionGroup;

fn activate(event: On<Activate>, controls: Query<&HistoryControl>, mut commands: Commands) {
    if let Ok(control) = controls.get(event.entity)
        && let Some(command) = control.0
    {
        commands.trigger(command);
    }
}

pub(crate) fn action_scene(
    label: String,
    command: HistoryCommand,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! { Node { padding: UiRect::all(px(space_8())), flex_shrink: 0.0 } BackgroundColor({ palette.panel_fill }) Button HistoryControl({ Some(command) }) on(activate) Children [ Text(label) TextRole(Role::Body) ] }
}

pub(crate) fn toolbar_scene(
    window: HistoryWindow,
    return_to_live: bool,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let mut actions = HistoryWindow::ALL
        .into_iter()
        .map(|value| {
            let key = match value {
                HistoryWindow::OneHour => "perf.replay.window.1h",
                HistoryWindow::TwentyFourHours => "perf.replay.window.24h",
                HistoryWindow::SevenDays => "perf.replay.window.7d",
            };
            let label = if window == value {
                format!("[{}]", t(key))
            } else {
                t(key).to_owned()
            };
            Box::new(action_scene(label, HistoryCommand::Window(value), palette)) as Box<dyn Scene>
        })
        .collect::<Vec<_>>();
    actions.push(Box::new(action_scene(
        t("perf.replay.refresh").to_owned(),
        HistoryCommand::Refresh,
        palette,
    )));
    if return_to_live {
        actions.push(Box::new(action_scene(
            t("perf.replay.back_to_live").to_owned(),
            HistoryCommand::ClosePerformance,
            palette,
        )));
    }
    bsn! { Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(space_8()), row_gap: px(space_8()) } HistoryActionGroup Children [ { actions } ] }
}
