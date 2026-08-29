//! First-run dialog wiring: the composition lane around the
//! `ui::first_run` state machine (GPUI `RootView` first-run parity).
//!
//! The dialog's fold and renderer live in `ui::first_run`; this module owns
//! everything around them: the boot-time `SetupScriptAction::Observe`
//! submission, typed action submissions from the dialog's buttons, the
//! correlation of drained platform answers (`CorrelatedSetupScriptEvent` /
//! first-run `OperationFailure`s) into [`FirstRunEvent`]s, and the
//! application of the fold's visibility transitions to the Iced-owned
//! `LocalSurface::FirstRun` slot.
//!
//! Persistence stays exactly GPUI's contract: there is no "do not show
//! again" field anywhere — the platform-side setup script's absence is the
//! done-state, so a later observation honestly answers `None` and the fold
//! hides the dialog.

use std::collections::HashMap;

use taskmanager_application::{
    CorrelatedSetupScriptEvent, PlatformEffect, PlatformEventBatch, SetupScriptRequest,
    UrlOpenRequest,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::setup::{SetupScriptAction, SetupScriptEvent};
use taskmanager_platform_contract::{CapabilityId, RequestId, SubmissionErrorKind};

use taskmanager_shell::QuitReason;

use super::super::{FirstRunMessage, IcedApp, LocalSurface, LocalSurfaceKind, Message};
use super::dispatch::UpdateDispatch;
use crate::ui::first_run::{FirstRunEvent, FirstRunPhase, FirstRunTransition};

/// Correlate one drained platform batch into first-run fold events. The
/// pending-request map is the correlation authority: an answer whose request
/// id is not tracked belongs to another lane and is ignored (GPUI parity).
/// Matched ids are consumed exactly once, so an event and its failure can
/// never double-resolve.
pub(crate) fn extract_batch_events(
    batch: &PlatformEventBatch,
    pending: &mut HashMap<RequestId, SetupScriptAction>,
) -> Vec<FirstRunEvent> {
    let mut events = Vec::new();
    for correlated in &batch.setup_script_events {
        if let Some(action) = pending.remove(&correlated.request_id) {
            events.push(correlated_event(action, correlated));
        }
    }
    for failure in &batch.failures {
        if failure.capability != CapabilityId::FIRST_RUN_SETUP {
            continue;
        }
        if let Some(action) = pending.remove(&failure.request_id) {
            events.push(FirstRunEvent::ActionFailed {
                action,
                kind: failure.kind,
            });
        }
    }
    events
}

/// Fold one correlated answer against the request that produced it. A
/// mismatched completion (the answered action differs from the submitted
/// one) is the typed provider-fault case, never a silent success.
fn correlated_event(
    action: SetupScriptAction,
    correlated: &CorrelatedSetupScriptEvent,
) -> FirstRunEvent {
    match (action, &correlated.event) {
        (SetupScriptAction::Observe, SetupScriptEvent::Observed(info)) => {
            FirstRunEvent::ObservationCompleted(info.clone())
        }
        (submitted, SetupScriptEvent::ActionCompleted { action: completed })
            if submitted == *completed =>
        {
            FirstRunEvent::ActionCompleted(submitted)
        }
        (submitted, _) => FirstRunEvent::ActionFailed {
            action: submitted,
            kind: FailureKind::ProviderFault,
        },
    }
}

/// Map a submission rejection onto the dialog's typed failure vocabulary
/// (GPUI `submission_failure_kind` parity).
const fn submission_failure_kind(kind: SubmissionErrorKind) -> FailureKind {
    match kind {
        SubmissionErrorKind::UnsupportedCapability => FailureKind::Unsupported,
        SubmissionErrorKind::Busy | SubmissionErrorKind::RuntimeStopped => {
            FailureKind::TemporarilyUnavailable
        }
        SubmissionErrorKind::InvalidRequest => FailureKind::Rejected,
    }
}

/// Submission timestamp for the direct platform lane (the same contract the
/// shell's effect dispatcher uses; GPUI keeps an identical helper).
fn platform_submission_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(u64::MAX)
}

impl IcedApp {
    /// Submit the boot observation that decides the dialog's visibility. A
    /// missing platform (demo / no-I/O tests) folds the honest hidden state
    /// without reporting anything, exactly like GPUI's no-platform branch.
    pub(crate) fn begin_first_run_observation(&mut self) {
        match self.submit_first_run_action(SetupScriptAction::Observe) {
            Ok(()) => {}
            Err(kind) => {
                self.fold_first_run_events(vec![FirstRunEvent::ActionFailed {
                    action: SetupScriptAction::Observe,
                    kind: submission_failure_kind(kind),
                }]);
            }
        }
    }

