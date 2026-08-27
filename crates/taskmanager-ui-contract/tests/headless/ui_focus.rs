use super::*;

#[test]
fn modal_entry_and_exit_have_explicit_semantic_targets() {
    let policy = ModalFocusPolicy::contained(512);
    let token = FocusRestoreToken::new(17);

    assert_eq!(policy.initial_target(), FocusTarget::ModalScope);
    assert_eq!(
        policy.restore_target(Some(token)),
        FocusTarget::Restore(token)
    );
    assert_eq!(policy.restore_target(None), FocusTarget::Clear);
    assert_eq!(token.value(), 17);
}

#[test]
fn cycle_settles_as_soon_as_focus_enters_the_modal() {
    let mut cycle = ModalFocusPolicy::contained(3).begin_cycle();

    assert_eq!(cycle.observe(false), FocusCycleStep::Continue);
    assert_eq!(cycle.observe(true), FocusCycleStep::Settled);
}

#[test]
fn exhausted_cycle_fails_closed_to_modal_scope() {
    let mut cycle = ModalFocusPolicy::contained(2).begin_cycle();

    assert_eq!(cycle.observe(false), FocusCycleStep::Continue);
    assert_eq!(
        cycle.observe(false),
        FocusCycleStep::Focus(FocusTarget::ModalScope)
    );
}

#[test]
fn zero_scan_limit_still_performs_one_bounded_attempt() {
    let policy = ModalFocusPolicy::contained(0);
    let mut cycle = policy.begin_cycle();

    assert_eq!(policy.scan_limit(), 1);
    assert_eq!(
        cycle.observe(false),
        FocusCycleStep::Focus(FocusTarget::ModalScope)
    );
}
