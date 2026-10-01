//! Read-only text selection and clipboard export surface.
//!
//! Delivers true parity with GPUI's `taskmanager-ui::SelectableText` and Iced's
//! `components::SelectableText`: detail readouts and scalar facts can attach a
//! [`SelectableText`] component so clicking selects the readout and Ctrl+C copies
//! the selected string or selected row summary to the clipboard.
//!
//! # Architecture
//!
//! Governed by an explicit discrete state machine ([`TextSelectionState`]):
//! `None` when inactive, `Some(session)` when text is selected. State
//! transitions emit [`TextSelectionChanged`] so the observer updates highlight
//! components without frame-by-frame polling.

use std::ops::Range;

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Changed, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::picking::hover::PickingInteraction;
use taskmanager_application::i18n::t;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

/// Active session of read-only text selection on an interactive element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextSelectionSession {
    pub(crate) target: Entity,
    pub(crate) id: String,
    pub(crate) content: String,
    pub(crate) range: Range<usize>,
}

/// Explicit discrete state machine resource for text selection:
/// `None` when inactive, `Some(session)` when text is selected.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TextSelectionState {
    pub(crate) session: Option<TextSelectionSession>,
}

impl TextSelectionState {
    #[must_use]
    pub(crate) fn is_active(&self) -> bool {
        self.session.is_some()
    }

    #[must_use]
    pub(crate) fn selected_text(&self) -> Option<&str> {
        let session = self.session.as_ref()?;
        session.content.get(session.range.clone())
    }

    pub(crate) fn select_range(
        &mut self,
        target: Entity,
        id: String,
        content: String,
        range: Range<usize>,
    ) {
        let start = range.start.min(content.len());
        let end = range.end.min(content.len());
        let valid_range = if start <= end {
            start..end
        } else {
            end..start
        };
        self.session = Some(TextSelectionSession {
            target,
            id,
            content,
            range: valid_range,
        });
    }

    pub(crate) fn select_all(&mut self, target: Entity, id: String, content: String) {
        let len = content.len();
        self.select_range(target, id, content, 0..len);
    }

    pub(crate) fn clear(&mut self) {
        self.session = None;
    }
}

/// Cross-platform clipboard port resource for Bevy UI.
/// Holds the last copied string and acts as the interface to the OS clipboard.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClipboardPort {
    pub(crate) contents: Option<String>,
}

impl ClipboardPort {
    pub(crate) fn set_text(&mut self, text: impl Into<String>) {
        self.contents = Some(text.into());
    }

    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn get_text(&self) -> Option<&str> {
        self.contents.as_deref()
    }

    #[allow(dead_code)]
    pub(crate) fn clear(&mut self) {
        self.contents = None;
    }
}

/// Event triggered when text selection session begins, changes, or clears.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct TextSelectionChanged;

/// Component attached to selectable read-only text readout nodes.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SelectableText(pub(crate) String, pub(crate) String);

impl SelectableText {
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self(id.into(), content.into())
    }

    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn id(&self) -> &str {
        &self.0
    }

    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn content(&self) -> &str {
        &self.1
    }
}

/// Marker component on visual nodes that represent an active selection highlight.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct TextSelectionHighlight;

/// Copies the active text selection or fallback row summary to the clipboard port.
/// Returns whether a copy was performed and feedback recorded on the shell.
pub(crate) fn copy_selection_or_row(
    selection: &mut TextSelectionState,
    clipboard: &mut ClipboardPort,
    shell: &mut ShellApp,
) -> bool {
    if let Some(text) = selection.selected_text().map(ToOwned::to_owned) {
        if !text.is_empty() {
            clipboard.set_text(text);
            shell.report_notice(
                FeedbackSource::Clipboard,
                FeedbackSeverity::Success,
                FeedbackLifecycle::SHORT,
                format!("Selected Text {}", t("common.copied")),
            );
            return true;
        }
    }
    if let Some(summary) = shell.selected_row_summary() {
        clipboard.set_text(summary);
        shell.report_notice(
            FeedbackSource::Clipboard,
            FeedbackSeverity::Success,
            FeedbackLifecycle::SHORT,
            format!("Selected Row {}", t("common.copied")),
        );
        return true;
    }
    false
}

/// System tracking pointer activation on entities with [`SelectableText`].
pub(crate) fn text_selection_picking_system(
    mut state: ResMut<TextSelectionState>,
    query: Query<(Entity, &SelectableText, &PickingInteraction), Changed<PickingInteraction>>,
    mut commands: Commands,
) {
    for (entity, selectable, interaction) in &query {
        if *interaction == PickingInteraction::Pressed {
            state.select_all(entity, selectable.0.clone(), selectable.1.clone());
            commands.trigger(TextSelectionChanged);
        }
    }
}

fn on_text_selection_changed(
    _changed: On<TextSelectionChanged>,
    state: Option<Res<TextSelectionState>>,
    mut query: Query<(Entity, Option<&TextSelectionHighlight>), With<SelectableText>>,
    mut commands: Commands,
) {
    let Some(state) = state else {
        return;
    };
    let active_target = state.session.as_ref().map(|s| s.target);
    for (entity, highlight) in &mut query {
        let is_active = Some(entity) == active_target;
        match (is_active, highlight.is_some()) {
            (true, false) => {
                commands.entity(entity).insert(TextSelectionHighlight);
            }
            (false, true) => {
                commands.entity(entity).remove::<TextSelectionHighlight>();
            }
            _ => {}
        }
    }
}

/// Register text selection state, systems, and observer.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<TextSelectionState>();
    app.init_resource::<ClipboardPort>();
    app.add_observer(on_text_selection_changed);
    app.add_systems(bevy::app::Update, text_selection_picking_system);
}

#[cfg(test)]
#[path = "../tests/headless/text_selection.rs"]
mod tests;
