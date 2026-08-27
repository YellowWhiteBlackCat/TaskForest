//! Stable, platform-neutral failure vocabulary.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Unsupported,
    PermissionDenied,
    MissingDependency,
    TimedOut,
    IdentityChanged,
    TemporarilyUnavailable,
    Rejected,
    ProviderFault,
    /// The data is absent because the unprivileged process lacks a capability
    /// that the per-feature escalation seam (ADR-023, permission-model
    /// Boundary 2) can reach through the OS-native prompt — so a consumer can
    /// tell "denied, offer one specific escalation" apart from a transient or
    /// hard `PermissionDenied`.
    ///
    /// This is the payload-free escalation-aware state: the FEATURE to prompt
    /// for is identified by the producing path (e.g. the Intel i915/xe PMU
    /// provider names `EscalationFeature::IntelPmu` at its denial point) and is
    /// NOT re-typed here. Carrying `EscalationFeature` on this variant would
    /// force `taskmanager-core` to depend on `taskmanager-escalation` (inverting
    /// the documented zero-dependency leaf) and would force `EscalationFeature`
    /// to derive serde (violating that crate's "ZERO dependencies" invariant),
    /// so the identity stays on the escalation crate's `EscalationAvailability`
    /// and is recovered from context by the consumer. The red line is honored
    /// either way: no number is fabricated while the capability is missing.
    RequiresEscalation,
}