    /// Submit one typed setup-script action through the platform client and
    /// track its request id for correlation. Mirrors GPUI's direct
    /// submission lane: the shell's feedback paths stay untouched, so a
    /// rejected submission folds into the dialog instead of the footer.
    fn submit_first_run_action(
        &mut self,
        action: SetupScriptAction,
    ) -> Result<(), SubmissionErrorKind> {
        let submission = self.runtime.platform_mut().map_or(
            Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_setup_script(
                        SetupScriptRequest { action },
                        platform_submission_time_ms(),
                    )
                    .map_err(|error| error.kind)
            },
        );
        match submission {
            Ok(request_id) => {
                self.first_run_requests.insert(request_id, action);
                Ok(())
            }
            Err(kind) => Err(kind),
        }
    }

    /// Reduce one dialog intent. `Close` is side-effect-free on the dialog
    /// state (the fold's `Dismissed` contract) and only closes the surface
    /// slot; `RequestAction` runs the GPUI request guards then submits;
    /// `OpenWiki` rides the existing typed URL-open effect port.
    pub(super) fn reduce_first_run_message(&mut self, message: Message) -> UpdateDispatch {
        let Message::FirstRun(intent) = message else {
            return UpdateDispatch::none();
        };
        let effect = match intent {
            FirstRunMessage::Close => {
                self.fold_first_run_events(vec![FirstRunEvent::Dismissed]);
                self.dismiss_first_run_slot();
                None
            }
            FirstRunMessage::RequestAction(action) => {
                self.apply_first_run_request(action);
                None
            }
            FirstRunMessage::OpenWiki => Some(PlatformEffect::OpenUrl(UrlOpenRequest {
                url: crate::ui::first_run::WIKI_URL.to_owned(),
            })),
        };
        UpdateDispatch::effect(effect)
    }

    /// Apply the dialog's typed button request. The guards mirror GPUI's
    /// `request_first_run_action`: Observe is never user-submitable, actions
    /// need an observed descriptor, and Restart needs the RestartRequired
    /// phase; rejected requests fold the typed failure instead of submitting.
    fn apply_first_run_request(&mut self, action: SetupScriptAction) {
        let rejected = if action == SetupScriptAction::Observe
            || (self.first_run.info.is_none() && action != SetupScriptAction::Restart)
        {
            Some(FailureKind::Unsupported)
        } else if action == SetupScriptAction::Restart
            && self.first_run.phase != FirstRunPhase::RestartRequired
        {
            Some(FailureKind::Rejected)
        } else {
            None
        };
        match rejected {
            Some(kind) => {
                self.fold_first_run_events(vec![FirstRunEvent::ActionFailed { action, kind }])
            }
            None => match self.submit_first_run_action(action) {
                Ok(()) => {
                    self.fold_first_run_events(vec![FirstRunEvent::ActionSubmitted(action)]);
                }
                Err(kind) => self.fold_first_run_events(vec![FirstRunEvent::ActionFailed {
                    action,
                    kind: submission_failure_kind(kind),
                }]),
            },
        }
    }

    /// Fold correlated events into the dialog state machine and apply the
    /// resulting visibility transitions to the surface slot. Called from the
    /// tick's platform-drain lane and from the intent reducer.
    pub(crate) fn fold_first_run_events(&mut self, events: Vec<FirstRunEvent>) {
        for event in events {
            if matches!(
                event,
                FirstRunEvent::ActionCompleted(SetupScriptAction::Restart)
            ) {
                // The platform has already relaunched the application (its
                // Restart spawns the replacement instance); this instance
                // records the quit so the shared finish system projects it
                // to the single window-close task, exactly like GPUI's
                // post-restart `app.quit()`. `Restart` names that lifecycle:
                // the replacement is running, so no failure or confirmation
                // feedback belongs to this instance's exit.
                self.shell.request_quit(QuitReason::Restart);
            }
            let transition = self.first_run.reduce(event);
            self.apply_first_run_transition(transition);
        }
    }

    /// Translate one dialog visibility transition onto the Iced-owned
    /// surface slot (opening closes competing primary surfaces through the
    /// existing local-surface contract).
    fn apply_first_run_transition(&mut self, transition: FirstRunTransition) {
        match transition {
            FirstRunTransition::Shown => self.open_local_surface(LocalSurface::FirstRun),
            FirstRunTransition::Hidden => self.dismiss_first_run_slot(),
            FirstRunTransition::Unchanged => {}
        }
    }

    fn dismiss_first_run_slot(&mut self) {
        self.dismiss_local_surface_kind(LocalSurfaceKind::FirstRun);
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/first_run_wiring_tests.rs"]
mod tests;
