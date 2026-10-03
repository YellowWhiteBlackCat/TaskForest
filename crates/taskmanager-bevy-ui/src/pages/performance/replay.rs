//! History replay state and view composition for Bevy Performance page.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, ResMut};
use bevy::scene::{Scene, bsn, on};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, Display, FlexDirection, JustifyContent, Node,
    UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_core::core::history::HistoryWindow;

use crate::palette::{UiPalette, space_2, space_4, space_8};
use crate::window::{Role, TextRole};

/// Resource controlling whether the Performance page displays historical replay data.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PerformanceHistoryReplay {
    pub(crate) open: bool,
    pub(crate) playing: bool,
    pub(crate) window: HistoryWindow,
}

impl Default for PerformanceHistoryReplay {
    fn default() -> Self {
        Self {
            open: false,
            playing: false,
            window: HistoryWindow::OneHour,
        }
    }
}

/// Marker component on the mounted replay control strip.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceHistoryReplayStrip;

/// Component on a history window selector button.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PerformanceReplayWindowButton(pub(crate) HistoryWindow);

impl Default for PerformanceReplayWindowButton {
    fn default() -> Self {
        Self(HistoryWindow::OneHour)
    }
}

/// Component on the play/pause replay button.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceReplayPlayButton;

pub(crate) fn replay_window_button_observer(
    activate: On<Activate>,
    buttons: Query<&PerformanceReplayWindowButton>,
    mut replay: ResMut<PerformanceHistoryReplay>,
) {
    if let Ok(btn) = buttons.get(activate.entity) {
        replay.window = btn.0;
    }
}

pub(crate) fn replay_play_button_observer(
    activate: On<Activate>,
    buttons: Query<&PerformanceReplayPlayButton>,
    mut replay: ResMut<PerformanceHistoryReplay>,
) {
    if buttons.get(activate.entity).is_ok() {
        replay.playing = !replay.playing;
    }
}

pub(crate) fn history_replay_strip_scene(
    window: HistoryWindow,
    playing: bool,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let window_pills: Vec<Box<dyn Scene>> = [
        (HistoryWindow::OneHour, "1h"),
        (HistoryWindow::TwentyFourHours, "24h"),
        (HistoryWindow::SevenDays, "7d"),
    ]
    .into_iter()
    .map(|(w, label)| Box::new(window_pill_scene(w, label, w == window, palette)) as Box<dyn Scene>)
    .collect();

    let play_label = if playing { "Pause" } else { "Play" };

    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
            display: Display::None,
        }
        BackgroundColor({ palette.panel_fill })
        PerformanceHistoryReplayStrip
        Children [
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
            }
            Children [
                Text(t("perf.replay.title")) TextRole(Role::Heading) --
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(space_4()),
                }
                Children [
                    @{ window_pills }
                ]
            ] --
            Node {
                width: percent(100),
                height: px(palette.control_height_px),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
                padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_2())),
                border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
            }
            BackgroundColor({ palette.content_bg })
            Children [
                Node {
                    padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_2())),
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BackgroundColor({ palette.accent })
                Button
                PerformanceReplayPlayButton
                on(replay_play_button_observer)
                Children [
                    Text(play_label) TextRole(Role::Caption)
                ] --
                Text("Timeline: -60m ------------------------* 0s (Live Replay)") TextRole(Role::Mono)
            ]
        ]
    }
}

fn window_pill_scene(
    window: HistoryWindow,
    label: &'static str,
    active: bool,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let bg = if active {
        palette.accent
    } else {
        palette.content_bg
    };
    bsn! {
        Node {
            padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_2())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ bg })
        Button
        PerformanceReplayWindowButton({ window })
        on(replay_window_button_observer)
        Children [
            Text(label) TextRole(Role::Caption)
        ]
    }
}
