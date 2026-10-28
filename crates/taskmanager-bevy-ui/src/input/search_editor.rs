//! Native search-field editing and its shell bridge.
//!
//! The Page search boxes are native `TextInput` widgets when the window
//! composition installs Bevy's `TextInputPlugin`; the shell stays the single
//! search authority. This module owns the one attach observer, the bidirectional
//! text sync, the focus handoff, and Escape. A headless composition without
//! the text-input plugin keeps the shell-backed editor in `dispatch`, so the
//! same shell contract is exercised without a window.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Changed, Has, Or, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSend, NonSendMut, Query, Res, ResMut};
use bevy::input::ButtonState;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::input_focus::{FocusCause, FocusGained, FocusedInput, InputFocus};
use bevy::picking::events::PointerPress;
use bevy::prelude::Visibility;
use bevy::text::{EditableText, TextEdit};
use bevy::ui_widgets::TextInputPlugin;
use taskmanager_application::AppPage;
use taskmanager_shell::SearchSlot;

use crate::app::FrontendTrack;
use crate::input::ShellInteractionApplied;
use crate::pages::processes::ProcessSearchInput;
use crate::pages::services::ServicesSearchInput;

use super::commit_query_to_shell;

/// Editor buffer seed for one search box. The attach observer installs the
/// `EditableText` component once this marker lands.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SearchEditor(pub(crate) String);

/// The service-search placeholder and whether it starts visible. The attach
/// observer installs the matching `Visibility` (BSN cannot build a dynamic
/// enum value as a component item).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ServicesSearchPlaceholder(pub(crate) bool);

/// Whether the composition installed Bevy's native text-input plugin. A
/// headless test composition without it keeps the shell-backed fallback.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeTextEditing(pub(crate) bool);

type ChangedSearchEditors<'w, 's> = Query<
    'w,
    's,
    (
        &'static EditableText,
        Has<ProcessSearchInput>,
        Has<ServicesSearchInput>,
    ),
    (
        Or<(With<ProcessSearchInput>, With<ServicesSearchInput>)>,
        Changed<EditableText>,
    ),
>;

type Editors<'w, 's> = Query<
    'w,
    's,
    (Has<ProcessSearchInput>, Has<ServicesSearchInput>),
    Or<(
        With<crate::pages::processes::ProcessSearchInput>,
        With<crate::pages::services::ServicesSearchInput>,
    )>,
>;

/// The page-owned slot one editor writes.
fn editor_slot(process: bool, services: bool) -> Option<SearchSlot> {
    if services {
        Some(SearchSlot::Services)
    } else if process {
        Some(SearchSlot::Applications)
    } else {
        None
    }
}

fn attach_editor(
    add: On<Add<SearchEditor>>,
    editors: Query<&SearchEditor>,
    mut commands: Commands,
) {
    let Ok(editor) = editors.get(add.event().entity) else {
        return;
    };
    commands
        .entity(add.event().entity)
        .insert(EditableText::new(&editor.0));
}

fn attach_placeholder_visibility(
    add: On<Add<ServicesSearchPlaceholder>>,
    placeholders: Query<&ServicesSearchPlaceholder>,
    mut commands: Commands,
) {
    let Ok(placeholder) = placeholders.get(add.event().entity) else {
        return;
    };
    commands
        .entity(add.event().entity)
        .insert(placeholder_visibility(if placeholder.0 {
            ""
        } else {
            "query"
        }));
}

/// Give a clicked search box the keyboard. The page opens search through the
/// `FocusGained` observer below, so pointer and Ctrl+F share one open path.
pub(crate) fn focus_search_on_press(
    press: On<PointerPress>,
    editors: Editors,
    mut focus: ResMut<InputFocus>,
) {
    if editors.contains(press.entity) {
        focus.set(press.entity, FocusCause::Pressed);
    }
}

pub(crate) fn open_search_on_editor_focus(
    gained: On<FocusGained>,
    editors: Editors,
    mut track: Option<NonSendMut<FrontendTrack>>,
    mut commands: Commands,
) {
    let Some(track) = track.as_deref_mut() else {
        return;
    };
    let Ok((process, services)) = editors.get(gained.entity) else {
        return;
    };
    let Some(slot) = editor_slot(process, services) else {
        return;
    };
    if !track.shell.search_active() || track.shell.search_slot() != slot {
        track.shell.open_search_for(slot);
        commands.trigger(ShellInteractionApplied);
    }
}

