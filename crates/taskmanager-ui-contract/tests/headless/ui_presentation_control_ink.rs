//! test-intent: behavior

use super::*;

#[test]
fn the_strongest_interaction_signal_owns_one_ink() {
    for state in ControlInkState::ALL {
        let priority = ControlInkPriority {
            hovered: matches!(
                state,
                ControlInkState::Hovered | ControlInkState::Pressed | ControlInkState::Focused
            ),
            pressed: state == ControlInkState::Pressed,
            focused: state == ControlInkState::Focused,
            disabled: state == ControlInkState::Disabled,
            selected: state == ControlInkState::Selected,
        };
        assert_eq!(priority.state(), state);
    }
}

#[test]
fn conflicting_signals_resolve_by_the_shared_precedence() {
    assert_eq!(
        ControlInkPriority::resolve(true, true, true, true, true),
        ControlInkState::Disabled
    );
    assert_eq!(
        ControlInkPriority::resolve(true, true, true, false, true),
        ControlInkState::Pressed
    );
    assert_eq!(
        ControlInkPriority::resolve(true, false, true, false, false),
        ControlInkState::Focused
    );
    assert_eq!(
        ControlInkPriority::resolve(true, false, false, false, true),
        ControlInkState::Hovered
    );
    assert_eq!(
        ControlInkPriority::resolve(false, false, false, false, true),
        ControlInkState::Selected
    );
    assert_eq!(ControlInkPriority::IDLE.state(), ControlInkState::Idle);
}

#[test]
fn ink_never_resizes_a_control() {
    for state in ControlInkState::ALL {
        assert!(!state.changes_measurement());
    }
}

/// A control whose selection IS its own ink resolves a different precedence:
/// the selection outranks the hover, so resting the pointer on an active
/// segmented segment never erases the accent that shows which segment is
/// active. Every other precedence is untouched.
#[test]
fn a_selection_dominant_control_keeps_its_selection_under_the_pointer() {
    assert_eq!(
        ControlInkPriority::resolve_selection_dominant(true, false, false, false, true),
        ControlInkState::Selected
    );
    assert_eq!(
        ControlInkPriority::resolve_selection_dominant(true, true, false, false, true),
        ControlInkState::Pressed
    );
    assert_eq!(
        ControlInkPriority::resolve_selection_dominant(true, false, true, false, true),
        ControlInkState::Focused
    );
    assert_eq!(
        ControlInkPriority::resolve_selection_dominant(true, true, true, true, true),
        ControlInkState::Disabled
    );
    assert_eq!(
        ControlInkPriority::resolve_selection_dominant(true, false, false, false, false),
        ControlInkState::Hovered
    );
    assert_eq!(
        ControlInkPriority::resolve_selection_dominant(false, false, false, false, false),
        ControlInkState::Idle
    );
    // The general fold is unchanged: a control whose selection is NOT its own
    // ink still lets a transient hover win.
    assert_eq!(
        ControlInkPriority::resolve(true, false, false, false, true),
        ControlInkState::Hovered
    );
}
