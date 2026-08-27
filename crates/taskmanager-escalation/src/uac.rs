//! Windows UAC transport seam for foreign-process control (ADR-035, stage 1).
//!
//! ADR-035 selected `ShellExecuteExW("runas")` + `SEE_MASK_NOCLOSEPROCESS` as
//! the Windows escalation crossing. Stage 1 (this module) lands the SEAM only:
//! the typed transport-fact vocabulary, the pure transport-fact → outcome
//! mapping, the install-fact readiness probe, and the injectable transport
//! trait with a `cfg(windows)` skeleton driver that honestly reports the
//! transport as unwired. Stage 2 — blocked on the owner's helper
//! signing/packaging decision — registers the runas call group in
//! `taskmanager-windows-api`, packages the helper, and wires the provider; the
//! production entry here then swaps the skeleton for the real driver without
//! changing this contract.
//!
//! ## No second vocabulary
//!
//! Per ADR-035 the request/outcome vocabulary keeps its existing authority:
//! [`crate::polkit::ForeignProcessControlTarget`] (PID + frozen creation
//! token), [`crate::polkit::ForeignProcessControlOperation`],
//! [`crate::polkit::ForeignProcessControlOutcome`], and
//! [`crate::EscalationDenialReason`]. This module adds only the TRANSPORT-FACT
//! layer underneath them, and reply payloads are parsed by the SAME
//! helper-contract reader as the Linux stdout crossing, so one authority
//! defines what a valid contract message is.
//!
//! ## Honesty (fixture-only in stage 1)
//!
//! No Windows runtime behavior is exercised or fabricated here: the mapping is
//! pure over [`uac::UacCrossingObservation`] values and every test drives fixtures
//! on any host. The real consent, deadline, and reply-channel behavior is
//! proven only by the stage-3 on-box receipts (never headless).
//!
//! ## Identity discipline
//!
//! The crossing request carries PID + creation token
//! ([`crate::polkit::ForeignProcessControlTarget`]; on Windows the token is
//! the `GetProcessTimes` 100ns creation time, the structural counterpart of
//! the Linux `/proc` start token). PID is never an authorization credential:
//! the elevated helper re-reads the kernel creation time on its own
//! `OpenProcess` handle and reports `identity_changed` when it does not match,
//! which maps to [`crate::polkit::ForeignProcessControlFailure::IdentityChanged`]
//! — a PID reuse can never masquerade as success.

#![forbid(unsafe_code)]

use crate::polkit::process_control::{ParsedOutput, parse_output};
use crate::polkit::{
    ForeignProcessControlOperation, ForeignProcessControlOutcome, ForeignProcessControlTarget,
};
use crate::{EscalationAvailability, EscalationDenialReason, EscalationFeature};

// ===========================================================================
// Readiness probe: install facts only — never a consent prompt.
// ===========================================================================

/// The packaging/install facts the Windows readiness gate consumes (ADR-035).
///
/// Stage 2 derives these from the real install tree (the packaged helper
/// binary plus its install-manifest row); until packaging exists they stay a
/// typed input so the probe semantics are fixture-provable on every host.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UacHelperInstallFacts {
    /// The packaged helper binary exists at its install location.
    pub helper_present: bool,
    /// The installed helper matches the install-manifest row.
    pub manifest_consistent: bool,
}

/// Map Windows install facts to the readiness answer for foreign-process
/// control — the symmetric counterpart of the Linux `PolkitGate` probe.
///
/// The probe reads ONLY install facts: its signature carries no transport, so
/// it structurally cannot raise a UAC consent or launch anything. Installed →
/// [`EscalationAvailability::RequiresEscalation`] (the prompt can be offered);
/// any missing piece → [`EscalationDenialReason::HelperUnavailable`] — the
/// honest "this host cannot offer the crossing" answer, deliberately NOT
/// `Unsupported` (the transport exists; only the install is missing).
#[must_use]
pub const fn probe_foreign_process_control_install(
    facts: UacHelperInstallFacts,
) -> EscalationAvailability {
    if facts.helper_present && facts.manifest_consistent {
        EscalationAvailability::RequiresEscalation(EscalationFeature::ForeignProcessControl)
    } else {
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        }
    }
}

// ===========================================================================
// Transport facts: what one UAC crossing observed.
// ===========================================================================

