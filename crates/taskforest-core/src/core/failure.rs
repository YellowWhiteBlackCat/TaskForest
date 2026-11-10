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
    /// force `taskforest-core` to depend on `taskmanager-escalation` (inverting
    /// the documented zero-dependency leaf) and would force `EscalationFeature`
    /// to derive serde (violating that crate's "ZERO dependencies" invariant),
    /// so the identity stays on the escalation crate's `EscalationAvailability`
    /// and is recovered from context by the consumer. The red line is honored
    /// either way: no number is fabricated while the capability is missing.
    RequiresEscalation,
}

impl FailureKind {
    /// The one catalog key for the user-facing reason of this failure.
    ///
    /// The core layer stays copy-free: the key is the vocabulary, and the
    /// application i18n owner is the single place where a key meets the active
    /// language. Before this fold existed, the copy table lived in the shell
    /// while the application layer needed the same reason — so
    /// `service_log_rejection_detail` fell back to `format!("{failure:?}")`
    /// and a Chinese user read an English enum variant. `RequiresEscalation`
    /// folds into the denial key: the escalation prompt itself is the action.
    #[must_use]
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Unsupported => "feedback.unsupported",
            Self::TemporarilyUnavailable | Self::MissingDependency => {
                "feedback.provider_unavailable"
            }
            Self::PermissionDenied | Self::RequiresEscalation => "feedback.permission_denied",
            Self::TimedOut => "feedback.timed_out",
            Self::Rejected => "feedback.request_rejected",
            Self::IdentityChanged => "feedback.target_changed",
            Self::ProviderFault => "feedback.provider_failed",
        }
    }

    /// How much the user needs to know about this failure, ranked for the one
    /// question a partial source must answer: when several fields failed at
    /// once, **which single failure represents the source**.
    ///
    /// This ordering is a product fact, so it has one address. Before it did,
    /// 24 subsystems across four crates answered it with their own table, in
    /// nine mutually inconsistent orderings: seventeen of the 24 disagreed with
    /// the one adopted here, and nine tied [`FailureKind::IdentityChanged`] with
    /// [`FailureKind::Rejected`] at the bottom, so those subsystems had no total
    /// order at all and a merge returned whichever argument arrived second.
    /// The disagreements were visible on real pairs: `{Rejected, Unsupported}`
    /// reported `Unsupported` in most subsystems but `Rejected` in the service
    /// inventory; `{ProviderFault, TimedOut}` flipped between the domain
    /// collector and the network counters; the GPU field reader ranked a
    /// mid-read identity change third of nine, above a provider timeout. Two
    /// surfaces could therefore print different headline causes for one
    /// identical set of field failures.
    ///
    /// The rank descends by how actionable and how source-level the condition
    /// is, not by how unusual it looks in a log:
    ///
    /// 1. [`FailureKind::RequiresEscalation`] — the data exists and one typed,
    ///    OS-native prompt can reach it. There is something to do *now*.
    /// 2. [`FailureKind::PermissionDenied`] — the data exists but this identity
    ///    cannot read it: a trust-boundary fact the permission model documents.
    /// 3. [`FailureKind::MissingDependency`] — a required component is absent;
    ///    installing it is a user action.
    /// 4. [`FailureKind::TimedOut`] — the provider did not answer in budget: a
    ///    live fault of the provider, not of one row.
    /// 5. [`FailureKind::ProviderFault`] — the provider answered incoherently;
    ///    nothing the user can do, but the source is actively untrustworthy.
    /// 6. [`FailureKind::TemporarilyUnavailable`] — transient; the next tick may
    ///    succeed, so it should not outrank a standing defect.
    /// 7. [`FailureKind::Unsupported`] — the platform cannot supply this at all.
    ///    Permanent and expected, so it outranks a per-target race but is not a
    ///    fault someone caused today.
    /// 8. [`FailureKind::IdentityChanged`] — the *target* was replaced mid-read.
    ///    The source worked; one row's identity moved. Self-healing next tick.
    /// 9. [`FailureKind::Rejected`] — the provider declined this specific
    ///    request. Least representative of a source's health of all.
    ///
    /// Ranks are sparse (not `0..n`) on purpose: they are compared, never stored,
    /// and the gaps keep the table readable against the list above.
    ///
    /// The arms name `FailureKind` rather than `Self` on purpose: a ranking
    /// table must name the vocabulary it ranks, which is what lets the owner
    /// gate see this table and still ignore the tables owned by
    /// `ProviderFailure`, `SmartProviderFailureKind` and
    /// `ProcessMetadataFailure`.
    ///
    /// The rank is deliberately **crate-private**: comparing ranks is a fold,
    /// and a fold written outside this file is how 24 subsystems ended up with
    /// nine different answers to the same question. Outside `taskforest-core`
    /// the only spellings are [`FailureKind::stronger`] and
    /// [`FailureKind::strongest`], so a re-ranking is a compile error rather
    /// than a review finding.
    #[must_use]
    const fn severity_rank(self) -> u8 {
        match self {
            FailureKind::RequiresEscalation => 9,
            FailureKind::PermissionDenied => 8,
            FailureKind::MissingDependency => 7,
            FailureKind::TimedOut => 6,
            FailureKind::ProviderFault => 5,
            FailureKind::TemporarilyUnavailable => 4,
            FailureKind::Unsupported => 3,
            FailureKind::IdentityChanged => 2,
            FailureKind::Rejected => 1,
        }
    }

    /// The one failure that represents two: the higher-ranked one, and `self`
    /// when the rank ties so the first observed cause is never replaced by an
    /// equal one. Folding a set of field failures into a source outcome is this
    /// method's only purpose; no layer may re-rank `FailureKind` to do it.
    #[must_use]
    pub const fn stronger(self, other: Self) -> Self {
        if other.severity_rank() > self.severity_rank() {
            other
        } else {
            self
        }
    }

    /// Fold one more observed failure into the failure that currently
    /// represents a source: `None` means nothing has failed yet, so the
    /// candidate is taken as given. This is an associated function rather than
    /// a free item so that no accumulator variable can shadow the fold it is
    /// supposed to call, and so every call site names the vocabulary it folds.
    #[must_use]
    pub const fn strongest(current: Option<Self>, candidate: Self) -> Self {
        match current {
            Some(current) => current.stronger(candidate),
            None => candidate,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_failure_tests.rs"]
mod tests;
