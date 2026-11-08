//! The accessibility seam: the shared `SemanticSnapshot` vocabulary kept in
//! step with the folded shell, and AccessKit nodes stamped onto the scene.
//!
//! Two artifacts, one authority (the shell):
//!
//! 1. **The snapshot** (`SemanticSnapshotResource`): the ui-contract
//!    semantic tree — bounded process rows, the status announcement, and the
//!    armed confirmation modal — rebuilt only when its inputs' revision key
//!    changes. This is the renderer-neutral surface other frontends publish
//!    and behavior tests consume.
//! 2. **The nodes**: `bevy_a11y::AccessibilityNode` components on table rows
//!    (see `process_row_node`), which Bevy's winit AccessKit bridge (the
//!    `accesskit_unix` feature on Linux) publishes to the platform tree. The
//!    windowed composition adds `AccessibilityPlugin`; headless compositions
//!    stay inert components.
//!
//! The revision key covers every fact the snapshot can express: process data
//! revision, the armed gate's frozen target key, the selected row, and the
//! feedback line. A quiet frame costs one key comparison — never polling
//! work, never a stale announcement.

use crate::first_run_modal::SetupState;
use crate::window_surface::{WindowSurface, WindowSurfaceState};
use bevy::app::{App, PostUpdate};
use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{NonSend, Res, ResMut};
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::first_run::FirstRunUiState;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::diagnostics::{
    diagnostic_failure_message, diagnostic_redaction_summary,
};
use taskmanager_shell::presentation::system_information::copy_all_text;
use taskmanager_shell::{ShellApp, base_semantic_snapshot_builder};
use taskmanager_ui_contract::{ModalInput, SemanticSnapshot, SemanticSnapshotError};

use crate::app::FrontendTrack;
use crate::confirmation::PendingConfirmationView;

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
pub(crate) fn build_snapshot(
    shell: &ShellApp,
    surface: Option<&WindowSurface>,
    setup: Option<&FirstRunUiState>,
) -> Result<SemanticSnapshot, SemanticSnapshotError> {
    let mut builder = base_semantic_snapshot_builder(shell, "TaskForestB");

    if let Some(WindowSurface::Diagnostic(state)) = surface {
        let description = match state {
            DiagnosticBundleUiState::Preview(plan) => diagnostic_redaction_summary(plan.preview()),
            DiagnosticBundleUiState::Writing(_) => t("diagnostics.writing").to_owned(),
            DiagnosticBundleUiState::Complete(path) => {
                t("diagnostics.complete").replace("{path}", &path.display().to_string())
            }
            DiagnosticBundleUiState::Failed(error) => diagnostic_failure_message(error),
        };
        builder = builder.modal(ModalInput {
            id: "diagnostic-bundle".into(),
            name: t("diagnostics.title").into(),
            description: Some(description),
        });
    } else if let Some(surface) = surface {
        let (id, name, description) = match surface {
            WindowSurface::ProcessTree => {
                ("process-tree", "Process tree", "Process hierarchy".into())
            }
            WindowSurface::EventCenter => {
                ("event-center", t("events.title"), t("events.empty").into())
            }
            WindowSurface::SavedViews => (
                "saved-views",
                t("saved_views.title"),
                t("saved_views.help").to_owned(),
            ),
            WindowSurface::SidebarDevices => (
                "sidebar-devices",
                "Edit devices",
                "Device visibility and order".to_owned(),
            ),
            WindowSurface::About => (
                "about",
                t("about.title"),
                env!("CARGO_PKG_VERSION").to_owned(),
            ),
            WindowSurface::SystemInformation(facts) => (
                "system-information",
                t("system_about.title"),
                copy_all_text(facts),
            ),
            WindowSurface::FirstRun => (
                "first-run",
                t("first_run.title"),
                setup.and_then(|state| state.info.as_ref()).map_or_else(
                    || t("first_run.discovering").to_owned(),
                    |info| info.run_command.clone(),
                ),
            ),
            WindowSurface::Diagnostic(_) => {
                ("diagnostic-bundle", t("diagnostics.title"), String::new())
            }
        };
        builder = builder.modal(ModalInput {
            id: id.into(),
            name: name.into(),
            description: Some(description),
        });
    } else if let Some(view) = shell
        .pending_confirmation()
        .and_then(PendingConfirmationView::from_pending)
    {
        builder = builder.modal(ModalInput {
            id: view.target_key,
            name: view.title,
            description: Some(view.body),
        });
    }
    builder.build()
}

/// The `PostUpdate` projection: rebuild only when the revision key moved.
fn sync_semantic_snapshot(
    track: NonSend<FrontendTrack>,
    mut state: ResMut<SemanticSnapshotResource>,
    surface: Option<Res<WindowSurfaceState>>,
    setup: Option<Res<SetupState>>,
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
    if state.last_key.as_ref() == Some(&key)
        && !surface.as_ref().is_some_and(|surface| surface.is_changed())
        && !setup.as_ref().is_some_and(|setup| setup.is_changed())
    {
        return;
    }
    state.snapshot = match build_snapshot(
        shell,
        surface.as_ref().and_then(|surface| surface.active()),
        setup.as_ref().map(|setup| setup.0.view()),
    ) {
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
pub(crate) fn process_row_node(name: &str, semantic_id: &str) -> accesskit::Node {
    let mut node = accesskit::Node::new(accesskit::Role::Row);
    node.set_label(name);
    node.set_description(semantic_id);
    node
}

/// Register the semantic projection. Called by the window plugin.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<SemanticSnapshotResource>()
        .add_systems(PostUpdate, sync_semantic_snapshot);
}

#[cfg(test)]
#[path = "../tests/headless/semantic.rs"]
mod tests;
