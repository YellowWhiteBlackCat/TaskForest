//! The accessibility seam: the shared `SemanticSnapshot` vocabulary kept in
//! step with the folded shell, and AccessKit nodes stamped onto the scene.
//!
//! Two artifacts, one authority (the shell):
//!
//! 1. **The snapshot** ([`SemanticSnapshotResource`]): the ui-contract
//!    semantic tree — bounded process rows, the status announcement, and the
//!    armed confirmation modal — rebuilt only when its inputs' revision key
//!    changes. This is the renderer-neutral surface other frontends publish
//!    and behavior tests consume.
//! 2. **The nodes**: `bevy_a11y::AccessibilityNode` components on table rows
//!    (see [`process_row_node`]), which Bevy's winit AccessKit bridge (the
//!    `accesskit_unix` feature on Linux) publishes to the platform tree. The
//!    windowed composition adds `AccessibilityPlugin`; headless compositions
//!    stay inert components.
//!
//! The revision key covers every fact the snapshot can express: process data
//! revision, the armed gate's frozen target key, the selected row, and the
//! feedback line. A quiet frame costs one key comparison — never polling
//! work, never a stale announcement.

use bevy::a11y::AccessibilityNode;
use bevy::app::{App, PostUpdate};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{NonSend, ResMut};
use taskmanager_shell::ShellApp;
use taskmanager_ui_contract::{
    ModalInput, ProcessRowInput, SemanticSnapshot, SemanticSnapshotBuilder, SemanticSnapshotError,
};

use crate::app::FrontendTrack;
use crate::confirmation::PendingConfirmationView;

/// Upper bound on rows in one semantic snapshot. Assistive technology reads
/// the same visible-window discipline the renderer uses; a five-figure
/// process list must never allocate an unbounded semantic tree.
pub(crate) const MAX_SNAPSHOT_ROWS: usize = 64;

/// The current snapshot plus the revision key it was built from.
#[derive(Resource, Default)]
pub(crate) struct SemanticSnapshotResource {
    pub(crate) snapshot: Option<SemanticSnapshot>,
    last_key: Option<SnapshotKey>,
}

/// Every input the snapshot can express, in one comparable tuple.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SnapshotKey {
    process_revision: u64,
    gate_key: Option<String>,
    selected: usize,
    feedback: String,
}

/// Build the shared snapshot from the folded shell. Pure; the system below is
/// its only applier and headless tests call it directly. Validation errors
/// surface typed — a broken tree must never be published.
pub(crate) fn build_snapshot(shell: &ShellApp) -> Result<SemanticSnapshot, SemanticSnapshotError> {
    let revision = shell.projection().process_revision;
    let mut builder = SemanticSnapshotBuilder::new(revision).application_name("TaskForestB");
    let visible = shell.visible_processes();
    if !visible.is_empty() {
        let selected = shell.selected.min(visible.len() - 1);
        let rows = visible
            .iter()
            .take(MAX_SNAPSHOT_ROWS)
            .enumerate()
            .map(|(index, process)| ProcessRowInput {
                id: taskmanager_shell::process_semantic_key(process),
                name: process.name.clone(),
                cpu_percent: process.current_cpu_percentage().map(f64::from),
                // A per-process share needs the denominator the shell does
                // not project on rows; the semantic value stays honestly
                // unavailable instead of a fabricated zero.
                memory_percent: None,
                selected: index == selected,
            });
        builder = builder.process_rows(rows);
    }
    let feedback = shell.feedback_text();
    if !feedback.is_empty() {
        builder = builder.status_announcement(feedback);
    }
    if let Some(view) = shell
        .pending_confirmation()
        .and_then(PendingConfirmationView::from_pending)
    {
        builder = builder.modal(ModalInput {
            id: view.target_key,
            name: view.title,
            description: Some(view.body),
        });
    }
    // The builder validates connectivity/roles; a broken tree must never be
    // published.
    builder.build()
}

/// The `PostUpdate` projection: rebuild only when the revision key moved.
fn sync_semantic_snapshot(
    track: NonSend<FrontendTrack>,
    mut state: ResMut<SemanticSnapshotResource>,
) {
    let shell = &track.shell;
    let key = SnapshotKey {
        process_revision: shell.projection().process_revision,
        gate_key: shell
            .pending_confirmation()
            .and_then(PendingConfirmationView::from_pending)
            .map(|view| view.target_key),
        selected: shell.selected,
        feedback: shell.feedback_text().to_owned(),
    };
    if state.last_key.as_ref() == Some(&key) {
        return;
    }
    state.snapshot = match build_snapshot(shell) {
        Ok(snapshot) => Some(snapshot),
        // A validation error is a contract bug in this module: keep the last
        // good snapshot (stale last-good beats a broken publication).
        Err(error) => {
            eprintln!("taskforest-b: semantic snapshot contract violated: {error}");
            state.snapshot.clone()
        }
    };
    state.last_key = Some(key);
}

/// The AccessKit node for one table row: row role, identity label. Inserted
/// with the row scene so the required-component default from `Button` never
/// reduces a data row to an unnamed button.
#[must_use]
pub(crate) fn process_row_node(name: &str, semantic_id: &str) -> AccessibilityNode {
    let mut node = accesskit::Node::new(accesskit::Role::Row);
    node.set_label(name);
    node.set_description(semantic_id);
    AccessibilityNode(node)
}

/// Register the semantic projection. Called by the window plugin.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<SemanticSnapshotResource>()
        .add_systems(PostUpdate, sync_semantic_snapshot);
}

#[cfg(test)]
#[path = "../tests/headless/semantic.rs"]
mod tests;