/// Keep the toolkit focus on the page's search box while search owns the
/// keyboard, and release it when search closes. Only active when the native
/// editor is the real input path.
pub(crate) fn focus_active_search(
    track: Option<NonSend<FrontendTrack>>,
    editors: Query<(Entity, Has<ProcessSearchInput>, Has<ServicesSearchInput>)>,
    mut focus: ResMut<InputFocus>,
    native: Option<Res<NativeTextEditing>>,
) {
    if !native.is_some_and(|native| native.0) {
        return;
    }
    let Some(track) = track else {
        return;
    };
    let page = track.shell.page();
    let target = editors
        .iter()
        .find(|(_, process, services)| match page {
            AppPage::Applications => *process,
            AppPage::Services => *services,
            _ => false,
        })
        .map(|(entity, _, _)| entity);
    if track.shell.search_active() {
        if let Some(target) = target
            && focus.get() != Some(target)
        {
            focus.set(target, FocusCause::Navigated);
        }
    } else if let Some(focused) = focus.get()
        && editors.iter().any(|(entity, _, _)| entity == focused)
    {
        focus.clear();
    }
}

/// Escape clears the query, then closes the field. Other keys belong to the
/// native widget, so this observer leaves them untouched.
pub(crate) fn on_search_editor_key(
    mut key: On<FocusedInput<KeyboardInput>>,
    editors: Editors,
    mut track: Option<NonSendMut<FrontendTrack>>,
    native: Option<Res<NativeTextEditing>>,
    mut commands: Commands,
) {
    if !native.is_some_and(|native| native.0)
        || key.input.state != ButtonState::Pressed
        || key.input.key_code != KeyCode::Escape
        || !editors.contains(key.focused_entity)
    {
        return;
    }
    let Some(track) = track.as_deref_mut() else {
        return;
    };
    let Ok((process, services)) = editors.get(key.focused_entity) else {
        return;
    };
    let Some(slot) = editor_slot(process, services) else {
        return;
    };
    let empty = match slot {
        SearchSlot::Applications => track.shell.query.is_empty(),
        SearchSlot::Services => track.shell.services_query.is_empty(),
    };
    if empty {
        track.shell.close_search();
    } else {
        match slot {
            SearchSlot::Applications => commit_query_to_shell(&mut track.shell, ""),
            SearchSlot::Services => track.shell.set_services_query(""),
        }
    }
    key.propagate(false);
    commands.trigger(ShellInteractionApplied);
}

/// Commit native edits to the shell. Runs after the toolkit applied them, and
/// only when the text really differs — the shell→widget sync below marks the
/// component without creating a loop.
pub(crate) fn sync_search_edits(
    mut track: Option<NonSendMut<FrontendTrack>>,
    editors: ChangedSearchEditors,
    mut commands: Commands,
) {
    let Some(track) = track.as_deref_mut() else {
        return;
    };
    for (editor, process, services) in &editors {
        let text = editor.value().to_string();
        if services {
            if text != track.shell.services_query {
                track.shell.set_services_query(&text);
                commands.trigger(ShellInteractionApplied);
            }
        } else if process && text != track.shell.query {
            commit_query_to_shell(&mut track.shell, &text);
            commands.trigger(ShellInteractionApplied);
        }
    }
}

/// Write the shell query into the native buffer when the shell changed it
/// (Escape, another frontend path, a page remount).
pub(crate) fn sync_search_editor(editor: &mut EditableText, query: &str) {
    if editor.value() != query {
        editor.editor_mut().set_text(query);
        editor.queue_edit(TextEdit::TextEnd(false));
    }
}

/// The placeholder is visible exactly while the query is empty.
pub(crate) fn placeholder_visibility(query: &str) -> Visibility {
    if query.is_empty() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    }
}

pub(super) fn register(app: &mut bevy::app::App) {
    app.insert_resource(NativeTextEditing(app.is_plugin_added::<TextInputPlugin>()))
        .add_observer(attach_editor)
        .add_observer(attach_placeholder_visibility)
        .add_observer(focus_search_on_press)
        .add_observer(open_search_on_editor_focus)
        .add_observer(on_search_editor_key)
        .add_systems(
            bevy::app::PostUpdate,
            sync_search_edits.after(bevy::text::EditableTextSystems),
        );
}

#[cfg(test)]
#[path = "../../tests/headless/search_editor.rs"]
mod tests;
