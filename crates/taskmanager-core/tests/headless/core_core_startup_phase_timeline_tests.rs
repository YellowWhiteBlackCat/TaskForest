//! test-intent: behavior
//!
//! The boot phase split has one honest distinction that a renderer depends
//! on: a phase that was measured carries a duration, a phase that was not
//! measured stays absent. The tests below pin that distinction so a zero-width
//! bar can never be mistaken for a measured phase.

use super::*;

#[test]
fn an_empty_phase_split_measures_nothing() {
    let snapshot = StartupTimelineSnapshot::default();

    assert!(!snapshot.has_measured_phase());
    assert_eq!(snapshot.measured_total_ms(), 0);
    assert!(
        snapshot
            .phases()
            .iter()
            .all(|phase| phase.duration_ms.is_none()),
        "an empty split must not carry a zero duration for any phase"
    );
}

#[test]
fn the_phases_are_published_in_boot_order() {
    let snapshot = StartupTimelineSnapshot {
        firmware_ms: Some(10),
        loader_ms: Some(20),
        kernel_ms: Some(30),
        user_space_ms: Some(40),
    };

    let kinds: Vec<StartupPhaseKind> = snapshot.phases().iter().map(|phase| phase.kind).collect();
    assert_eq!(
        kinds,
        vec![
            StartupPhaseKind::Firmware,
            StartupPhaseKind::Loader,
            StartupPhaseKind::Kernel,
            StartupPhaseKind::UserSpace,
        ]
    );
    let durations: Vec<Option<u64>> = snapshot
        .phases()
        .iter()
        .map(|phase| phase.duration_ms)
        .collect();
    assert_eq!(durations, vec![Some(10), Some(20), Some(30), Some(40)]);
}

#[test]
fn an_unmeasured_phase_stays_absent_while_its_neighbours_keep_their_values() {
    let snapshot = StartupTimelineSnapshot {
        firmware_ms: Some(1_200),
        loader_ms: None,
        kernel_ms: Some(5_600),
        user_space_ms: None,
    };

    assert!(snapshot.has_measured_phase());
    let durations: Vec<Option<u64>> = snapshot
        .phases()
        .iter()
        .map(|phase| phase.duration_ms)
        .collect();
    assert_eq!(
        durations,
        vec![Some(1_200), None, Some(5_600), None],
        "an unmeasured phase must stay absent instead of borrowing a neighbour"
    );
}

#[test]
fn the_measured_total_counts_only_the_phases_that_were_measured() {
    let snapshot = StartupTimelineSnapshot {
        firmware_ms: Some(1_200),
        loader_ms: None,
        kernel_ms: Some(5_600),
        user_space_ms: None,
    };

    assert_eq!(snapshot.measured_total_ms(), 6_800);
}

#[test]
fn a_measured_zero_is_distinct_from_an_unmeasured_phase() {
    let measured_zero = StartupTimelineSnapshot {
        kernel_ms: Some(0),
        ..StartupTimelineSnapshot::default()
    };
    let unmeasured = StartupTimelineSnapshot::default();

    assert!(measured_zero.has_measured_phase());
    assert!(!unmeasured.has_measured_phase());
    assert_eq!(measured_zero.kernel_ms, Some(0));
    assert_eq!(unmeasured.kernel_ms, None);
}
