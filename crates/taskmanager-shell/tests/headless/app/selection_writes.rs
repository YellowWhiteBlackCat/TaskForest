//! The named selection-write seam grouped/tree frontends consume instead of
//! assigning the shell's selection fields directly: one writer for the
//! resolved row + detail identity pair, one for multi-select membership, and
//! one for the positional cursor.

use super::*;
use taskmanager_application::AppPage;
use taskmanager_core::core::process::ProcessCategory;

/// Resolve one fixture row identity by visible position.
fn identity_at(app: &ShellApp, index: usize) -> ProcessLiveKey {
    app.row_identity_at(index).expect("fixture process row")
}

#[test]
fn set_row_selection_moves_the_row_and_the_detail_identity_together() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let identity = identity_at(&app, 1);
    let process = app.process_by_identity(identity).cloned();

    app.set_row_selection(Some(ProcessRowId::Process(identity)), process.as_ref());

    assert_eq!(
        app.selected_row,
        Some(ProcessRowId::Process(identity)),
        "the resolved row becomes the semantic primary row"
    );
    assert_eq!(
        app.application.selected_process.map(|frozen| frozen.pid),
        Some(identity.pid()),
        "the typed detail identity follows the same row"
    );
}

#[test]
fn set_row_selection_clears_both_fields_off_the_applications_page() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let identity = identity_at(&app, 0);
    let process = app.process_by_identity(identity).cloned();
    app.set_row_selection(Some(ProcessRowId::Process(identity)), process.as_ref());
    assert!(app.selected_row.is_some());

    // The same call on another page reports no Applications selection at all,
    // instead of leaving a stale row pointing at another table.
    app.application.active_page = AppPage::Services;
    app.set_row_selection(Some(ProcessRowId::Process(identity)), process.as_ref());
    assert_eq!(app.selected_row, None);
    assert_eq!(
        app.application.selected_process, None,
        "the detail identity is cleared with the row, not left pointing at another page's table"
    );
}

#[test]
fn set_row_selection_reports_a_category_row_without_a_process_target() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let identity = identity_at(&app, 0);
    let process = app.process_by_identity(identity).cloned();
    app.set_row_selection(Some(ProcessRowId::Process(identity)), process.as_ref());

    // A structural header row carries no process identity, so the detail
    // panel must fall back to the honest empty state.
    app.set_row_selection(
        Some(ProcessRowId::Category(ProcessCategory::Application)),
        None,
    );
    assert_eq!(
        app.selected_row,
        Some(ProcessRowId::Category(ProcessCategory::Application))
    );
    assert_eq!(
        app.application.selected_process, None,
        "a structural header row carries no process target"
    );
}

#[test]
fn add_selected_identity_extends_the_set_without_toggling() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let first = identity_at(&app, 0);
    let second = identity_at(&app, 1);

    app.clear_selected_rows();
    app.add_selected_identity(first);
    app.add_selected_identity(second);
    app.add_selected_identity(first);

    let set = app.selected_identities();
    assert_eq!(set.len(), 2, "a repeated add must not toggle membership");
    assert!(set.contains(&first) && set.contains(&second));
}

#[test]
fn move_selection_to_repoints_the_cursor_without_rederiving_the_row() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let identity = identity_at(&app, 0);
    let process = app.process_by_identity(identity).cloned();
    app.set_row_selection(Some(ProcessRowId::Process(identity)), process.as_ref());
    let row_before = app.selected_row;

    // A grouped frontend's cursor indexes its own interleaved visual list, so
    // the flat projection must not be consulted here.
    app.move_selection_to(3);
    assert_eq!(app.selected, 3);
    assert_eq!(app.selected_row, row_before);
}