/// `ERROR_CANCELLED` (1223): the user answered "No" on the consent prompt.
const WIN32_ERROR_CANCELLED: u32 = 1223;
/// `ERROR_FILE_NOT_FOUND` (2): the helper executable is missing at its
/// packaged path — the "not installed" transport fact.
const WIN32_ERROR_FILE_NOT_FOUND: u32 = 2;

/// What one Windows UAC crossing attempt observed (ADR-035 transport facts).
///
/// This is the injectable seam between the OS transport and the typed
/// escalation vocabulary: stage 2's runas driver produces these from
/// `ShellExecuteExW`, the bounded wait, and the one-shot reply channel; tests
/// produce them directly. The failure facts stay mutually exclusive — a user
/// refusal is never folded into a deadline, a missing install is never a
/// protocol violation, and an unwired transport is never a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UacCrossingObservation {
    /// The transport is not wired (ADR-035 stage 1) — the honest current
    /// state, never a fabricated crossing.
    TransportUnwired,
    /// There is no interactive session to show the consent in (e.g. a Session
    /// 0 service context) or the prompt cannot be presented. Authorization
    /// did not complete and the fact does not attribute it to the user.
    ConsentUnavailable,
    /// `ShellExecuteExW("runas", …)` failed; `win32_error` is the raw
    /// `GetLastError` code, classified by the launch-failure table.
    LaunchFailed {
        /// The raw Win32 error code from the failed launch.
        win32_error: u32,
    },
    /// The bounded wait ended while the consent/helper was still outstanding —
    /// the crossing did not deliver, which is not a user refusal.
    DeadlineExceeded,
    /// The helper process completed and the reply channel delivered `payload`
    /// (possibly empty — an empty or non-contract payload is a protocol
    /// violation, never success).
    HelperReply {
        /// The raw reply-channel bytes.
        payload: Vec<u8>,
    },
}

/// Classify a failed `ShellExecuteExW("runas")` launch by its Win32 error.
///
/// `ERROR_CANCELLED` is the user's explicit "No" → retryable
/// [`EscalationDenialReason::PermissionDenied`] (the pkexec-126 counterpart).
/// `ERROR_FILE_NOT_FOUND` means the helper is not installed →
/// [`EscalationDenialReason::HelperUnavailable`]. Every other code means the
/// authorization did not complete for a reason that cannot be attributed to
/// the user → the neutral
/// [`EscalationDenialReason::AuthorizationUnavailable`] (the pkexec-127
/// discipline: never invent either a user refusal or an authorization-service
/// fault).
const fn launch_failure_reason(win32_error: u32) -> EscalationDenialReason {
    match win32_error {
        WIN32_ERROR_CANCELLED => EscalationDenialReason::PermissionDenied,
        WIN32_ERROR_FILE_NOT_FOUND => EscalationDenialReason::HelperUnavailable,
        _ => EscalationDenialReason::AuthorizationUnavailable,
    }
}

