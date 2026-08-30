//! test-intent: behavior
//!
//! Headless behavior tests for the accessibility seam (`src/semantic.rs`):
//! what an assistive-technology user can learn from the shared
//! `SemanticSnapshot` over the folded shell. The assertions are the product
//! facts, not builder structure: a row keeps one stable identity across
//! rebuilds while its selected state follows the cursor, an unavailable share
//! is announced as unavailable (never as zero), and arming the confirmation
//! gate surfaces a modal an AT user can read, understand, and dismiss.

use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{ProcessItem, ProcessScalarObservations};

use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture;
use taskmanager_ui_contract::{SemanticAction, SemanticNodeId, SemanticRole};

use super::build_snapshot;
use crate::confirmation::PendingConfirmationView;

/// A process with a CPU value but no memory denominator — the honest
/// unavailable memory share is exactly the case the snapshot must express.
/// The start token makes the row selectable by the shared gate vocabulary.
fn fixture_process(pid: u32, name: &str, cpu: f32) -> ProcessItem {
    let mut process = ProcessItem::new(pid, name);
    process.apply_scalar_observations(ProcessScalarObservations {
        cpu_percentage: ScalarObservation::available(cpu, 1),
        start_token: ScalarObservation::available(u64::from(pid) * 10_000, 1),
        ..Default::default()
    });
    process
}

fn shell_with(items: Vec<ProcessItem>) -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| *processes = Some(items));
    let _ = shell.apply_action(taskmanager_application::AppAction::SelectPage(
        taskmanager_application::AppPage::Applications,
    ));
    shell
}

fn row_id(pid: u32) -> SemanticNodeId {
    SemanticNodeId::owned(format!(
        "row:process:pid:{pid}:start:{}",
        u64::from(pid) * 10_000
    ))
}

fn cell_id(pid: u32, cell: &str) -> SemanticNodeId {
    SemanticNodeId::owned(format!("{}:cell:{cell}", row_id(pid).as_str()))
}

#[test]
fn rows_keep_stable_identity_while_selected_state_follows_the_cursor() {
    let mut shell = shell_with(vec![
        fixture_process(100, "alpha", 12.5),
        fixture_process(200, "beta", 3.0),
    ]);

    let before = build_snapshot(&shell).expect("the shell projects a valid snapshot");
    // Move the keyboard cursor to the second row exactly like the input
    // seam's arrow path does, then rebuild the snapshot independently.
    shell.move_selection(1);
    let after = build_snapshot(&shell).expect("a rebuild stays valid");

    // The identity is stable across rebuilds — an AT user keeps their join
    // to the same row — while the selected state moved to the landed row.
    let alpha_before = before.get(&row_id(100)).expect("alpha row present");
    let alpha_after = after.get(&row_id(100)).expect("alpha row survives");
    assert_eq!(alpha_before.id(), alpha_after.id());
    assert_eq!(
        alpha_before.state().selected,
        Some(true),
        "the snapshot before the move marks the landed row selected"
    );
    assert_eq!(
        alpha_after.state().selected,
        Some(false),
        "the snapshot after the move releases the departed row"
    );
    assert_eq!(
        after
            .get(&row_id(200))
            .expect("beta row present")
            .state()
            .selected,
        Some(true),
        "the cursor row announces itself as selected"
    );
}

#[test]
fn unavailable_shares_are_announced_unavailable_and_real_values_are_announced() {
    let shell = shell_with(vec![fixture_process(100, "alpha", 12.5)]);
    let snapshot = build_snapshot(&shell).expect("valid snapshot");

    let cpu = snapshot
        .get(&cell_id(100, "cpu"))
        .expect("the cpu cell exists");
    assert_eq!(
        cpu.value_text(),
        Some("12.5%"),
        "a trustworthy CPU value is announced with its real magnitude"
    );

    let memory = snapshot
        .get(&cell_id(100, "memory"))
        .expect("the memory cell exists");
    let memory_text = memory.value_text().expect("the memory cell speaks");
    assert_ne!(memory_text, "0.0%");
    assert_ne!(memory_text, "0%");
    assert!(
        memory_text.to_ascii_lowercase().contains("unavail"),
        "a share without a denominator is announced unavailable, got {memory_text:?}"
    );
}

#[test]
fn arming_the_gate_surfaces_a_modal_an_at_user_can_dismiss() {
    let mut shell = shell_with(vec![fixture_process(100, "alpha", 1.0)]);

    // Before arming: no dialog exists in the semantic tree.
    let quiet = build_snapshot(&shell).expect("valid snapshot");
    assert!(
        quiet
            .nodes()
            .all(|node| node.role() != SemanticRole::Dialog),
        "no confirmation, no modal surface"
    );

    // Arm the gate through the same shared action the Delete chord fires.
    let _ = shell.apply_action(taskmanager_application::AppAction::RequestEndTask);
    let pending = shell
        .pending_confirmation()
        .expect("the gate armed for the selected process")
        .clone();
    let view = PendingConfirmationView::from_pending(&pending).expect("EndTask renders a view");

    let armed = build_snapshot(&shell).expect("valid snapshot with the modal");
    let dialog = armed
        .nodes()
        .find(|node| node.role() == SemanticRole::Dialog)
        .expect("the armed gate is discoverable as a dialog");
    assert_eq!(
        dialog.name(),
        Some(view.title.as_str()),
        "the dialog is named by the exact confirmation copy the sighted user reads"
    );
    assert_eq!(
        dialog.description(),
        Some(view.body.as_str()),
        "the dialog description echoes the frozen target"
    );
    assert!(
        dialog.supports_action(SemanticAction::Dismiss),
        "the AT path can dismiss the confirmation"
    );
}
