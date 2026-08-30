//! CORE-01/02 coverage for the shell's stable process-row authority.

use super::*;

use crate::ProcessRowAnchor;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{ProcessCategory, ProcessItem, ProcessScalarObservations};

fn selected_identity(app: &ShellApp) -> ProcessLiveKey {
    app.selected_row
        .and_then(ProcessRowId::live_key)
        .expect("the test selection is process-backed")
}

fn replacement(process: &ProcessItem, start_token: u64) -> ProcessItem {
    ProcessItem::new(process.pid, "replacement").with_scalar_observations(
        ProcessScalarObservations {
            start_token: ScalarObservation::available(start_token, 2),
            ..ProcessScalarObservations::default()
        },
    )
}

#[test]
fn row_anchor_preserves_identity_when_the_snapshot_reorders() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let identity = app.row_identity_at(0).expect("fixture row identity");
    let next_identity = app.row_identity_at(1).expect("second fixture row identity");
    let anchor = app.row_anchor_at(0).expect("fixture row anchor");
    assert!(app.select_row_anchor(anchor));

    crate::fixture::edit_processes(&mut app, |processes| {
        let rows = processes.as_mut().expect("fixture process rows");
        for process in rows {
            let Some(cpu) = (process.pid == identity.pid())
                .then_some(0.0)
                .or_else(|| (process.pid == next_identity.pid()).then_some(99.0))
            else {
                continue;
            };
            let mut observations = *process.scalar_observations();
            observations.cpu_percentage = ScalarObservation::available(cpu, 2);
            process.apply_scalar_observations(observations);
        }
    });

    assert_eq!(app.selected_row, Some(ProcessRowId::Process(identity)));
    let position = app
        .visible_position_of_identity(identity)
        .expect("reordered identity remains visible");
    assert_ne!(position, 0, "the projection order changed");
    assert_eq!(app.selected, position, "the cursor follows the stable row");
    assert_eq!(
        app.selected_process_identity().map(|target| target.pid),
        Some(identity.pid())
    );
    assert!(!app.select_row_anchor(anchor), "old geometry is stale");
}

#[test]
fn a_disappeared_identity_cannot_fall_back_to_a_neighboring_process() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    assert!(app.select_row(0));
    let identity = selected_identity(&app);

    crate::fixture::edit_processes(&mut app, |processes| {
        processes
            .as_mut()
            .expect("fixture process rows")
            .retain(|process| process.pid != identity.pid());
    });

    assert_eq!(app.selected_process_identity(), None);
    assert!(!app.selected_identities().contains(&identity));
    assert_eq!(
        app.request_process_batch(ProcessBatchAction::Suspend),
        None,
        "a disappeared row must not target the clamped neighbor"
    );
}

#[test]
fn a_reused_pid_is_a_new_row_and_cannot_keep_the_old_selection() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    assert!(app.select_row(0));
    let identity = selected_identity(&app);
    let old_token = identity.start_token();

    crate::fixture::edit_processes(&mut app, |processes| {
        let rows = processes.as_mut().expect("fixture process rows");
        let process = rows
            .iter_mut()
            .find(|process| process.pid == identity.pid())
            .expect("selected fixture process");
        *process = replacement(process, old_token.saturating_add(1));
    });

    let replacement_identity = app
        .projection()
        .processes_slice()
        .iter()
        .find(|process| process.pid == identity.pid())
        .and_then(ProcessLiveKey::from_process)
        .expect("replacement row identity");
    assert_ne!(replacement_identity, identity);
    assert_eq!(app.selected_process_identity(), None);
    assert!(!app.selected_identities().contains(&replacement_identity));
    assert_eq!(
        app.request_process_batch(ProcessBatchAction::Suspend),
        None,
        "PID reuse must not retarget the replacement"
    );
}

#[test]
fn application_row_uses_the_root_identity_without_fabricating_a_member_selection() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let root = app.row_identity_at(0).expect("fixture root identity");

    assert!(app.select_row_id(ProcessRowId::Application(root)));
    assert_eq!(app.selected_row, Some(ProcessRowId::Application(root)));
    assert!(app.selected_identities().is_empty());
    assert_eq!(app.selected_process_identity(), None);

    let Some(PlatformEffect::ExecuteBatch(intent)) =
        app.request_process_batch(ProcessBatchAction::Suspend)
    else {
        panic!("application row must freeze its root tree identity");
    };
    assert_eq!(intent.targets.len(), 1);
    assert_eq!(intent.targets[0].pid, root.pid());
    assert_eq!(
        intent.targets[0].authoritative_start_token(),
        Some(root.start_token())
    );
}

#[test]
fn category_anchor_is_structural_and_cannot_create_a_process_target() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let category = ProcessRowId::Category(ProcessCategory::Application);
    let anchor = ProcessRowAnchor::new(category, app.projection().process_projection_generation());

    assert!(app.select_row_anchor(anchor));
    assert_eq!(app.selected_row, Some(category));
    assert_eq!(app.selected_row_anchor(), Some(anchor));
    assert_eq!(app.selected_process_identity(), None);
    assert_eq!(app.request_process_batch(ProcessBatchAction::Suspend), None);
}

#[test]
fn stale_anchor_generation_is_rejected_even_when_the_identity_survives() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let anchor = app.row_anchor_at(0).expect("fixture row anchor");

    crate::fixture::seed_projection_fact(
        &mut app,
        crate::fixture::ProjectionSeedFact::AdvanceRevision(
            crate::fixture::ProjectionSeedDomain::Processes,
        ),
    );

    assert!(!app.select_row_anchor(anchor));
}
