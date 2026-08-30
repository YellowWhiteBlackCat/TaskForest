//! macOS native-authorization transport seam for foreign-process control.
//!
//! This is the macOS counterpart of the Windows [`crate::uac`] seam: the
//! typed transport-fact vocabulary, the pure fact→outcome mapping, the
//! install-fact readiness probe, and the injectable transport trait. As with
//! UAC, the request/outcome vocabulary keeps its existing authority
//! ([`crate::polkit::ForeignProcessControlTarget`] PID + frozen creation
//! token, [`crate::polkit::ForeignProcessControlOperation`],
//! [`crate::polkit::ForeignProcessControlOutcome`]) and reply payloads are
//! parsed by the SAME helper-contract reader as the Linux stdout crossing.
//!
//! ## Why the Security-framework crossing is honestly unwired
//!
//! The only non-deprecated privileged-execution seam macOS offers is a
//! signed, installed privileged helper (SMJobBless / SMAppService launchd
//! daemon) — a packaging + code-signing trust decision this repository has
//! not taken (permission-model "新能力退出门"). The legacy
//! `AuthorizationExecuteWithPrivileges` is deprecated by Apple precisely
//! because it executes arbitrary commands as root, and driving
//! `osascript ... with administrator privileges` would be a command-
//! interpreter path — forbidden by the same rule that bans PowerShell on
//! Windows. Until an ADR freezes the signed-helper lane, the production
//! entry here fails closed as typed `Unsupported`: it never spawns a normal
//! child while claiming elevation.
//!
//! ## Honesty (headless)
//!
//! Everything in this module is pure data mapping; the fixture tests run on
//! any host. Real consent and helper behavior are on-box receipt territory
//! and are never fabricated headless.

#![forbid(unsafe_code)]

use crate::polkit::process_control::{ParsedOutput, parse_output};
use crate::polkit::{
    ForeignProcessControlOperation, ForeignProcessControlOutcome, ForeignProcessControlTarget,
};
use crate::{EscalationAvailability, EscalationDenialReason, EscalationFeature};

/// `errAuthorizationCanceled` (Security framework): the user dismissed the
/// authorization dialog.
const OSSTATUS_AUTHORIZATION_CANCELED: i32 = -60006;
/// `errAuthorizationDenied`: the authorization was explicitly refused (by the
/// user or by policy).
const OSSTATUS_AUTHORIZATION_DENIED: i32 = -60007;
/// `errAuthorizationNotAvailable`: no authorization broker could be reached.
const OSSTATUS_AUTHORIZATION_NOT_AVAILABLE: i32 = -60022;

// ===========================================================================
// Readiness probe: install facts only — never a consent prompt.
// ===========================================================================

/// The install facts the macOS readiness gate consumes for the (future)
/// signed privileged-helper lane.
///
/// Mirrors [`crate::uac::UacHelperInstallFacts`]: the probe is install-facts
/// only so it structurally cannot raise an authorization dialog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MacHelperInstallFacts {
    /// The privileged helper daemon is installed (SMJobBless/SMAppService).
    pub helper_installed: bool,
    /// The installed helper's signing/label registration matches the
    /// packaging decision.
    pub registration_consistent: bool,
}

/// Map macOS install facts to the readiness answer for foreign-process
/// control — the counterpart of [`crate::uac::probe_foreign_process_control_install`].
///
/// Installed → [`EscalationAvailability::RequiresEscalation`]; any missing
/// piece → [`EscalationDenialReason::HelperUnavailable`] (the transport lane
/// exists; only the install is missing — never `Unsupported`, which would
/// hide the install fix behind a permanent refusal).
#[must_use]
pub const fn probe_foreign_process_control_install(
    facts: MacHelperInstallFacts,
) -> EscalationAvailability {
    if facts.helper_installed && facts.registration_consistent {
        EscalationAvailability::RequiresEscalation(EscalationFeature::ForeignProcessControl)
    } else {
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        }
    }
}

// ===========================================================================
// Transport facts: what one authorization crossing observed.
// ===========================================================================

/// What one macOS authorization crossing attempt observed.
///
/// The failure facts stay mutually exclusive with each other and with the
/// helper's own contract failures: a user refusal is never folded into a
/// broker failure, a missing daemon is never a protocol violation, and the
/// unwired transport is never a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacAuthorizationObservation {
    /// The transport is not wired (current state: no signed-helper ADR) —
    /// the honest fail-closed fact.
    TransportUnwired,
    /// The authorization dialog could not be shown (no GUI session) or the
    /// request could not be attributed to the user.
    ConsentUnavailable,
    /// The Security framework returned an authorization OSStatus; classified
    /// by [`authorization_failure_reason`].
    AuthorizationFailed {
        /// The raw OSStatus code from the Security framework call.
        osstatus: i32,
    },
    /// The signed privileged helper daemon is not installed or not
    /// registered with launchd.
    HelperNotInstalled,
    /// The bounded wait ended while the authorization/helper was still
    /// outstanding — not a user refusal.
    DeadlineExceeded,
    /// The helper completed and the reply channel delivered `payload`
    /// (possibly empty — an empty or non-contract payload is a protocol
    /// violation, never success).
    HelperReply {
        /// The raw reply bytes.
        payload: Vec<u8>,
    },
}

