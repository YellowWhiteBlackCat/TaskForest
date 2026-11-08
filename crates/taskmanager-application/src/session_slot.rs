//! One optional application session per frontend.
//!
//! A missing client is an honest `Unavailable`, not a fabricated session. The
//! slot owns that distinction once: frontends install a real client and bind
//! `active_mut` to their own toolkit event loop, without re-declaring the same
//! enum or its close/absent behavior in every product crate.

#[derive(Debug)]
pub struct SessionSlot<S>(Option<S>);

impl<S> Default for SessionSlot<S> {
    fn default() -> Self {
        Self(None)
    }
}

/// A session that can retract its in-flight request without dropping the
/// installed port. Frontends call [`SessionSlot::close`] rather than
/// reimplementing `Option::as_mut().map(SessionClose::close)` per product.
pub trait SessionClose {
    fn close_session(&mut self);
}

impl<S> SessionSlot<S> {
    /// Install the one active session for this frontend. A second install
    /// replaces the prior port; it never creates a second parallel owner.
    pub fn install_with(&mut self, session: impl FnOnce() -> S) {
        self.0 = Some(session());
    }

    /// The active session, or `None` when no client was installed.
    pub fn active_mut(&mut self) -> Option<&mut S> {
        self.0.as_mut()
    }

    /// Whether a session has been installed.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.0.is_some()
    }
}

impl<S: SessionClose> SessionSlot<S> {
    /// Close the installed session's in-flight request, if one exists.
    pub fn close(&mut self) {
        if let Some(session) = self.0.as_mut() {
            session.close_session();
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/application_session_slot_tests.rs"]
mod tests;
