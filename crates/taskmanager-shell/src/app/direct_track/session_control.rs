//! Session-control confirmation for the shell-owned direct track.

use taskmanager_application::{
    ConfirmationKind, InteractionEvent, PendingConfirmation, PlatformEffect, SurfaceDismissReason,
};
use taskmanager_core::core::session::{SessionIdentity, SessionItem};

use super::DirectTrackState;

impl DirectTrackState {
    /// Revalidate the frozen session against the canonical direct-track
    /// projection, then consume the shared interaction gate into one platform
    /// effect. Stale, missing, or duplicated identities dismiss without work.
    #[must_use]
    pub fn confirm_session_control(&mut self) -> Option<PlatformEffect> {
        let pending = match self.interaction.pending_confirmation() {
            Some(PendingConfirmation::SessionControl(pending)) => Some((
                pending.request_id,
                pending.session.identity(),
                pending.action,
            )),
            _ => None,
        };
        let (request_id, identity, action) = pending?;

        let identity_is_current = identity.as_ref().is_some_and(|identity| {
            self.projection
                .sessions
                .as_deref()
                .is_some_and(|sessions| session_identity_is_current_unique(sessions, identity))
        });
        if !identity_is_current {
            if let Some(identity) = identity.as_ref() {
                let _ = self.accept_session_control(request_id, identity, action);
            } else if let Some((pending_id, pending_identity, pending_action)) =
                self.projection.session_control_requests.pending()
                && pending_id == request_id
                && pending_action == action
            {
                let identity = pending_identity.clone();
                let _ = self.accept_session_control(pending_id, &identity, pending_action);
            }
            let _ = self.interaction.reduce(InteractionEvent::Dismiss(
                SurfaceDismissReason::TargetUnavailable,
            ));
            return None;
        }

        self.interaction
            .reduce(InteractionEvent::Confirm(ConfirmationKind::SessionControl))
            .effect
    }
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
