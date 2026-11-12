//! Typed availability for the frozen Users-row menu target.
//!
//! The shared shell already rejects a session-control request whose provider
//! identity is missing or no longer unique in the canonical inventory. This
//! frontend projection asks the same question before mounting or focusing the
//! menu's action buttons, so a disabled control is never an activation path.

use taskforest_core::core::session::{SessionIdentity, SessionItem};

/// Whether the frozen menu session can still be controlled.
///
/// The check uses the canonical projection, not the displayed row order, and
/// mirrors [`crate::app::IcedApp::arm_user_menu_session_control`]'s shared
/// `ShellApp::select_session_control` gate: a valid identity must appear
/// exactly once. A renderer refresh that duplicates or drops the identity
/// leaves the visible menu actions disabled.
#[must_use]
pub(crate) fn user_menu_actions_enabled(
    menu_session: Option<&SessionItem>,
    projection_sessions: Option<&[SessionItem]>,
) -> bool {
    let Some(identity) = menu_session.and_then(SessionItem::identity) else {
        return false;
    };
    session_identity_is_current_unique(projection_sessions.unwrap_or(&[]), &identity)
}

fn session_identity_is_current_unique(
    sessions: &[SessionItem],
    identity: &SessionIdentity,
) -> bool {
    let mut matches = sessions
        .iter()
        .filter(|session| session.identity().as_ref() == Some(identity));
    matches.next().is_some() && matches.next().is_none()
}
