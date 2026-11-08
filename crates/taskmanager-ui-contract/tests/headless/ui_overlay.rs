//! test-intent: behavior
//! Shared overlay dismissal policy: every normalized cause maps to one
//! documented outcome, and unlisted pairs are no-ops.

use super::*;

fn outcome(kind: OverlayKind, cause: OverlayDismissCause) -> OverlayDismissOutcome {
    dismiss_outcome(kind, cause, OverlayDismissContext::single())
}

#[test]
fn modal_cancels_on_escape_or_scrim_and_restores_focus() {
    for cause in [
        OverlayDismissCause::Escape,
        OverlayDismissCause::OutsidePress,
    ] {
        let decided = outcome(OverlayKind::Modal, cause);
        assert!(decided.dismiss, "{cause:?} must dismiss a modal");
        assert!(decided.restore_focus);
        assert!(!decided.forward_outside_press);
    }
    assert!(!outcome(OverlayKind::Modal, OverlayDismissCause::ItemActivation).dismiss);
    assert!(!outcome(OverlayKind::Modal, OverlayDismissCause::FocusLoss).dismiss);
}

#[test]
fn context_menu_dismisses_on_outside_item_escape_or_blur() {
    for cause in [
        OverlayDismissCause::OutsidePress,
        OverlayDismissCause::PointerPress,
        OverlayDismissCause::ItemActivation,
        OverlayDismissCause::Escape,
        OverlayDismissCause::WindowBlur,
    ] {
        let decided = outcome(OverlayKind::ContextMenu, cause);
        assert!(decided.dismiss, "{cause:?} must dismiss a context menu");
        assert!(!decided.restore_focus);
        assert!(decided.forward_outside_press);
    }
    assert!(!outcome(OverlayKind::ContextMenu, OverlayDismissCause::FocusLoss).dismiss);
    assert!(!outcome(OverlayKind::ContextMenu, OverlayDismissCause::PointerExit).dismiss);
}

#[test]
fn dropdown_dismisses_on_outside_escape_focus_loss_or_blur() {
    for cause in [
        OverlayDismissCause::OutsidePress,
        OverlayDismissCause::Escape,
        OverlayDismissCause::FocusLoss,
        OverlayDismissCause::WindowBlur,
    ] {
        let decided = outcome(OverlayKind::DropdownMenu, cause);
        assert!(decided.dismiss, "{cause:?} must dismiss a dropdown");
        assert!(decided.restore_focus);
        assert!(decided.forward_outside_press);
    }
    assert!(!outcome(OverlayKind::DropdownMenu, OverlayDismissCause::PointerExit).dismiss);
}

#[test]
fn dropdown_item_activation_respects_multi_select() {
    let single = dismiss_outcome(
        OverlayKind::DropdownMenu,
        OverlayDismissCause::ItemActivation,
        OverlayDismissContext::single(),
    );
    assert!(single.dismiss);
    let multi = dismiss_outcome(
        OverlayKind::DropdownMenu,
        OverlayDismissCause::ItemActivation,
        OverlayDismissContext::multi(),
    );
    assert!(!multi.dismiss, "a multi-select menu stays open");
}

#[test]
fn tooltip_dismisses_on_pointer_escape_or_focus_loss_without_focus_ownership() {
    for cause in [
        OverlayDismissCause::PointerExit,
        OverlayDismissCause::PointerPress,
        OverlayDismissCause::Escape,
        OverlayDismissCause::FocusLoss,
    ] {
        let decided = outcome(OverlayKind::Tooltip, cause);
        assert!(decided.dismiss, "{cause:?} must dismiss a tooltip");
        assert!(!decided.restore_focus);
        assert!(!decided.forward_outside_press);
    }
    assert!(!outcome(OverlayKind::Tooltip, OverlayDismissCause::OutsidePress).dismiss);
    assert!(!outcome(OverlayKind::Tooltip, OverlayDismissCause::ItemActivation).dismiss);
}

#[test]
fn the_named_tooltip_helper_agrees_with_the_cause_table() {
    for cause in [
        OverlayDismissCause::PointerExit,
        OverlayDismissCause::PointerPress,
        OverlayDismissCause::Escape,
        OverlayDismissCause::FocusLoss,
    ] {
        assert!(tooltip_dismisses(cause), "{cause:?} must dismiss a tooltip");
    }
    assert!(!tooltip_dismisses(OverlayDismissCause::OutsidePress));
    assert!(!tooltip_dismisses(OverlayDismissCause::ItemActivation));
    assert!(!tooltip_dismisses(OverlayDismissCause::WindowBlur));
}

#[test]
fn an_unlisted_pair_is_a_noop() {
    assert_eq!(
        outcome(OverlayKind::Modal, OverlayDismissCause::PointerExit),
        OverlayDismissOutcome {
            dismiss: false,
            restore_focus: false,
            forward_outside_press: false,
        }
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FixtureSurface {
    Settings,
    About,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FixtureKind {
    Settings,
    About,
}

impl OverlayKindOf for FixtureSurface {
    type Kind = FixtureKind;

    fn kind(&self) -> Self::Kind {
        match self {
            Self::Settings => FixtureKind::Settings,
            Self::About => FixtureKind::About,
        }
    }
}

fn open(surface: FixtureSurface) -> OverlaySlotCommand<FixtureSurface, FixtureKind, ()> {
    OverlaySlotCommand::Open(surface)
}

#[test]
fn one_slot_opens_replaces_and_dismisses_without_a_second_owner() {
    let mut slot = OverlaySlot::default();
    assert_eq!(
        slot.reduce(open(FixtureSurface::Settings)),
        OverlaySlotOutcome::Opened(FixtureKind::Settings)
    );
    assert_eq!(slot.kind(), Some(FixtureKind::Settings));
    assert_eq!(
        slot.reduce(open(FixtureSurface::About)),
        OverlaySlotOutcome::Replaced {
            previous: FixtureKind::Settings,
            current: FixtureKind::About,
        }
    );
    assert_eq!(
        slot.reduce(OverlaySlotCommand::Dismiss(FixtureKind::Settings, ())),
        OverlaySlotOutcome::Unchanged,
        "a stale close must not consume the current surface"
    );
    assert_eq!(slot.kind(), Some(FixtureKind::About));
    assert_eq!(
        slot.reduce(OverlaySlotCommand::Dismiss(FixtureKind::About, ())),
        OverlaySlotOutcome::Dismissed(FixtureKind::About, ())
    );
    assert_eq!(slot.kind(), None);
    assert_eq!(
        slot.reduce(OverlaySlotCommand::DismissCurrent(())),
        OverlaySlotOutcome::Unchanged
    );
}
