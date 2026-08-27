//! Intel PMU per-feature escalation tie-in (ADR-023, permission-model
//! Boundary 2).
//!
//! [`classify_intel_pmu_open_failure`] wraps the shared IO classifier so a
//! `perf_event_open` `EACCES` (restrictive `perf_event_paranoid`, e.g. this
//! dev's CachyOS at paranoid = 2) types as [`FailureKind::RequiresEscalation`]
//! rather than a bare `PermissionDenied`: the Intel i915/xe PMU denial is
//! reachable through the OS-native per-feature prompt, so a consumer can offer
//! that one escalation instead of treating the gap as a hard wall.
//!
//! Honesty red line: the PMU still yields NO numbers when denied; only the
//! typing becomes escalation-aware. `UnprivilegedGate` is the honest default
//! (the app runs unprivileged and has escalated nothing yet), so under the
//! default gate a denial always escalates; wiring a real gate that talks to the
//! privileged helper is a follow-up tracked in ADR-023, not this seam.

use std::io;

use taskmanager_core::FailureKind;
use taskmanager_escalation::{
    EscalationAvailability, EscalationFeature, PrivilegeGate, UnprivilegedGate,
};

use super::super::super::gpu_io_failure;

/// Classify a `GpuEngineCounter::open` failure for the Intel i915/xe PMU path,
/// making a `perf_event_open` permission denial ESCALATION-AWARE.
///
/// The audited boundary crate's `perf_event_open` returns `EACCES` under a
/// restrictive `perf_event_paranoid`, which the shared IO classifier maps to
/// `FailureKind::PermissionDenied`. For the Intel PMU that denial is not a hard
/// wall: the per-feature escalation seam can reach the counter through the
/// OS-native prompt. So when the privilege gate reports
/// `RequiresEscalation(IntelPmu)` the typed failure becomes
/// `FailureKind::RequiresEscalation` — letting a consumer tell "denied, offer
/// the Intel PMU prompt" apart from a transient error — instead of the bare
/// `PermissionDenied` a non-escalatable permission gap would carry.
///
/// The escalation-aware typing is consultative: it only kicks in for the
/// permission-denial class, so an unrelated open failure (e.g. ENOENT) keeps its
/// honest non-escalation classification and the escalation signal is never
/// fabricated.
pub(super) fn classify_intel_pmu_open_failure(error: &io::Error) -> FailureKind {
    let classified = gpu_io_failure(error, FailureKind::Unsupported);
    if classified == FailureKind::PermissionDenied
        && matches!(
            UnprivilegedGate.probe(EscalationFeature::IntelPmu),
            EscalationAvailability::RequiresEscalation(_)
        )
    {
        FailureKind::RequiresEscalation
    } else {
        classified
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/headless/linux_engine_hardware_gpu_provider_intel_escalation_tests.rs"]
mod tests;
