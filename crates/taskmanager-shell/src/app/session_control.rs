//! Identity-safe session and startup control seams for [`ShellApp`].

use super::ShellApp;
use crate::presentation::smart::self_test_supported;
use taskmanager_application::{
    ConfirmationKind, InteractionEvent, PendingConfirmation, PlatformEffect,
    SessionControlConfirmation, StartupControlRequest, SurfaceDismissReason,
};
use taskmanager_core::core::session::{SessionControlAction, SessionIdentity, SessionItem};
use taskmanager_core::core::startup::StartupEntry;
use taskmanager_core::core::system_health::SmartSelfTestIntent;

impl ShellApp {
    /// Arm the shared session-control confirmation gate for an exact
    /// renderer-local row target (a session menu pick). The provider-issued
    /// identity, the action, and a fresh correlation id are frozen into
    /// [`Self::pending_session`]; the platform request is produced only by
    /// [`Self::confirm_session_control`]. Mirrors
    /// [`Self::request_startup_control_for`] (which also accepts an explicit
    /// row so frontends with sorted tables keep their visual cursor separate
    /// from the frozen target). Returns false when the row carries no
    /// provider-issued identity or when that identity is missing, duplicated,
    /// or no longer present in the current projection.
    pub fn select_session_control(
        &mut self,
        session: &SessionItem,
        action: SessionControlAction,
    ) -> bool {
        let Some(identity) = session.identity() else {
            return false;
        };
        if !self.session_identity_is_current_unique(&identity) {
            return false;
        }
        let request_id = self.data.session_control_requests.begin(identity, action);
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
    /// confirm must still contain exactly one copy of the frozen identity;
    /// otherwise the gate is dismissed without submitting.
    #[must_use]
    pub fn confirm_session_control(&mut self) -> Option<PlatformEffect> {
        let pending = self.pending_session().map(|pending| {
            (
                pending.request_id,
                pending.session.identity(),
                pending.action,
            )
        });
        let identity = pending
            .as_ref()
            .and_then(|(_, identity, _)| identity.as_ref());
        if !identity.is_some_and(|identity| self.session_identity_is_current_unique(identity)) {
            if self.confirmation_kind() == Some(ConfirmationKind::SessionControl) {
                if let Some((request_id, Some(identity), action)) = pending {
                    self.data
                        .session_control_requests
                        .accept(request_id, &identity, action);
                }
                let reduction = self
                    .application
                    .interaction
                    .reduce(InteractionEvent::Dismiss(
                        SurfaceDismissReason::TargetUnavailable,
                    ));
                self.apply_surface_transition(reduction.transition);
            }
            return None;
        }
        self.confirm_confirmation(ConfirmationKind::SessionControl)
    }

    fn session_identity_is_current_unique(&self, identity: &SessionIdentity) -> bool {
        let Some(sessions) = self.data.sessions.as_deref() else {
            return false;
        };
        let mut matches = sessions
            .iter()
            .filter(|session| session.identity().as_ref() == Some(identity));
        matches.next().is_some() && matches.next().is_none()
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

    /// Arm the shared SMART self-test confirmation gate with an explicit,
    /// still-current device intent. A stale generation or unsupported kind is
    /// rejected before confirmation can reach the platform adapter.
    pub fn arm_smart_self_test(&mut self, intent: SmartSelfTestIntent) -> bool {
        let supported = self.projection().snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .disks
                .iter()
                .filter(|disk| {
                    disk.device_id == intent.device_id.as_str()
                        && disk.device_generation == intent.device_generation
                        && disk.name == intent.device_key.as_str()
                        && intent.device_generation.is_valid()
                        && self_test_supported(disk, intent.kind)
                })
                .count()
                == 1
        });
        if !supported {
            return false;
        }
        self.arm_confirmation(PendingConfirmation::SmartSelfTest(intent));
        true
    }

    /// Confirm the pending SMART self-test: emit it as a
    /// [`PlatformEffect::SmartControl`] and clear the gate.
    #[must_use]
    pub fn confirm_smart_self_test(&mut self) -> Option<PlatformEffect> {
        self.confirm_confirmation(ConfirmationKind::SmartSelfTest)
    }
}