/// Classify a Security-framework authorization failure by its OSStatus.
///
/// `errAuthorizationCanceled` and `errAuthorizationDenied` are the user's
/// explicit refusal → retryable [`EscalationDenialReason::PermissionDenied`]
/// (the pkexec-126 / UAC `ERROR_CANCELLED` counterparts). Every other code —
/// including `errAuthorizationNotAvailable` — is the neutral
/// [`EscalationDenialReason::AuthorizationUnavailable`]: the authorization
/// did not complete and the fact does not invent a cause (the pkexec-127
/// discipline).
#[must_use]
pub const fn authorization_failure_reason(osstatus: i32) -> EscalationDenialReason {
    match osstatus {
        OSSTATUS_AUTHORIZATION_CANCELED | OSSTATUS_AUTHORIZATION_DENIED => {
            EscalationDenialReason::PermissionDenied
        }
        // `errAuthorizationNotAvailable`: the broker was unreachable — the
        // neutral "authorization did not complete" answer.
        OSSTATUS_AUTHORIZATION_NOT_AVAILABLE => EscalationDenialReason::AuthorizationUnavailable,
        // Every other unattributable code shares that neutral bucket: the
        // fact does not invent a user refusal or a service fault.
        _ => EscalationDenialReason::AuthorizationUnavailable,
    }
}

/// Map one observed transport fact to the typed escalation outcome.
///
/// Reply payloads are parsed by the shared helper-contract reader
/// (schema==1, `applied`/`error`); anything else — including an empty reply —
/// is [`EscalationDenialReason::HelperProtocolViolation`], never a fabricated
/// [`ForeignProcessControlOutcome::Applied`].
#[must_use]
pub fn mac_authorization_outcome(
    observation: MacAuthorizationObservation,
) -> ForeignProcessControlOutcome {
    match observation {
        MacAuthorizationObservation::TransportUnwired => {
            ForeignProcessControlOutcome::Unavailable {
                reason: EscalationDenialReason::Unsupported,
                detail: "the macOS Security-framework authorization transport is not wired \
                         (no signed-helper decision)"
                    .to_owned(),
            }
        }
        MacAuthorizationObservation::ConsentUnavailable => {
            ForeignProcessControlOutcome::Unavailable {
                reason: EscalationDenialReason::AuthorizationUnavailable,
                detail: "no GUI session could show the authorization dialog; authorization did \
                         not complete"
                    .to_owned(),
            }
        }
        MacAuthorizationObservation::AuthorizationFailed { osstatus } => {
            ForeignProcessControlOutcome::Unavailable {
                reason: authorization_failure_reason(osstatus),
                detail: format!("the authorization call failed with OSStatus {osstatus}"),
            }
        }
        MacAuthorizationObservation::HelperNotInstalled => {
            ForeignProcessControlOutcome::Unavailable {
                reason: EscalationDenialReason::HelperUnavailable,
                detail: "the signed privileged helper is not installed or registered".to_owned(),
            }
        }
        MacAuthorizationObservation::DeadlineExceeded => {
            ForeignProcessControlOutcome::Unavailable {
                reason: EscalationDenialReason::HelperUnavailable,
                detail:
                    "the authorization crossing was abandoned at its deadline; no contract was \
                     delivered"
                        .to_owned(),
            }
        }
        MacAuthorizationObservation::HelperReply { payload } => {
            let reply = String::from_utf8_lossy(&payload);
            match parse_output(&reply) {
                ParsedOutput::Applied => ForeignProcessControlOutcome::Applied,
                ParsedOutput::Failed { kind, detail } => {
                    ForeignProcessControlOutcome::Failed { kind, detail }
                }
                ParsedOutput::NotContract => ForeignProcessControlOutcome::Unavailable {
                    reason: EscalationDenialReason::HelperProtocolViolation,
                    detail: format!(
                        "the macOS helper reply was not the contract: {}",
                        crate::polkit::truncate_for_detail(&reply)
                    ),
                },
            }
        }
    }
}

// ===========================================================================
// Transport seam.
// ===========================================================================

/// Injectable seam for one macOS authorization foreign-process-control
/// crossing.
///
/// Production implements this with the Security-framework call group behind a
/// future signed-helper ADR; tests implement it with canned
/// [`MacAuthorizationObservation`]s so no real dialog, daemon, or reply
/// channel is ever touched headless. The request carries PID + frozen
/// creation token ([`ForeignProcessControlTarget`]) — PID alone is never an
/// authorization credential.
pub trait MacAuthorizationForeignProcessControlTransport {
    /// Observe one crossing for `target` / `operation`.
    fn cross(
        &self,
        target: ForeignProcessControlTarget,
        operation: &ForeignProcessControlOperation,
    ) -> MacAuthorizationObservation;
}

/// Drive one crossing through `transport` and map the observed fact to the
/// typed escalation outcome.
///
/// `P` is `?Sized` so a runtime object-safe lane (a boxed transport held by
/// provider wiring) can drive this exact mapping instead of mirroring it.
pub fn invoke_mac_foreign_process_control_with<
    P: MacAuthorizationForeignProcessControlTransport + ?Sized,
>(
    transport: &P,
    target: ForeignProcessControlTarget,
    operation: ForeignProcessControlOperation,
) -> ForeignProcessControlOutcome {
    mac_authorization_outcome(transport.cross(target, &operation))
}

/// Invoke the production macOS authorization crossing for one
/// foreign-process operation.
///
/// The Security-framework call group is not wired (see the module docs: the
/// signed privileged-helper lane needs its own ADR), so the honest answer is
/// typed `Unsupported` — this entry never spawns a child process with the
/// application's inherited token, which would not be an escalation.
pub fn invoke_mac_foreign_process_control(
    _target: ForeignProcessControlTarget,
    _operation: ForeignProcessControlOperation,
) -> ForeignProcessControlOutcome {
    ForeignProcessControlOutcome::Unavailable {
        reason: EscalationDenialReason::Unsupported,
        detail: "the macOS Security-framework authorization transport is not wired (no \
                 signed-helper decision)"
            .to_owned(),
    }
}

#[cfg(test)]
#[path = "../tests/headless/escalation_authorization.rs"]
mod tests;
