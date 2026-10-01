//! Process table column-visibility selection modal (Applications page).
//!
//! Delivers parity with GPUI's `columns_dropdown` and TUI's `C` column menu:
//! users can toggle visibility of any non-identity column in the process table,
//! resetting to defaults or dismissing via scrim click or Escape.

use std::collections::HashSet;

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    PositionType, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button, ScrollArea};
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::PROCESS_COLUMNS;

use crate::input::ShellInteractionApplied;
use crate::palette::{UiPalette, space_16, space_4, space_8};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};

/// Stores the user's hidden column IDs for the processes table.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessHiddenColumns(pub(crate) HashSet<String>);

/// Active session of the column-visibility picker modal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessColumnsSession {
    pub(crate) focused_index: usize,
}

/// Modal state resource for the process table columns selector:
/// `None` when closed, `Some(session)` when open.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessColumnsModalState {
    pub(crate) session: Option<ProcessColumnsSession>,
}

impl ProcessColumnsModalState {
    pub(crate) fn is_open(&self) -> bool {
        self.session.is_some()
    }

    pub(crate) fn open(&mut self) {
        self.session = Some(ProcessColumnsSession { focused_index: 0 });
    }

    pub(crate) fn close(&mut self) {
        self.session = None;
    }
}

/// Event fired when column visibility modal state or selection changes.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct ProcessColumnsModalChanged;

/// Marker for the columns modal root container.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessColumnsModalOverlay;

/// Marker for the full-screen scrim behind the modal card.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessColumnsModalScrim;

/// Component attached to a column item button identifying which column it toggles.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessColumnToggleId(pub(crate) &'static str);

/// Scene for the "Choose columns" button mounted in the processes toolbar.
pub(crate) fn choose_columns_button_scene(palette: &UiPalette) -> impl Scene + use<> {
    let label = t("proc.choose_columns");
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    bsn! {
        Node {
            height: px(height),
            padding: UiRect::axes(Val::Px(space_16()), Val::Px(space_4())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.nav_active_bg })
        Button
        ControlVisual(ControlTone::Surface, false)
        crate::tooltip::TooltipText({ label.to_string() })
        on(on_open_columns_modal_activated)
        Children [
            ( Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } ),
        ]
    }
}

/// Declarative scene for the column visibility selection card and scrim.
pub(crate) fn columns_modal_scene(
    hidden: &HashSet<String>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = t("proc.choose_columns");
    let subtitle = t("common.actions");
    let reset_label = t("common.reset");
    let close_label = t("common.cancel");

    let items: Vec<Box<dyn Scene>> = PROCESS_COLUMNS
        .iter()
        .filter(|spec| spec.hideable)
        .map(|spec| {
            let is_hidden = hidden.contains(spec.id);
            let check_mark = if is_hidden { "[ ]" } else { "[X]" };
            let label = format!("{check_mark} {}", spec.id);
            let bg = if is_hidden {
                palette.panel_fill
            } else {
                palette.selection_bg
            };
            let col_id = spec.id;
            Box::new(bsn! {
                Node {
                    width: percent(100),
                    height: px(32.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_4())),
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                    margin: UiRect::vertical(Val::Px(2.0)),
                }
                BackgroundColor({ bg })
                Button
                ProcessColumnToggleId({ col_id })
                on(on_column_toggle_activated)
                Children [
                    ( Text(label) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap } ),
                ]
            }) as Box<dyn Scene>
        })
        .collect();

    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            right: px(0.0),
            top: px(0.0),
            bottom: px(0.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        }
        ProcessColumnsModalOverlay
        Children [
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    right: px(0.0),
                    top: px(0.0),
                    bottom: px(0.0),
                }
                BackgroundColor({ palette.scrim })
                Button
                ProcessColumnsModalScrim
                on(on_columns_scrim_dismiss_activated)
            ),
            (
                Node {
                    width: px(380.0),
                    max_height: px(520.0),
                    flex_direction: FlexDirection::Column,
                    padding: Val::Px(space_16()),
                    border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
                    row_gap: Val::Px(space_8()),
                }
                BackgroundColor({ palette.panel_fill })
                Children [
                    ( Text(title) TextRole(Role::Heading) ),
                    ( Text(subtitle) TextRole(Role::Caption) ),
                    (
                        Node {
                            width: percent(100),
                            max_height: px(340.0),
                            flex_direction: FlexDirection::Column,
                            overflow: Overflow::clip_y(),
                        }
                        ScrollArea
                        Children [
                            { items }
                        ]
                    ),
                    (
                        Node {
                            width: percent(100),
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::FlexEnd,
                            column_gap: Val::Px(space_8()),
                        }
                        Children [
                            (
                                Node {
                                    height: px(palette.control_height_px),
                                    padding: UiRect::axes(Val::Px(space_16()), Val::Px(space_4())),
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::Center,
                                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                                }
                                BackgroundColor({ palette.content_bg })
                                Button
                                on(on_columns_reset_activated)
                                Children [
                                    ( Text(reset_label) TextRole(Role::Caption) ),
                                ]
                            ),
                            (
                                Node {
                                    height: px(palette.control_height_px),
                                    padding: UiRect::axes(Val::Px(space_16()), Val::Px(space_4())),
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::Center,
                                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                                }
                                BackgroundColor({ palette.nav_active_bg })
                                Button
                                on(on_columns_close_activated)
                                Children [
                                    ( Text(close_label) TextRole(Role::Caption) ),
                                ]
                            ),
                        ]
                    ),
                ]
            ),
        ]
    }
}

