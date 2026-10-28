//! Toolkit-neutral overlay dismissal and focus-restoration policy.
//!
//! This module is the single authority for which causes dismiss an overlay and
//! whether dismissal restores focus or forwards an outside press to the
//! background target. Frontends map their native events to
//! [`OverlayDismissCause`] and execute their own close path; they must not
//! re-declare the policy. The semantic rules come from the capability specs in
//! [`crate::capabilities`] (`ModalOverlay`, `ContextMenu`, `DropdownMenu`,
//! `Tooltip`).

/// The product overlay family whose dismissal policy is being queried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverlayKind {
    /// A modal dialog with a scrim and focus containment.
    Modal,
    /// A contextual action menu anchored to a targeted object.
    ContextMenu,
    /// A menu anchored to a control or surface.
    DropdownMenu,
    /// A read-only explanation surface.
    Tooltip,
}

/// One normalized dismissal cause every frontend maps its native events onto.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverlayDismissCause {
    /// Escape (or the terminal's equivalent cancel chord).
    Escape,
    /// A press/click outside the overlay (the modal scrim included).
    OutsidePress,
    /// The pointer left the tooltip anchor.
    PointerExit,
    /// A pointer press on the overlay surface.
    PointerPress,
    /// Keyboard focus left the overlay or its trigger.
    FocusLoss,
    /// An item inside the overlay was activated.
    ItemActivation,
    /// The window lost focus.
    WindowBlur,
}

/// The policy outcome for one `(kind, cause)` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayDismissOutcome {
    /// Whether this cause dismisses the overlay.
    pub dismiss: bool,
    /// Whether dismissal restores focus to the trigger when one exists.
    pub restore_focus: bool,
    /// Whether the outside press is forwarded to the background target after
    /// the overlay closes. The reference GPUI shape forwards it; a shape that
    /// blocks the background interaction records that difference at its
    /// capability declaration instead of changing this policy.
    pub forward_outside_press: bool,
}

/// Caller-supplied overlay state that changes the item-activation outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayDismissContext {
    /// A multi-select menu stays open across individual item toggles.
    pub multi_select: bool,
}

impl OverlayDismissContext {
    /// A single-select overlay.
    #[must_use]
    pub const fn single() -> Self {
        Self {
            multi_select: false,
        }
    }

    /// A multi-select overlay (for example a column chooser).
    #[must_use]
    pub const fn multi() -> Self {
        Self { multi_select: true }
    }
}

/// Decide one normalized dismissal event. An unlisted pair is a no-op.
#[must_use]
pub const fn dismiss_outcome(
    kind: OverlayKind,
    cause: OverlayDismissCause,
    context: OverlayDismissContext,
) -> OverlayDismissOutcome {
    use OverlayDismissCause::{
        Escape, FocusLoss, ItemActivation, OutsidePress, PointerExit, PointerPress, WindowBlur,
    };
    use OverlayKind::{ContextMenu, DropdownMenu, Modal, Tooltip};
    let (dismiss, restore_focus, forward_outside_press) = match (kind, cause) {
        // A modal cancels on Escape or a scrim/outside press and restores focus.
        (Modal, Escape | OutsidePress) => (true, true, false),
        // A context menu dismisses on outside press, item activation, Escape,
        // or window blur; it does not own focus and forwards the outside press.
        (ContextMenu, OutsidePress | PointerPress | ItemActivation | Escape | WindowBlur) => {
            (true, false, true)
        }
        // A dropdown dismisses on outside press, focus loss, Escape, or window
        // blur, and forwards the outside press to the background target.
        (DropdownMenu, OutsidePress | FocusLoss | Escape | WindowBlur) => (true, true, true),
        // A single-select dropdown closes on item activation; a multi-select
        // menu stays open across individual toggles.
        (DropdownMenu, ItemActivation) if !context.multi_select => (true, true, false),
        // A tooltip is hover/focus-read-only: pointer exit, pointer press,
        // Escape, or focus loss dismisses it; it never owns focus.
        (Tooltip, PointerExit | PointerPress | Escape | FocusLoss) => (true, false, false),
        _ => (false, false, false),
    };
    OverlayDismissOutcome {
        dismiss,
        restore_focus,
        forward_outside_press,
    }
}

#[cfg(test)]
#[path = "../tests/headless/ui_overlay.rs"]
mod tests;
