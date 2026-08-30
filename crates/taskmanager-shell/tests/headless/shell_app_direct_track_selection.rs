//! CORE-01 behavior battery: identity-authoritative selection reconcile on
//! the direct track. Every rule is deterministic and observable — refresh,
//! reorder, disappearance, and pid reuse each have exactly one outcome.

use crate::app::direct_track::ProcessSelection;
use crate::app::process_rows::ProcessRowId;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{ProcessItem, ProcessLiveKey, ProcessScalarObservations};

fn live_process(pid: u32, start_token: u64) -> ProcessItem {
    ProcessItem::new(pid, "worker").with_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(start_token, 1),
        ..ProcessScalarObservations::default()
    })
}

fn identity(pid: u32, start_token: u64) -> ProcessLiveKey {
    ProcessLiveKey::from_process(&live_process(pid, start_token))
        .expect("fixture carries a current start token")
}

#[test]
fn selection_survives_a_refresh_and_a_reorder_because_it_is_identity_keyed() {
    let a = identity(10, 100);
    let b = identity(20, 200);
    let mut selection = ProcessSelection::default();
    selection.toggle(a);
    selection.toggle(b);
    assert_eq!(selection.rows().len(), 2);

    // Same processes, snapshot order reversed: nothing changes.
    let reordered = [live_process(20, 200), live_process(10, 100)];
    selection.reconcile(&reordered);
    assert!(selection.contains(a) && selection.contains(b));
    assert_eq!(selection.anchor(), Some(b), "anchor keeps its identity");
}

#[test]
fn reconcile_drops_a_disappeared_row_without_retargeting() {
    let a = identity(10, 100);
    let gone = identity(20, 200);
    let mut selection = ProcessSelection::default();
    selection.select_single(gone);
    selection.toggle(a);

    selection.reconcile(&[live_process(10, 100)]);
    assert!(!selection.contains(gone), "disappeared row drops");
    assert!(selection.contains(a));
    assert_ne!(selection.anchor(), Some(gone));
}

#[test]
fn pid_reuse_never_matches_the_impostor() {
    let original = identity(30, 300);
    let mut selection = ProcessSelection::default();
    selection.select_single(original);

    // The pid exists in the new snapshot, but under a different provider
    // start token: a different process is reusing the pid.
    let impostor = live_process(30, 999);
    selection.reconcile(&[impostor]);
    assert!(
        !selection.contains(original),
        "the reused pid must not keep the old row selected"
    );
    assert_eq!(
        selection.anchor(),
        None,
        "the anchor never jumps to the impostor"
    );
    assert_eq!(selection.active_row(), None);
}

#[test]
fn structural_category_row_clears_actionable_selection() {
    let category =
        ProcessRowId::Category(taskmanager_core::core::process::ProcessCategory::Application);
    let mut selection = ProcessSelection::default();
    selection.select_single(identity(10, 100));
    selection.move_to_row(Some(category), false);

    // A structural header carries no process target: navigating onto it
    // clears the actionable selection instead of keeping a stale identity.
    assert_eq!(selection.active_row(), None);
    assert!(selection.batch_identities().is_empty());

    selection.reconcile(&[]);
    assert_eq!(selection.active_row(), None, "nothing to retarget");
}

#[test]
fn frozen_targets_resolve_exactly_and_fail_closed() {
    let a = identity(10, 100);
    let stale = identity(20, 200);
    let mut selection = ProcessSelection::default();
    selection.toggle(a);
    selection.toggle(stale);

    // Only `a` resolves against the live snapshot; `stale` (and a pid-reuse
    // impostor for pid 20) must not be frozen into a dangerous effect.
    let live = [live_process(10, 100), live_process(20, 999)];
    let frozen = selection.frozen_targets(&live);
    assert_eq!(frozen.len(), 1, "unresolvable identities are excluded");
    assert_eq!(frozen[0].pid, 10);
    assert_eq!(frozen[0].authoritative_start_token(), Some(100));
}

#[test]
fn extend_range_spans_the_display_order_by_identity() {
    let a = identity(10, 100);
    let b = identity(20, 200);
    let c = identity(30, 300);
    let display = [a, b, c];

    let mut selection = ProcessSelection::default();
    selection.select_single(a);
    selection.extend_range(&display, c);
    assert!(selection.contains(a) && selection.contains(b) && selection.contains(c));
    assert_eq!(selection.anchor(), Some(c));

    // A stale end identity inserts nothing (the caller keeps its prior set).
    let stale = identity(40, 400);
    selection.extend_range(&display, stale);
    assert!(!selection.contains(stale));
}
