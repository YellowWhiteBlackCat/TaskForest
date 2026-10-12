//! Read-only text selection and clipboard export surface.
//!
//! Bevy's current surface selects a whole readout on pointer press and copies
//! it with Ctrl/Cmd+C. It does not implement range or word gestures and does
//! not render a selection highlight, so its capability is declared
//! `Divergent` from GPUI's `SelectableText` contract.
//!
//! # Architecture
//!
//! Governed by an explicit discrete state machine ([`TextSelectionState`]):
//! `None` when inactive, `Some(session)` when a readout is selected.

use crate::app::FrontendTrack;
use bevy::app::PostUpdate;
use bevy::clipboard::{Clipboard, ClipboardError};
use std::collections::VecDeque;
use std::ops::Range;

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::Changed;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{NonSendMut, Query, ResMut};
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
        let valid_range = if start <= end { start..end } else { end..start };
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

/// Queued UI output and the last completed write. Success is published only
/// after the toolkit's native clipboard accepts the text.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClipboardPort {
    pending: VecDeque<(String, String)>,
    contents: Option<String>,
}
impl ClipboardPort {
    pub(crate) fn request_text(&mut self, text: impl Into<String>, label: impl Into<String>) {
        self.pending.push_back((text.into(), label.into()));
    }
}

pub(crate) fn flush_clipboard(
    port: &mut ClipboardPort,
    shell: &mut ShellApp,
    mut write: impl FnMut(&str) -> Result<(), String>,
) {
    for _ in 0..16 {
        let Some((text, label)) = port.pending.pop_front() else {
            break;
        };
        match write(&text) {
            Ok(()) => {
                port.contents = Some(text);
                shell.report_notice(
                    FeedbackSource::Clipboard,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    format!("{label} {}", t("common.copied")),
                );
            }
            Err(error) => shell.report_notice(
                FeedbackSource::Clipboard,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                format!("{label}: {error}"),
            ),
        }
    }
}
fn flush_system_clipboard(
    mut port: ResMut<ClipboardPort>,
    mut track: Option<NonSendMut<FrontendTrack>>,
    mut clipboard: Option<ResMut<Clipboard>>,
) {
    let Some(track) = track.as_deref_mut() else {
        return;
    };
    flush_clipboard(&mut port, &mut track.shell, |text| {
        clipboard
            .as_deref_mut()
            .ok_or(ClipboardError::ClipboardNotSupported)
            .and_then(|clipboard| clipboard.set_text(text))
            .map_err(|error| error.to_string())
    });
}

/// Component attached to selectable read-only text readout nodes.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SelectableText(pub(crate) String, pub(crate) String);

impl SelectableText {
    #[cfg_attr(not(test), allow(dead_code))]
    #[must_use]
    pub(crate) fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self(id.into(), content.into())
    }
}

/// Copies the active text selection or fallback row summary to the clipboard port.
/// Returns whether a copy was performed and feedback recorded on the shell.
pub(crate) fn copy_selection_or_row(
    selection: &mut TextSelectionState,
    clipboard: &mut ClipboardPort,
    shell: &mut ShellApp,
) -> bool {
    if let Some(text) = selection.selected_text().map(ToOwned::to_owned)
        && !text.is_empty()
    {
        clipboard.request_text(text, "Selected Text");
        return true;
    }
    if let Some(summary) = shell.selected_row_summary() {
        clipboard.request_text(summary, "Selected Row");
        return true;
    }
    false
}

/// System tracking pointer activation on entities with [`SelectableText`].
pub(crate) fn text_selection_picking_system(
    mut state: ResMut<TextSelectionState>,
    query: Query<(Entity, &SelectableText, &PickingInteraction), Changed<PickingInteraction>>,
) {
    for (entity, selectable, interaction) in &query {
        if *interaction == PickingInteraction::Pressed {
            state.select_all(entity, selectable.0.clone(), selectable.1.clone());
        }
    }
}

/// Register readout selection and clipboard state.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.add_systems(PostUpdate, flush_system_clipboard);
    app.init_resource::<TextSelectionState>();
    app.init_resource::<ClipboardPort>();
    app.add_systems(bevy::app::Update, text_selection_picking_system);
}

#[cfg(test)]
#[path = "../tests/headless/text_selection.rs"]
mod tests;
