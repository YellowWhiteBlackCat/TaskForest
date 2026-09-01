//! Identity-safe session and startup control seams for [`ShellApp`].

use super::ShellApp;
use taskmanager_application::{
    ConfirmationKind, PendingConfirmation, PlatformEffect, SessionControlConfirmation,
    SessionControlTarget, StartupControlRequest,
};
use taskmanager_core::core::session::{SessionControlAction, SessionItem};
use taskmanager_core::core::startup::StartupEntry;

impl ShellApp {
    /// Capture the selected provider-issued session identity and produce a
    /// renderer-neutral action. No frontend may derive a native target from a
    /// display name or invoke a session tool directly.
    #[must_use]
    pub fn request_session_control(
        &mut self,
        action: SessionControlAction,
    ) -> Option<PlatformEffect> {
        let session_id = self
            .data
            .sessions
            .as_deref()?
            .get(self.selected)?
            .id
            .clone();
        let request_id = self.data.session_control_requests.begin();
        // A new action supersedes the previous outcome's feedback.
        self.data.session_control_feedback = None;
        Some(PlatformEffect::SessionControl(SessionControlTarget {
            request_id,
            session_id,
            action,
        }))
    }

    /// Arm the shared session-control confirmation gate for an exact
    /// renderer-local row target (a session menu pick). The provider-issued
    /// identity, the action, and a fresh correlation id are frozen into
    /// [`Self::pending_session`]; the platform request is produced only by
    /// [`Self::confirm_session_control`]. Mirrors
    /// [`Self::request_startup_control_for`] (which also accepts an explicit
    /// row so frontends with sorted tables keep their visual cursor separate
    /// from the frozen target). Returns false when the row carries no
    /// provider-issued identity.
    pub fn select_session_control(
        &mut self,
        session: &SessionItem,
        action: SessionControlAction,
    ) -> bool {
        if session.id.as_str().is_empty() {
            return false;
        }
        let request_id = self.data.session_control_requests.begin();
        // A new action supersedes the previous outcome's feedback.
        self.data.session_control_feedback = None;
        self.arm_confirmation(PendingConfirmation::SessionControl(
            SessionControlConfirmation {
                request_id,
                session: session.clone(),
                action,
            },
        ));
        true
    }

    /// Confirm the pending session-control gate: emit it as a
    /// [`PlatformEffect::SessionControl`] built from the frozen target and
    /// clear the gate. Returns `None` when no session action is pending
    /// (mirrors [`Self::confirm_startup_control`]). A refresh between arm and
    /// confirm can never retarget the request — the frozen identity is what
    /// the confirmation displayed.
    #[must_use]
    pub fn confirm_session_control(&mut self) -> Option<PlatformEffect> {
        self.confirm_confirmation(ConfirmationKind::SessionControl)
    }

    /// Capture the selected startup entry and produce a renderer-neutral
    /// enable/disable action. The selected [`StartupEntry`] is frozen into the
    /// request so a later inventory refresh cannot silently change the target
    /// (mirrors [`Self::request_session_control`]). No frontend may invoke a
    /// startup tool directly. Like GPUI's `request_startup_control_confirmation`,
    /// every Enable/Disable is GATED behind an explicit confirmation: this sets
    /// [`Self::pending_startup`] and returns `None`; the frontend's confirm
    /// control calls [`Self::confirm_startup_control`] to actually emit.
    #[must_use]
    pub fn request_startup_control(&mut self, enabled: bool) -> Option<PlatformEffect> {
        let entry = self
            .data
            .startup_entries
            .as_deref()?
            .get(self.selected)?
            .clone();
        self.request_startup_control_for(entry, enabled)
    }

    /// Capture an exact provider-issued startup entry for a renderer-local
    /// row menu. The ordinary selected-row helper delegates here; frontends
    /// with sorted tables can therefore keep their visual cursor separate from
    /// the provider-order identity used by the control request.
    #[must_use]
    pub fn request_startup_control_for(
        &mut self,
        entry: StartupEntry,
        enabled: bool,
    ) -> Option<PlatformEffect> {
        let request_id = self.data.startup_control_requests.begin();
        self.arm_confirmation(PendingConfirmation::StartupControl(StartupControlRequest {
            request_id,
            entry,
            enabled,
        }));
        None
    }

    /// Confirm the pending startup-control request: emit it as a
    /// [`PlatformEffect::StartupControl`] and clear the gate. Returns `None`
    /// when no startup action is pending (mirrors [`Self::confirm_process_batch`]).
    #[must_use]
    pub fn confirm_startup_control(&mut self) -> Option<PlatformEffect> {
        self.confirm_confirmation(ConfirmationKind::StartupControl)
    }
}
