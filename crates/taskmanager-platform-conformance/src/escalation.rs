//! Capability-availability invariants for permission-family failures.
//!
//! `platform-contract` publishes [`CapabilityStatus::PermissionRequired`] and
//! [`CapabilityStatus::RequiresEscalation`] as distinct capability states. This
//! module turns that distinction into a host-neutral assertion over synthetic
//! input, so an adapter cannot silently fold "the per-feature escalation seam
//! can still reach this data" onto "permission required, no offer".
//!
//! The projection under test is owned once by the contract crate as
//! [`ProviderFailure::capability_status`], which both the runtime catalog and
//! this module consume. The assertion therefore pins the shared rule instead of
//! reproducing it, so a drift in either consumer fails here.

use taskmanager_core::FailureKind;
use taskmanager_platform_contract::{CapabilityStatus, ProviderFailure};

/// Whether a provider failure proves the per-feature escalation seam (ADR-023,
/// permission-model Boundary 2) can still reach the data.
///
/// Only [`ProviderFailure::RequiresEscalation`] carries that proof. A hard
/// [`ProviderFailure::PermissionDenied`] is a solved denial with no offer, so
/// the two must stay distinguishable rather than collapse onto one word.
#[must_use]
pub const fn admits_escalation(failure: ProviderFailure) -> bool {
    matches!(failure, ProviderFailure::RequiresEscalation)
}

/// The capability status the runtime catalog must publish for one provider
/// failure.
///
/// This is a namespaced re-entry to the contract authority
/// [`ProviderFailure::capability_status`], not a second copy of the rule:
/// `platform-conformance` deliberately does not depend on the runtime crate
/// (that would invert the adapter→conformance direction the workspace keeps),
/// so it reuses the contract projection the runtime also delegates to. The
/// contract function is the sole authority; if the projection changes, this
/// wrapper and the headless contract test follow it automatically.
#[must_use]
pub const fn projected_capability_status(failure: ProviderFailure) -> CapabilityStatus {
    failure.capability_status()
}

/// Assert that the capability status published for `failure` matches the
/// contract projection, spelling out the permission/escalation distinction.
///
/// The invariant under test is asymmetric on purpose: a failure that
/// [`admits_escalation`] must publish [`CapabilityStatus::RequiresEscalation`]
/// and never [`CapabilityStatus::PermissionRequired`]; a hard
/// [`ProviderFailure::PermissionDenied`] must publish
/// [`CapabilityStatus::PermissionRequired`] and never
/// [`CapabilityStatus::RequiresEscalation`]; a transient failure with no
/// escalation offer must publish [`CapabilityStatus::TemporarilyUnavailable`].
pub fn assert_capability_failure_status(
    failure: ProviderFailure,
    observed: CapabilityStatus,
) -> Result<(), String> {
    let expected = projected_capability_status(failure);
    if observed == expected {
        return Ok(());
    }
    let reason = match failure.kind() {
        FailureKind::RequiresEscalation => {
            "escalatable denial must publish RequiresEscalation, never PermissionRequired"
        }
        FailureKind::PermissionDenied => {
            "hard denial must publish PermissionRequired, never RequiresEscalation"
        }
        FailureKind::TimedOut | FailureKind::TemporarilyUnavailable | FailureKind::Rejected => {
            "transient failure with no escalation offer must publish TemporarilyUnavailable"
        }
        _ => "published status differs from the contract projection",
    };
    Err(format!(
        "{failure:?}: {reason} (expected {expected:?}, observed {observed:?})"
    ))
}

#[cfg(test)]
#[path = "../tests/headless/escalation_contract.rs"]
mod tests;
