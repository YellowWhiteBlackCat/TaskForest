use super::*;

#[test]
fn only_keyboard_modality_paints_a_focus_ring() {
    assert!(InputModality::Keyboard.shows_focus_ring());
    assert!(!InputModality::Pointer.shows_focus_ring());
    assert!(!InputModality::Programmatic.shows_focus_ring());
}

#[test]
fn modality_starts_programmatic_and_reports_real_changes() {
    let mut modality = InputModality::default();

    assert_eq!(modality, InputModality::Programmatic);
    assert!(modality.transition(InputModality::Keyboard));
    assert!(!modality.transition(InputModality::Keyboard));
    assert!(modality.transition(InputModality::Pointer));
    assert_eq!(modality, InputModality::Pointer);
}

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