/// Map one observed transport fact to the typed escalation outcome
/// (ADR-035's transport-fact → outcome table).
///
/// Reply payloads are parsed by the shared helper-contract reader
/// (schema==1, `applied`/`error`); anything else — including an empty reply
/// from a helper that crashed — is
/// [`EscalationDenialReason::HelperProtocolViolation`], never a fabricated
/// [`ForeignProcessControlOutcome::Applied`].
#[must_use]
pub fn uac_crossing_outcome(observation: UacCrossingObservation) -> ForeignProcessControlOutcome {
    match observation {
        UacCrossingObservation::TransportUnwired => ForeignProcessControlOutcome::Unavailable {
            reason: EscalationDenialReason::Unsupported,
            detail: "the Windows UAC runas transport is not wired (ADR-035 stage 1)".to_owned(),
        },
        UacCrossingObservation::ConsentUnavailable => ForeignProcessControlOutcome::Unavailable {
            reason: EscalationDenialReason::AuthorizationUnavailable,
            detail: "no interactive session could show the UAC consent; authorization did not \
                     complete"
                .to_owned(),
        },
        UacCrossingObservation::LaunchFailed { win32_error } => {
            ForeignProcessControlOutcome::Unavailable {
                reason: launch_failure_reason(win32_error),
                detail: format!("ShellExecuteExW(runas) failed with Win32 error {win32_error}"),
            }
        }
        UacCrossingObservation::DeadlineExceeded => ForeignProcessControlOutcome::Unavailable {
            reason: EscalationDenialReason::HelperUnavailable,
            detail: "the UAC crossing was abandoned at its deadline; no contract was delivered"
                .to_owned(),
        },
        UacCrossingObservation::HelperReply { payload } => {
            let reply = String::from_utf8_lossy(&payload);
            match parse_output(&reply) {
                ParsedOutput::Applied => ForeignProcessControlOutcome::Applied,
                ParsedOutput::Failed { kind, detail } => {
                    ForeignProcessControlOutcome::Failed { kind, detail }
                }
                ParsedOutput::NotContract => ForeignProcessControlOutcome::Unavailable {
                    reason: EscalationDenialReason::HelperProtocolViolation,
                    detail: format!(
                        "the UAC helper reply was not the contract: {}",
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

/// Injectable seam for one UAC foreign-process-control crossing.
///
/// Production (ADR-035 stage 2) implements this with the runas call group:
/// `ShellExecuteExW("runas")` + `SEE_MASK_NOCLOSEPROCESS`, a bounded wait, and
/// the one-shot reply channel. Tests implement it with canned
/// [`UacCrossingObservation`]s, so no real consent, child process, or channel
/// is ever touched headless. The request carries PID + frozen creation token
/// ([`ForeignProcessControlTarget`]) — PID alone is never an authorization
/// credential.
pub trait UacForeignProcessControlTransport {
    /// Observe one crossing for `target` / `operation`.
    fn cross(
        &self,
        target: ForeignProcessControlTarget,
        operation: &ForeignProcessControlOperation,
    ) -> UacCrossingObservation;
}

/// Drive one crossing through `transport` and map the observed fact to the
/// typed escalation outcome.
///
/// `P` is `?Sized` so a runtime object-safe lane (a boxed transport held by
/// the stage-2 provider wiring) can drive this exact mapping instead of
/// mirroring it.
pub fn invoke_uac_foreign_process_control_with<P: UacForeignProcessControlTransport + ?Sized>(
    transport: &P,
    target: ForeignProcessControlTarget,
    operation: ForeignProcessControlOperation,
) -> ForeignProcessControlOutcome {
    uac_crossing_outcome(transport.cross(target, &operation))
}

/// Stage-1 skeleton of the production runas transport (ADR-035).
///
/// The real driver — `ShellExecuteExW("runas")` + `SEE_MASK_NOCLOSEPROCESS` +
/// a bounded wait + the one-shot reply channel, registered in
/// `taskmanager-windows-api` — lands in stage 2 behind the owner's
/// signing/packaging decision. Until then the only honest observation this
/// driver can make is [`UacCrossingObservation::TransportUnwired`]: it never
/// spawns the helper with the application's inherited token (that would not
/// be an escalation) and never fabricates a crossing.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RunasForeignProcessControl;

#[cfg(windows)]
impl RunasForeignProcessControl {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(windows)]
impl UacForeignProcessControlTransport for RunasForeignProcessControl {
    fn cross(
        &self,
        _target: ForeignProcessControlTarget,
        _operation: &ForeignProcessControlOperation,
    ) -> UacCrossingObservation {
        // Stage 1: the runas call group is not registered in the audited
        // boundary yet, so fail closed — the typed Unwired fact maps to
        // `Unsupported` downstream.
        UacCrossingObservation::TransportUnwired
    }
}

/// Invoke the production UAC crossing for one foreign-process operation.
///
/// Stage-1 Windows arm: the skeleton driver honestly reports the transport as
/// unwired → typed `Unsupported`; stage 2 swaps in the real driver without
/// changing this entry's contract. Non-Windows arm: the UAC transport can
/// never exist there → typed `Unsupported` without touching anything.
#[cfg(windows)]
pub fn invoke_uac_foreign_process_control(
    target: ForeignProcessControlTarget,
    operation: ForeignProcessControlOperation,
) -> ForeignProcessControlOutcome {
    invoke_uac_foreign_process_control_with(&RunasForeignProcessControl::new(), target, operation)
}

#[cfg(not(windows))]
pub fn invoke_uac_foreign_process_control(
    _target: ForeignProcessControlTarget,
    _operation: ForeignProcessControlOperation,
) -> ForeignProcessControlOutcome {
    ForeignProcessControlOutcome::Unavailable {
        reason: EscalationDenialReason::Unsupported,
        detail: "the UAC runas escalation transport is Windows-only".to_owned(),
    }
}

#[cfg(test)]
#[path = "../tests/headless/escalation_uac.rs"]
mod tests;