#[test]
fn the_grouped_arrow_sequence_reproduces_the_previous_direct_writes() {
    // What a grouped frontend used to do with four field assignments must be
    // reachable through the named seam alone: collapse the multi-select set,
    // re-add the landed row, and move the cursor.
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    assert!(app.select_row(0));
    let first = identity_at(&app, 0);
    assert!(app.selected_identities().contains(&first));
    let second = identity_at(&app, 2);
    let second_process = app.process_by_identity(second).cloned();

    app.clear_selected_rows();
    app.set_row_selection(Some(ProcessRowId::Process(second)), second_process.as_ref());
    app.add_selected_identity(second);
    app.move_selection_to(2);

    assert_eq!(app.selected, 2);
    assert_eq!(app.selected_row, Some(ProcessRowId::Process(second)));
    assert_eq!(app.selected_identities().len(), 1);
    assert!(app.selected_identities().contains(&second));
    assert_eq!(
        app.application.selected_process.map(|frozen| frozen.pid),
        Some(second.pid())
    );
}

#[test]
fn the_batch_confirmation_authority_gates_by_destructive_and_reach() {
    use taskmanager_core::core::process::{PriorityTier, ProcessBatchAction};

    // A destructive verb is gated whatever it targets.
    assert!(ShellApp::process_batch_is_destructive(
        ProcessBatchAction::Kill
    ));
    assert!(ShellApp::process_batch_is_destructive(
        ProcessBatchAction::End
    ));
    assert!(ShellApp::process_batch_requires_confirmation(
        ProcessBatchAction::Kill,
        1,
        false
    ));
    // A single non-destructive target applies immediately.
    assert!(!ShellApp::process_batch_is_destructive(
        ProcessBatchAction::Suspend
    ));
    assert!(!ShellApp::process_batch_requires_confirmation(
        ProcessBatchAction::Suspend,
        1,
        false
    ));
    // Reaching past the looked-at row gates a reversible verb too: a multi-row
    // set, and an application tree even when it holds one process.
    let reversible = [
        ProcessBatchAction::Suspend,
        ProcessBatchAction::Resume,
        ProcessBatchAction::SetPriority(PriorityTier::Normal),
    ];
    for action in reversible {
        assert!(ShellApp::process_batch_requires_confirmation(
            action, 2, false
        ));
        assert!(ShellApp::process_batch_requires_confirmation(
            action, 1, true
        ));
    }
}

#[test]
fn selection_queries_report_the_gate_before_the_intent_is_frozen() {
    use taskmanager_core::core::process::ProcessBatchAction;

    // Single selected process row: a reversible verb submits directly.
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    assert!(app.select_row(0));
    assert!(!app.selection_requires_batch_confirmation(ProcessBatchAction::Suspend));
    assert!(app.selection_requires_batch_confirmation(ProcessBatchAction::Kill));

    // Two marked rows: the reversible verb is gated before anything freezes.
    assert!(app.toggle_row_selection(2));
    assert_eq!(app.selected_identities().len(), 2);
    assert!(app.selection_requires_batch_confirmation(ProcessBatchAction::Suspend));

    // An application root is a tree freeze, gated even for one descendant.
    let root = app.row_identity_at(4).expect("fixture root identity");
    assert!(app.select_row_id(ProcessRowId::Application(root)));
    assert!(app.selection_requires_batch_confirmation(ProcessBatchAction::Resume));
    // The query predicts what the request does.
    assert_eq!(app.request_process_batch(ProcessBatchAction::Resume), None);
    assert!(app.pending_batch().is_some());
}

#[test]
fn application_tree_end_is_typed_as_a_tree_action() {
    use taskmanager_core::core::process::ProcessBatchAction;

    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let root = app.row_identity_at(4).expect("fixture root identity");
    assert!(app.select_row_id(ProcessRowId::Application(root)));

    assert_eq!(app.request_process_batch(ProcessBatchAction::End), None);
    assert_eq!(
        app.pending_batch().map(|intent| intent.action),
        Some(ProcessBatchAction::EndProcessTree),
        "a tree expansion must not be presented as a single-task end"
    );
}
