//! Windows UAC transport seam for foreign-process control (ADR-035).
//!
//! ADR-035 selected `ShellExecuteExW("runas")` + `SEE_MASK_NOCLOSEPROCESS` as
//! the Windows escalation crossing. Stage 1 landed the SEAM (typed
//! transport-fact vocabulary, pure fact→outcome mapping, install-fact
//! readiness probe, injectable transport trait). Stage 2 (this module's
//! current state) adds the pure launch layer the real driver consumes: the
//! helper command-line builder and the one-shot reply-channel naming rule.
//! The OS call group itself lives in the audited `taskmanager-windows-api`
//! boundary (`runas` module) and the production driver lives in
//! `taskmanager-platform-windows` (`provider::process::uac`) — THIS crate is
//! documented zero-dependency and `#![forbid(unsafe_code)]`, so the
//! dependency direction forces the raw `ShellExecuteExW` /
//! `WaitForSingleObject` / `GetExitCodeProcess` / `CloseHandle` calls outside
//! it. The platform driver implements
//! [`crate::uac::UacForeignProcessControlTransport`] and feeds
//! [`crate::uac::invoke_uac_foreign_process_control_with`], so the fact→outcome
//! mapping here stays the single authority.
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
//! ## Honesty (headless)
//!
//! No Windows runtime behavior is exercised or fabricated here: the mappings
//! are pure over [`crate::uac::UacCrossingObservation`] values and every test drives
//! fixtures on any host. The real consent, deadline, and reply-channel
//! behavior is proven only by on-box receipts (never headless); until those
//! land the wiring is compile-verified only (see `docs/PERMISSION_MODEL.md`).
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

use std::path::Path;

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
/// The platform driver derives these from the real install tree (the packaged
/// helper binary plus its install-manifest row); where no packaging decision
/// exists yet they stay a typed input so the probe semantics are
/// fixture-provable on every host.
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
/// escalation vocabulary: the stage-2 runas driver in
/// `taskmanager-platform-windows` produces these from `ShellExecuteExW`, the
/// bounded wait, and the one-shot reply channel; tests produce them directly.
/// The failure facts stay mutually exclusive — a user refusal is never folded
/// into a deadline, a missing install is never a protocol violation, and an
/// unwired transport is never a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UacCrossingObservation {
    /// The transport is not wired for this composition — the honest
    /// fail-closed fact, kept so an adapter set that never registered the
    /// runas driver still cannot fabricate a crossing.
    TransportUnwired,
    /// There is no interactive session to show the consent in (e.g. a Session
    /// 0 service context) or the prompt cannot be presented. Authorization
    /// did not complete and the fact does not attribute it to the user.
    ConsentUnavailable,
    /// `ShellExecuteExW("runas", …)` failed; `win32_error` is the raw
    /// `GetLastError` code (or the non-win32 HRESULT bits when Windows
    /// reports a plain COM failure), classified by the launch-failure table.
    LaunchFailed {
        /// The raw Win32 error code from the failed launch.
        win32_error: u32,
    },
    /// The one-shot reply channel could not be established before any launch
    /// was attempted (temp directory unavailable, name collisions exhausted).
    /// The crossing infrastructure is unusable — distinct from a missing
    /// helper install and from a launch the OS refused.
    ReplyChannelUnavailable,
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
            detail: "the Windows UAC runas transport is not wired for this composition".to_owned(),
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
        UacCrossingObservation::ReplyChannelUnavailable => {
            ForeignProcessControlOutcome::Unavailable {
                reason: EscalationDenialReason::HelperUnavailable,
                detail: "the one-shot UAC reply channel could not be created; the crossing was \
                         never launched"
                    .to_owned(),
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
// Stage-2 pure launch layer: helper command line + one-shot reply channel.
// ===========================================================================

/// The reply-channel file name for one crossing (ADR-035 decision 4: per-call
/// and randomly named; the channel's access restriction rides the per-user
/// temp directory ACL plus the app's exclusive creation).
///
/// Pure over the caller-supplied `nonce`; the driver sources nonce entropy
/// from a process-addressed `RandomState` hash, so two crossings in one
/// process never share a channel and no randomness dependency is added.
#[must_use]
pub fn reply_channel_file_name(nonce: u64) -> String {
    format!("taskforest-uac-reply-{nonce:016x}.json")
}

/// Quote one argument for the `lpParameters` command line of
/// `ShellExecuteExW`.
///
/// Applies the documented Windows command-line rule: an argument with no
/// whitespace and no quote passes through verbatim; otherwise it is wrapped
/// in quotes, every embedded quote is escaped as `\"`, and every backslash
/// run before a quote or the closing quote is doubled. Pure, so the builder
/// is provable on any host.
#[must_use]
pub fn quote_windows_argument(argument: &str) -> String {
    let needs_quoting =
        argument.is_empty() || argument.chars().any(|c| c.is_whitespace() || c == '"');
    if !needs_quoting {
        return argument.to_owned();
    }
    let mut quoted = String::with_capacity(argument.len() + 2);
    quoted.push('"');
    let mut backslash_run = 0usize;
    for character in argument.chars() {
        match character {
            '\\' => backslash_run += 1,
            '"' => {
                for _ in 0..(2 * backslash_run + 1) {
                    quoted.push('\\');
                }
                quoted.push('"');
                backslash_run = 0;
            }
            _ => {
                for _ in 0..backslash_run {
                    quoted.push('\\');
                }
                backslash_run = 0;
                quoted.push(character);
            }
        }
    }
    for _ in 0..(2 * backslash_run) {
        quoted.push('\\');
    }
    quoted.push('"');
    quoted
}

/// The fixed `lpParameters` string for one elevated helper launch.
///
/// Argument order is the helper's fixed contract — PID, start token,
/// operation wire form, one-shot reply-channel path — so the PID and the
/// frozen creation token cross together and the PID alone is never an
/// authorization credential (the helper re-validates the token on its own
/// elevated handle). Pure over its inputs.
#[must_use]
pub fn runas_command_line(
    target: ForeignProcessControlTarget,
    operation: &ForeignProcessControlOperation,
    reply_channel: &Path,
) -> String {
    let arguments = [
        target.pid().to_string(),
        target.start_token().to_string(),
        operation.argument(),
        reply_channel.to_string_lossy().into_owned(),
    ];
    arguments
        .iter()
        .map(|argument| quote_windows_argument(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

// ===========================================================================
// Transport seam.
// ===========================================================================

/// Injectable seam for one UAC foreign-process-control crossing.
///
/// The production driver (ADR-035 stage 2) lives in
/// `taskmanager-platform-windows`: it builds the launch through
/// [`runas_command_line`], drives the audited `runas` call group in
/// `taskmanager-windows-api` (`ShellExecuteExW("runas")` +
/// `SEE_MASK_NOCLOSEPROCESS`, bounded wait, one-shot reply channel), and
/// returns the observed [`UacCrossingObservation`]. Tests implement the trait
/// with canned observations, so no real consent, child process, or channel is
/// ever touched headless. The request carries PID + frozen creation token
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
/// the provider wiring) can drive this exact mapping instead of mirroring it.
pub fn invoke_uac_foreign_process_control_with<P: UacForeignProcessControlTransport + ?Sized>(
    transport: &P,
    target: ForeignProcessControlTarget,
    operation: ForeignProcessControlOperation,
) -> ForeignProcessControlOutcome {
    uac_crossing_outcome(transport.cross(target, &operation))
}

#[cfg(test)]
#[path = "../tests/headless/escalation_uac.rs"]
mod tests;
