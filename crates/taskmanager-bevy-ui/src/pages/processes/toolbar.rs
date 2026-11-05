//! Action bar, search, and segmented status filter row for the Processes page.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::system::{Commands, NonSendMut, Query};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextColor, TextFont, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, FlexWrap, JustifyContent, Node,
    UiRect, Val, percent, px, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button, TextInput};
use taskforest_core::core::process::ProcessBatchAction;
use taskmanager_application::AppAction;
use taskmanager_application::i18n::t;
use taskmanager_shell::ProcessStatusFilter;

use super::ProcessOptionalChrome;
use super::ProcessSearchInput;
use super::columns_modal::choose_columns_button_scene;
use super::on_search_input_activated;
use crate::app::{FrontendTrack, PageContext};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, space_4, space_8, space_24};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};

#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessFilterButton(pub(crate) ProcessStatusFilter);

#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessEndTaskButton;

#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessKillTaskButton;

fn on_filter_button_activated(
    event: On<Activate>,
    filters: Query<&ProcessFilterButton>,
    track: Option<NonSendMut<FrontendTrack>>,
    mut commands: Commands,
) {
    if let Ok(filter) = filters.get(event.entity)
        && let Some(mut track) = track
    {
        track.shell.set_process_status_filter(filter.0);
        commands.trigger(ShellProjectionFolded);
    }
}

fn on_end_task_activated(
    _event: On<Activate>,
    track: Option<NonSendMut<FrontendTrack>>,
    mut commands: Commands,
) {
    if let Some(mut track) = track {
        if track.shell.selected_rows.len() > 1 {
            let _ = track.shell.request_process_batch(ProcessBatchAction::End);
        } else {
            let _ = track.shell.apply_action(AppAction::RequestEndTask);
        }
        commands.trigger(ShellProjectionFolded);
    }
}

fn on_kill_task_activated(
    _event: On<Activate>,
    track: Option<NonSendMut<FrontendTrack>>,
    mut commands: Commands,
) {
    if let Some(mut track) = track {
        let _ = track.shell.request_process_batch(ProcessBatchAction::Kill);
        commands.trigger(ShellProjectionFolded);
    }
}

fn search_input_scene(palette: &UiPalette, query: &str) -> impl Scene + use<> {
    let text = query.to_owned();
    let width = space_24() * 12.0;
    let radius = palette.control_radius_px;
    // The seed size is the palette's body rung; the role observer stamps the
    // final metrics. No scene spells a type number (ADR-063).
    let body_size = palette.body.font_size;
    bsn! {
        Node {
            width: px(width),
            height: rem(palette.control_height_rem),
            align_items: AlignItems::Center,
            padding: Val::Px(space_8()),
            flex_shrink: 0.0,
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.panel_fill })
        ProcessSearchInput
        crate::input::SearchEditor({ text })
        TextInput
        Text({ query.to_owned() })
        TextFont { font_size: body_size }
        TextColor({ palette.body_color })
        TextRole(Role::Body)
        TextLayout { linebreak: LineBreak::NoWrap }
        Button
        on(on_search_input_activated)
    }
}

pub(super) fn toolbar_scene(context: &PageContext<'_>) -> impl Scene + use<> {
    let palette = context.palette;
    let search = search_input_scene(palette, &context.shell.query);
    let choose_columns_btn = choose_columns_button_scene(palette);
    let saved_views = crate::saved_views::button(
        t("saved_views.title").into(),
        crate::saved_views::SavedViewAction::Open,
        palette,
    );
    let has_selection = context.shell.selected_process_identity().is_some();
    let end_task_btn = bsn! {
        Node {
            height: rem(palette.control_height_rem),
            padding: UiRect::horizontal(px(space_8())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_shrink: 0.0,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ if has_selection { palette.nav_active_bg } else { palette.content_bg } })
        ControlVisual(ControlTone::Surface, has_selection)
        Button
        ProcessEndTaskButton
        on(on_end_task_activated)
        Children [
            Text({ t("proc.end_task").to_owned() }) TextRole(Role::Caption) Pickable::IGNORE
        ]
    };
    let kill_task_btn = bsn! {
        Node {
            height: rem(palette.control_height_rem),
            padding: UiRect::horizontal(px(space_8())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_shrink: 0.0,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ if has_selection { palette.nav_active_bg } else { palette.content_bg } })
        ControlVisual(ControlTone::Surface, has_selection)
        Button
        ProcessKillTaskButton
        on(on_kill_task_activated)
        Children [
            Text({ t("proc.kill").to_owned() }) TextRole(Role::Caption) Pickable::IGNORE
        ]
    };
    let active_filter = context.shell.process_status_filter;
    let filter_pills: Vec<Box<dyn Scene>> = ProcessStatusFilter::ALL
        .into_iter()
        .map(|filter| {
            let is_active = filter == active_filter;
            let bg = if is_active {
                palette.nav_active_bg
            } else {
                palette.content_bg
            };
            Box::new(bsn! {
                Node {
                    height: rem(palette.control_height_rem),
                    padding: UiRect::horizontal(px(space_8())),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    flex_shrink: 0.0,
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BackgroundColor({ bg })
                ControlVisual(ControlTone::Surface, is_active)
                Button
                ProcessFilterButton(filter)
                on(on_filter_button_activated)
                Children [
                    Text({ filter.label().to_owned() }) TextRole(Role::Caption) Pickable::IGNORE
                ]
            }) as Box<dyn Scene>
        })
        .collect();
    let filter_strip = bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
            flex_shrink: 0.0,
        }
        ProcessOptionalChrome
        Children [ { filter_pills } ]
    };
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            flex_wrap: FlexWrap::Wrap,
            flex_shrink: 0.0,
            column_gap: Val::Px(space_8()),
            row_gap: Val::Px(space_4()),
        }
        Children [
             @{ search } --
             @{ end_task_btn } --
             @{ kill_task_btn } --
             @{ choose_columns_btn } --
             @{ saved_views } --
             @{ crate::pages::process_tree::open_button(palette) } --
             @{ filter_strip }
        ]
    }
}
