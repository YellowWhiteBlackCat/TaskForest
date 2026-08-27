//! Toolkit-neutral focus and modal-containment policy.

/// Opaque identity for the control that should regain focus after a modal closes.
///
/// Frontend adapters own the actual toolkit handle and associate it with this
/// token. Keeping only the identity here prevents toolkit objects from leaking
/// into the shared contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FocusRestoreToken(u64);

impl FocusRestoreToken {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Semantic focus destination. Frontends decide how each target maps to their
/// concrete focus handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusTarget {
    /// Keep focus inside the active modal even when it has no usable tab stop.
    ModalScope,
    /// Restore the exact control recorded before the modal opened.
    Restore(FocusRestoreToken),
    /// Clear focus when no safe restoration target was recorded.
    Clear,
}

/// Result of observing one toolkit traversal attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusCycleStep {
    /// Focus is inside the modal and traversal is complete.
    Settled,
    /// The adapter should perform another traversal attempt.
    Continue,
    /// The bounded scan was exhausted; apply this fail-closed target.
    Focus(FocusTarget),
}

/// Bounded state for one forward or reverse modal focus traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusCycle {
    remaining_attempts: usize,
}

impl FocusCycle {
    /// Observe whether the adapter's most recent traversal landed inside the
    /// modal scope.
    pub fn observe(&mut self, within_modal_scope: bool) -> FocusCycleStep {
        if within_modal_scope {
            return FocusCycleStep::Settled;
        }

        self.remaining_attempts = self.remaining_attempts.saturating_sub(1);
        if self.remaining_attempts == 0 {
            FocusCycleStep::Focus(FocusTarget::ModalScope)
        } else {
            FocusCycleStep::Continue
        }
    }
}

/// Shared modal-focus decisions independent of a UI toolkit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModalFocusPolicy {
    scan_limit: usize,
}

impl ModalFocusPolicy {
    /// Construct a containment policy. A zero limit is normalized to one so an
    /// adapter always makes one bounded traversal attempt before failing closed.
    #[must_use]
    pub const fn contained(scan_limit: usize) -> Self {
        Self {
            scan_limit: if scan_limit == 0 { 1 } else { scan_limit },
        }
    }

    #[must_use]
    pub const fn scan_limit(self) -> usize {
        self.scan_limit
    }

    /// Initial focus belongs to the modal scope, never the inert application.
    #[must_use]
    pub const fn initial_target(self) -> FocusTarget {
        FocusTarget::ModalScope
    }

    /// Closing restores an exact recorded target or clears focus when none was
    /// available.
    #[must_use]
    pub const fn restore_target(self, restore_token: Option<FocusRestoreToken>) -> FocusTarget {
        match restore_token {
            Some(token) => FocusTarget::Restore(token),
            None => FocusTarget::Clear,
        }
    }

    #[must_use]
    pub const fn begin_cycle(self) -> FocusCycle {
        FocusCycle {
            remaining_attempts: self.scan_limit,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/ui_focus.rs"]
mod tests;
