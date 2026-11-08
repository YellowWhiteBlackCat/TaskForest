use super::*;
use taskmanager_application::AppPage;

const SERVICES: VimContext = VimContext {
    page: AppPage::Services,
    perf_device: PerfDevice::Cpu,
};
const SYSTEM: VimContext = VimContext {
    page: AppPage::System,
    perf_device: PerfDevice::Cpu,
};
const GPU: VimContext = VimContext {
    page: AppPage::Performance,
    perf_device: PerfDevice::Gpu,
};

#[test]
fn gg_arms_and_completes_only_once() {
    let available = VimAvailability {
        first_row: true,
        ..VimAvailability::default()
    };

    let first = advance(VimPending::Idle, VimKey::G, SERVICES, available);
    assert_eq!(first.pending, VimPending::FirstRow { context: SERVICES });
    assert_eq!(first.action, None);
    assert!(first.consumed);

    let second = advance(first.pending, VimKey::G, SERVICES, available);
    assert_eq!(second.pending, VimPending::Idle);
    assert_eq!(second.action, Some(VimAction::FirstRow));
    assert!(second.consumed);

    let replay = advance(second.pending, VimKey::G, SERVICES, available);
    assert_eq!(replay.pending, VimPending::FirstRow { context: SERVICES });
}

#[test]
fn a_stale_context_cannot_complete_and_restarts_in_the_new_context() {
    let pending = VimPending::FirstRow { context: SERVICES };
    let available = VimAvailability {
        first_row: true,
        ..VimAvailability::default()
    };

    let transition = advance(pending, VimKey::G, SYSTEM, available);
    assert_eq!(transition.pending, VimPending::FirstRow { context: SYSTEM });
    assert_eq!(transition.action, None);
    assert!(transition.consumed);
}

#[test]
fn unrelated_or_unavailable_keys_cancel_without_hiding_the_next_owner() {
    let pending = VimPending::FirstRow { context: SERVICES };
    let empty = VimAvailability::default();

    let unrelated = advance(pending, VimKey::Slash, SERVICES, empty);
    assert_eq!(unrelated.pending, VimPending::Idle);
    assert_eq!(unrelated.action, None);
    assert!(!unrelated.consumed);

    let unavailable = advance(
        VimPending::ClearMarks { context: SYSTEM },
        VimKey::D,
        SYSTEM,
        VimAvailability {
            clear_marks: false,
            ..VimAvailability::default()
        },
    );
    assert_eq!(unavailable.pending, VimPending::Idle);
    assert!(!unavailable.consumed);
}

#[test]
fn dd_clear_marks_sequence_is_finite_and_repeatable() {
    let available = VimAvailability {
        clear_marks: true,
        ..VimAvailability::default()
    };
    let first = advance(VimPending::Idle, VimKey::D, SYSTEM, available);
    assert_eq!(first.pending, VimPending::ClearMarks { context: SYSTEM });
    let second = advance(first.pending, VimKey::D, SYSTEM, available);
    assert_eq!(second.action, Some(VimAction::ClearMarks));
    assert_eq!(second.pending, VimPending::Idle);
}

#[test]
fn uppercase_g_and_slash_are_single_key_safe_shortcuts() {
    let last = advance(
        VimPending::Idle,
        VimKey::ShiftG,
        SERVICES,
        VimAvailability {
            last_row: true,
            ..VimAvailability::default()
        },
    );
    assert_eq!(last.action, Some(VimAction::LastRow));
    assert!(last.consumed);

    let search = advance(
        VimPending::Idle,
        VimKey::Slash,
        SERVICES,
        VimAvailability {
            search: true,
            ..VimAvailability::default()
        },
    );
    assert_eq!(search.action, Some(VimAction::Search));
    assert!(search.consumed);
}

#[test]
fn a_device_change_invalidates_a_pending_performance_chord() {
    let pending = VimPending::FirstRow { context: GPU };
    let available = VimAvailability {
        first_row: true,
        ..VimAvailability::default()
    };

    let transition = advance(pending, VimKey::G, GPU, available);
    assert_eq!(transition.pending, VimPending::Idle);
    assert_eq!(transition.action, Some(VimAction::FirstRow));

    let after_device_change = advance(pending, VimKey::G, SERVICES, available);
    assert_eq!(
        after_device_change.pending,
        VimPending::FirstRow { context: SERVICES }
    );
    assert_eq!(after_device_change.action, None);
}
