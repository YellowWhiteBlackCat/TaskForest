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

/// Whether the shared overlay policy dismisses a tooltip on one normalized
/// cause.
///
/// A tooltip is hover/focus-read-only: pointer exit, pointer press, Escape or
/// focus loss dismisses it. The verdict belongs to the cause table above, so
/// no frontend re-derives it from the kind/cause table.
#[must_use]
pub const fn tooltip_dismisses(cause: OverlayDismissCause) -> bool {
    dismiss_outcome(OverlayKind::Tooltip, cause, OverlayDismissContext::single()).dismiss
}

/// The one toolkit-neutral active overlay slot.
///
/// A frontend owns its own surface payload and kind type, but must not
/// re-declare `Option<S>` plus the Open/Dismiss/DismissCurrent state machine.
/// The slot exposes the raw replacement/dismissal facts; each product maps
/// them onto its own typed transition without a second semantic path.
#[derive(Debug)]
pub struct OverlaySlot<S>(Option<S>);

impl<S> Default for OverlaySlot<S> {
    fn default() -> Self {
        Self(None)
    }
}

/// The typed identity of one overlay payload. `kind` is a product-owned
/// classification, never a second copy of the slot's state machine.
pub trait OverlayKindOf {
    type Kind: Copy + Eq;

    fn kind(&self) -> Self::Kind;
}

/// One normalized command every frontend maps its native events onto.
pub enum OverlaySlotCommand<S, K, D> {
    Open(S),
    DismissCurrent(D),
    Dismiss(K, D),
}

/// The toolkit-neutral event vocabulary. Frontends alias this with their own
/// surface/kind/dismissal types; no product re-declares the variants.
pub enum OverlayEvent<S, K, D> {
    Open(S),
    DismissCurrent(D),
    Dismiss(K, D),
}

/// The toolkit-neutral transition vocabulary. `Opened`/`Replaced` carry the
/// product kind; `Dismissed` carries the kind plus the product's own dismissal
/// payload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverlayTransition<K, D> {
    #[default]
    Unchanged,
    Opened(K),
    Replaced {
        previous: K,
        current: K,
    },
    Dismissed(K, D),
}

/// Facts emitted by one slot transition, with the caller's own dismissal payload attached to the
/// dismissal branch only. `Open`/`Replaced`/`Unchanged` never invent one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlaySlotOutcome<K, D> {
    Opened(K),
    Replaced { previous: K, current: K },
    Dismissed(K, D),
    Unchanged,
}

impl<S> OverlaySlot<S> {
    /// Construct a slot with one pre-existing active payload (used by the
    /// GPUI window's startup restore path).
    #[must_use]
    pub const fn initial(active: Option<S>) -> Self {
        Self(active)
    }

    #[must_use]
    pub const fn active(&self) -> Option<&S> {
        self.0.as_ref()
    }

    #[must_use]
    pub const fn active_mut(&mut self) -> Option<&mut S> {
        self.0.as_mut()
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.0.is_some()
    }

    /// Replace the active payload. `None` means this is the first open;
    /// `Some(previous)` means another product surface was replaced.
    pub fn open(&mut self, surface: S) -> Option<S> {
        self.0.replace(surface)
    }

    /// Dismiss the active payload, if any.
    pub fn dismiss_current(&mut self) -> Option<S> {
        self.0.take()
    }
}

impl<S: OverlayKindOf> OverlaySlot<S> {
    #[must_use]
    pub fn kind(&self) -> Option<<S as OverlayKindOf>::Kind> {
        self.0.as_ref().map(OverlayKindOf::kind)
    }
}

impl<S: OverlayKindOf> OverlaySlot<S> {
    /// Take the active payload only when its kind matches the expected branch.
    pub fn dismiss_expected(&mut self, expected: <S as OverlayKindOf>::Kind) -> Option<S> {
        if self.kind() == Some(expected) {
            self.0.take()
        } else {
            None
        }
    }

    /// One owner for Open/DismissCurrent/Dismiss. `D` is the frontend's own
    /// dismissal payload (GPUI's reason, or `()` for the other shapes); the
    /// owner never interprets it.
    pub fn reduce<D>(
        &mut self,
        command: OverlaySlotCommand<S, <S as OverlayKindOf>::Kind, D>,
    ) -> OverlaySlotOutcome<<S as OverlayKindOf>::Kind, D> {
        match command {
            OverlaySlotCommand::Open(surface) => {
                let current = surface.kind();
                match self.open(surface) {
                    Some(previous) => OverlaySlotOutcome::Replaced {
                        previous: previous.kind(),
                        current,
                    },
                    None => OverlaySlotOutcome::Opened(current),
                }
            }
            OverlaySlotCommand::DismissCurrent(detail) => {
                let dismissed = self.dismiss_current().map(|surface| surface.kind());
                match dismissed {
                    Some(kind) => OverlaySlotOutcome::Dismissed(kind, detail),
                    None => OverlaySlotOutcome::Unchanged,
                }
            }
            OverlaySlotCommand::Dismiss(expected, detail) => {
                let dismissed = self
                    .dismiss_expected(expected)
                    .map(|surface| surface.kind());
                match dismissed {
                    Some(kind) => OverlaySlotOutcome::Dismissed(kind, detail),
                    None => OverlaySlotOutcome::Unchanged,
                }
            }
        }
    }

    /// Apply the shared event vocabulary through the single state machine.
    pub fn apply<D>(
        &mut self,
        event: OverlayEvent<S, <S as OverlayKindOf>::Kind, D>,
    ) -> OverlayTransition<<S as OverlayKindOf>::Kind, D> {
        let command = match event {
            OverlayEvent::Open(surface) => OverlaySlotCommand::Open(surface),
            OverlayEvent::DismissCurrent(detail) => OverlaySlotCommand::DismissCurrent(detail),
            OverlayEvent::Dismiss(expected, detail) => {
                OverlaySlotCommand::Dismiss(expected, detail)
            }
        };
        match self.reduce(command) {
            OverlaySlotOutcome::Opened(current) => OverlayTransition::Opened(current),
            OverlaySlotOutcome::Replaced { previous, current } => {
                OverlayTransition::Replaced { previous, current }
            }
            OverlaySlotOutcome::Dismissed(kind, detail) => {
                OverlayTransition::Dismissed(kind, detail)
            }
            OverlaySlotOutcome::Unchanged => OverlayTransition::Unchanged,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/ui_overlay.rs"]
mod tests;