pub(crate) fn on_open_columns_modal_activated(
    _activate: On<Activate>,
    mut state: Option<ResMut<ProcessColumnsModalState>>,
    mut commands: Commands,
) {
    if let Some(ref mut state) = state {
        state.open();
        commands.trigger(ProcessColumnsModalChanged);
    }
}

pub(crate) fn on_columns_scrim_dismiss_activated(
    _activate: On<Activate>,
    mut state: Option<ResMut<ProcessColumnsModalState>>,
    mut commands: Commands,
) {
    if let Some(ref mut state) = state {
        state.close();
        commands.trigger(ProcessColumnsModalChanged);
    }
}

pub(crate) fn on_columns_close_activated(
    _activate: On<Activate>,
    mut state: Option<ResMut<ProcessColumnsModalState>>,
    mut commands: Commands,
) {
    if let Some(ref mut state) = state {
        state.close();
        commands.trigger(ProcessColumnsModalChanged);
    }
}

pub(crate) fn on_columns_reset_activated(
    _activate: On<Activate>,
    mut hidden: Option<ResMut<ProcessHiddenColumns>>,
    mut commands: Commands,
) {
    if let Some(ref mut hidden) = hidden {
        hidden.0.clear();
        commands.trigger(ProcessColumnsModalChanged);
        commands.trigger(ShellInteractionApplied);
    }
}

pub(crate) fn on_column_toggle_activated(
    activate: On<Activate>,
    items: Query<&ProcessColumnToggleId>,
    mut hidden: Option<ResMut<ProcessHiddenColumns>>,
    mut commands: Commands,
) {
    let Ok(target) = items
        .get(activate.entity)
        .or_else(|_| items.get(activate.event().entity))
    else {
        return;
    };
    if let Some(ref mut hidden) = hidden {
        let col_id = target.0.to_string();
        if !hidden.0.insert(col_id.clone()) {
            hidden.0.remove(&col_id);
        }
        commands.trigger(ProcessColumnsModalChanged);
        commands.trigger(ShellInteractionApplied);
    }
}

fn on_modal_state_changed(
    _changed: On<ProcessColumnsModalChanged>,
    state: Option<Res<ProcessColumnsModalState>>,
    hidden: Option<Res<ProcessHiddenColumns>>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<AppShellRoot>>,
    overlays: Query<Entity, With<ProcessColumnsModalOverlay>>,
    mut commands: Commands,
) {
    for entity in &overlays {
        commands.entity(entity).despawn();
    }
    let Some(state) = state else {
        return;
    };
    if !state.is_open() {
        return;
    }
    let Some(palette) = palette else {
        return;
    };
    let Ok(root) = roots.single() else {
        return;
    };
    let empty_hidden = HashSet::new();
    let hidden_set = hidden.as_ref().map_or(&empty_hidden, |h| &h.0);
    let scene = columns_modal_scene(hidden_set, &palette.inner);
    let overlay = commands.spawn_scene(scene).id();
    commands.entity(root).add_one_related::<ChildOf>(overlay);
}

/// Register the columns modal state and observer on the app.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<ProcessHiddenColumns>();
    app.init_resource::<ProcessColumnsModalState>();
    app.add_observer(on_modal_state_changed);
}

#[cfg(test)]
#[path = "../../../tests/headless/pages/process_columns.rs"]
mod tests;
