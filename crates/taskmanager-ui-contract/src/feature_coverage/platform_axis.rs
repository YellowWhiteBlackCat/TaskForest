//! The platform axis fold: `FeatureId x FrontendShape x PlatformAxis`.
//!
//! This module owns the CONNECTION between the three axes, not a fourth
//! vocabulary. It composes two axes this crate owns ([`FeatureId`] and
//! [`FrontendShape`]) with the platform identity axis owned by
//! `taskmanager-platform-contract` ([`PlatformAxis`]; consumers import that
//! type from its owner, this crate never re-exports it) and folds them with
//! one mechanical function, [`classify`].
//!
//! ## One fact, one authority
//!
//! | Layer | Fact | Authority |
//! |---|---|---|
//! | A | what a feature needs from the platform | [`FeatureId::platform_binding`] |
//! | B | which source a platform registers per capability | [`PlatformCapabilitySurface`] (supplied by the caller; M3.2 moves the concrete declarations next to the real providers) |
//! | C | what the runtime catalog reports right now | `CapabilityCatalog::snapshot`, consumed only by platform conformance - never folded into this static ledger |
//!
//! Layer C is deliberately excluded: the runtime catalog seeds descriptors only
//! for registered routes, so folding it would silently turn "absent" into "no
//! entry" and lose the typed reason. Typed absence vocabulary is reused, never
//! duplicated: [`PlatformSource::Absent`] carries
//! [`CapabilityStatus`], and [`FeaturePlatformStatus::Unsupported`] carries the
//! capability-level reason plus its [`RetryDisposition`].
//!
//! ## `Missing` versus `Unsupported` (definition A)
//!
//! - `Missing` is produced ONLY by the frontend axis: a declaration that omits
//!   the feature or declares [`CapabilitySupport::Unsupported`]. It is a real
//!   parity defect and the only frontend-axis defect this fold reports.
//! - `Unsupported`/`Unregistered` are produced ONLY by the platform axis: the
//!   feature needs a capability identity but the platform registers no source
//!   for it (or the feature has no capability identity yet).
//!
//! The two are never inferred from each other. A platform `Unsupported` with a
//! declared frontend entry is the achieved shape of definition A, not a gap;
//! a platform `Present` with no frontend entry is `Missing`.
//!
//! ## Read-only at M3.1
//!
//! [`feature_platform_report`] returns the per-cell status without failing
//! anything: M3.1 lands the axes and the mechanical fold, and deliberately
//! does not wire a hard gate (M3.4 does). [`FeaturePlatformStatus::Ready`]
//! means "the static source commitment is complete" - never a runtime
//! `Available` claim, which only platform conformance can prove on a real
//! host. Evidence anchors arrive with the P4 manifest integration, so every
//! M3.1 cell honestly reports [`NO_EVIDENCE`].
//!
//! ## `NotApplicable` criterion
//!
//! Only [`PlatformBinding::NotApplicable`] produces
//! [`FeaturePlatformStatus::NotApplicable`], and only when the delivery
//! definition is fully satisfied by shared projections/derivation, no source
//! is waiting to be implemented, and the binding note states why. A platform
//! gap that is merely not implemented yet must stay `Requires` + absent or
//! undeclared; marking it `NotApplicable` would legitimize the gap.

use std::collections::BTreeMap;
use std::fmt;

use taskmanager_platform_contract::{
    CapabilityId, CapabilityStatus, PlatformAxis, RetryDisposition,
};

use super::platform_binding::PlatformBinding;
use super::{FeatureCoverageDeclaration, FeatureId};
use crate::capabilities::CapabilitySupport;
use crate::keybindings::FrontendShape;

/// The evidence anchor a cell carries before the P4 manifest integration
/// attaches real anchors.
///
/// An empty anchor is the honest "no proof attached yet" state; it is never a
/// substitute for behaviour evidence, and a `Ready` cell that still carries
/// [`NO_EVIDENCE`] claims the static source commitment only.
pub const NO_EVIDENCE: &str = "";

/// Reason reported when a frontend declaration omits a registered feature.
pub const MISSING_DECLARATION_REASON: &str = "frontend declaration omits the feature";

/// Reason reported when a frontend declares
/// [`CapabilitySupport::Unsupported`] with an empty explanation.
pub const MISSING_UNSUPPORTED_REASON: &str = "frontend declares no entry point";

const fn non_empty_reason(reason: &'static str, fallback: &'static str) -> &'static str {
    if reason.is_empty() { fallback } else { reason }
}

/// One capability's static registration fact on one platform.
///
/// This is a declaration, never a runtime status: `Present` means "this
/// platform registers a real source for the capability identity"; a runtime
/// `Available`/`Degraded` observation belongs to `CapabilityCatalog` and is
/// checked by platform conformance, not folded here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformSource {
    /// A real provider is registered for every required capability.
    Present,
    /// Registered but honestly absent, with the typed capability-level
    /// reason. The status is restricted to the four absence projections of
    /// `ProviderFailure::capability_status`
    /// ([`PlatformSource::is_absence_projection`]); runtime transients are not
    /// static commitments.
    Absent(CapabilityStatus),
    /// The product expects the capability identity, but this platform
    /// registers no declaration at all - the forbidden silent absence.
    Undeclared,
}

/// Why a [`CapabilityStatus`] cannot justify a static absence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformSourceError {
    /// The rejected status.
    pub status: CapabilityStatus,
}

impl fmt::Display for PlatformSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "capability status {:?} is a runtime state, not a static absence projection",
            self.status
        )
    }
}

impl std::error::Error for PlatformSourceError {}

impl PlatformSource {
    /// Whether `status` is one of the four absence projections
    /// `ProviderFailure::capability_status` can produce from a static failure.
    ///
    /// A runtime status (`Available`, `Degraded`, `TemporarilyUnavailable`,
    /// `Stale`) never justifies a static absence.
    #[must_use]
    pub const fn is_absence_projection(status: CapabilityStatus) -> bool {
        matches!(
            status,
            CapabilityStatus::Unsupported
                | CapabilityStatus::PermissionRequired
                | CapabilityStatus::RequiresEscalation
                | CapabilityStatus::MissingDependency
        )
    }

    /// Typed constructor for an honest static absence.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformSourceError`] when `status` is a runtime state that
    /// cannot be a static source commitment.
    pub fn absent(status: CapabilityStatus) -> Result<Self, PlatformSourceError> {
        if Self::is_absence_projection(status) {
            Ok(Self::Absent(status))
        } else {
            Err(PlatformSourceError { status })
        }
    }
}

/// One platform's declared source surface (layer B).
///
/// The surface is input, not authority: callers (platform adapter conformance
/// crates, or a test fixture) declare `(platform, capability) -> source`.
/// [`PlatformCapabilitySurface::source`] answers [`PlatformSource::Undeclared`]
/// for a pair nobody declared, so a missing declaration is a typed, visible
/// fact instead of a default success.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlatformCapabilitySurface {
    entries: BTreeMap<(PlatformAxis, CapabilityId), PlatformSource>,
}

impl PlatformCapabilitySurface {
    /// An empty surface: every queried pair is `Undeclared`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare one platform's source commitment, replacing and returning any
    /// previous declaration for the same pair.
    pub fn declare(
        &mut self,
        platform: PlatformAxis,
        capability: CapabilityId,
        source: PlatformSource,
    ) -> Option<PlatformSource> {
        self.entries.insert((platform, capability), source)
    }

    /// The declared source for one platform/capability pair.
    #[must_use]
    pub fn source(&self, platform: PlatformAxis, capability: &CapabilityId) -> PlatformSource {
        self.entries
            .get(&(platform, capability.clone()))
            .copied()
            .unwrap_or(PlatformSource::Undeclared)
    }

    /// Every declared pair in deterministic order.
    pub fn entries(
        &self,
    ) -> impl Iterator<Item = ((PlatformAxis, CapabilityId), PlatformSource)> + '_ {
        self.entries
            .iter()
            .map(|(key, source)| (key.clone(), *source))
    }
}

/// The typed reason a non-`Ready` cell reports for an unavailable source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformUnavailability {
    /// The required capability identity, or `None` when the feature has no
    /// capability identity yet (`Unregistered`).
    pub capability: Option<CapabilityId>,
    /// The authoritative typed reason. A conforming source surface restricts
    /// this to an absence projection
    /// ([`PlatformSource::is_absence_projection`]); an inconsistent
    /// declaration is preserved verbatim so the report shows the
    /// contradiction instead of hiding it.
    pub status: CapabilityStatus,
    /// The authoritative retry semantics for `status`.
    pub retry: RetryDisposition,
}

/// Why a cell is `Partial`: at least one half is populated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartialCause {
    /// The platform source gap, when a required capability has no complete
    /// source.
    pub platform: Option<CapabilityStatus>,
    /// The frontend's own narrowing reason, when the declaration is
    /// [`CapabilitySupport::Divergent`].
    pub frontend: Option<&'static str>,
}

/// The folded state of one `(feature, frontend, platform)` cell.
///
/// The names deliberately do not collide with [`CapabilityStatus`] or
/// [`super::FeatureCoverageStatus`]: this is a third, derived axis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeaturePlatformStatus {
    /// The frontend declares an entry and every required platform source is
    /// `Present` - a static source commitment, never a runtime `Available`
    /// claim.
    Ready,
    /// Every source exists, but a permission decision stands in the way
    /// ([`CapabilityStatus::PermissionRequired`] or
    /// [`CapabilityStatus::RequiresEscalation`]). The escalation affordance is
    /// preserved, never folded into a generic denial.
    Gated(CapabilityStatus),
    /// The platform has no source (or registers nothing) for a required
    /// capability; the typed reason names the capability.
    Unsupported(PlatformUnavailability),
    /// A source exists but coverage is incomplete: the frontend narrows the
    /// surface, or only some required capabilities are present, or both.
    Partial(PartialCause),
    /// The frontend has no entry point. This is the real parity defect; the
    /// platform axis never produces it.
    Missing(&'static str),
    /// The delivery has no operating-system source by design; the binding note
    /// states why.
    NotApplicable(&'static str),
    /// The feature needs operating-system facts with no capability identity
    /// yet. The note lives on [`FeatureId::platform_binding`]
    /// ([`PlatformBinding::note`]) and is deliberately not copied here, so the
    /// binding stays the one authority.
    Unregistered(PlatformUnavailability),
}

impl FeaturePlatformStatus {
    /// Stable machine label for reports and diagnostics.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Gated(_) => "gated",
            Self::Unsupported(_) => "unsupported",
            Self::Partial(_) => "partial",
            Self::Missing(_) => "missing",
            Self::NotApplicable(_) => "not-applicable",
            Self::Unregistered(_) => "unregistered",
        }
    }
}

/// One row of the read-only three-axis report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeaturePlatformCell {
    pub feature: FeatureId,
    pub frontend: FrontendShape,
    pub platform: PlatformAxis,
    pub status: FeaturePlatformStatus,
    /// Evidence anchor (`test_id_or_scenario` in the P4 manifest), or
    /// [`NO_EVIDENCE`] while no anchor is attached.
    pub evidence: &'static str,
}

impl FeaturePlatformCell {
    /// Whether a behaviour evidence anchor is attached.
    #[must_use]
    pub const fn has_evidence(&self) -> bool {
        !self.evidence.is_empty()
    }
}

/// The folded cell matrix over one or more frontend declarations, in
/// declaration x [`FeatureId::ALL`] x [`PlatformAxis::ALL`] order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeaturePlatformLedger {
    cells: Vec<FeaturePlatformCell>,
}

impl FeaturePlatformLedger {
    /// Fold one frontend declaration over every feature and platform.
    #[must_use]
    pub fn from_declaration(
        declaration: &FeatureCoverageDeclaration,
        sources: &PlatformCapabilitySurface,
    ) -> Self {
        Self::from_declarations(std::slice::from_ref(declaration), sources)
    }

    /// Fold several frontend declarations.
    ///
    /// Callers pass at most one declaration per frontend shape; a duplicated
    /// shape duplicates its cells and is left visible rather than merged.
    #[must_use]
    pub fn from_declarations(
        declarations: &[FeatureCoverageDeclaration],
        sources: &PlatformCapabilitySurface,
    ) -> Self {
        let mut cells =
            Vec::with_capacity(declarations.len() * FeatureId::ALL.len() * PlatformAxis::ALL.len());
        for declaration in declarations {
            for feature in FeatureId::ALL {
                for platform in PlatformAxis::ALL {
                    cells.push(FeaturePlatformCell {
                        feature: *feature,
                        frontend: declaration.frontend,
                        platform,
                        status: classify(declaration, *feature, platform, sources),
                        evidence: NO_EVIDENCE,
                    });
                }
            }
        }
        Self { cells }
    }

    /// Every cell in canonical order.
    #[must_use]
    pub fn cells(&self) -> &[FeaturePlatformCell] {
        &self.cells
    }

    /// Iterate every cell in canonical order.
    pub fn iter(&self) -> impl Iterator<Item = &FeaturePlatformCell> {
        self.cells.iter()
    }

    /// Number of folded cells.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Whether the ledger carries no cell.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// The cell for one triple, if it is part of this fold.
    #[must_use]
    pub fn cell(
        &self,
        feature: FeatureId,
        frontend: FrontendShape,
        platform: PlatformAxis,
    ) -> Option<&FeaturePlatformCell> {
        self.cells.iter().find(|cell| {
            cell.feature == feature && cell.frontend == frontend && cell.platform == platform
        })
    }

    /// The folded status for one triple, if it is part of this fold.
    #[must_use]
    pub fn status(
        &self,
        feature: FeatureId,
        frontend: FrontendShape,
        platform: PlatformAxis,
    ) -> Option<&FeaturePlatformStatus> {
        self.cell(feature, frontend, platform)
            .map(|cell| &cell.status)
    }
}

/// The single mechanical fold of one cell: frontend axis first, then platform
/// axis.
///
/// The frontend axis decides `Missing` independently of the platform (a
/// feature without an entry point is a parity defect even when no platform has
/// a source). [`CapabilitySupport::Divergent`] records a frontend narrowing
/// that surfaces as `Partial` when the platform axis would otherwise be
/// `Ready`. The platform axis then folds [`FeatureId::platform_binding`]
/// against the declared surface; it never infers a frontend gap.
#[must_use]
pub fn classify(
    declaration: &FeatureCoverageDeclaration,
    feature: FeatureId,
    platform: PlatformAxis,
    sources: &PlatformCapabilitySurface,
) -> FeaturePlatformStatus {
    let declared_support = declaration
        .entries
        .iter()
        .find(|entry| entry.feature == feature)
        .map(|entry| entry.support);
    let frontend_partial = match declared_support {
        None => return FeaturePlatformStatus::Missing(MISSING_DECLARATION_REASON),
        Some(CapabilitySupport::Unsupported { reason }) => {
            return FeaturePlatformStatus::Missing(non_empty_reason(
                reason,
                MISSING_UNSUPPORTED_REASON,
            ));
        }
        Some(CapabilitySupport::Divergent { reason }) => Some(reason),
        Some(
            CapabilitySupport::Reference
            | CapabilitySupport::Ported
            | CapabilitySupport::Native { .. },
        ) => None,
    };
    match feature.platform_binding() {
        PlatformBinding::NotApplicable(reason) => FeaturePlatformStatus::NotApplicable(reason),
        PlatformBinding::NotInVocabulary(_) => unregistered_without_identity(),
        PlatformBinding::Requires(capabilities) => {
            classify_required(capabilities, platform, sources, frontend_partial)
        }
    }
}

/// Fold one `Requires` binding against the declared platform sources.
fn classify_required(
    capabilities: &'static [CapabilityId],
    platform: PlatformAxis,
    sources: &PlatformCapabilitySurface,
    frontend_partial: Option<&'static str>,
) -> FeaturePlatformStatus {
    if capabilities.is_empty() {
        // A `Requires` binding without a capability identity is a declaration
        // contradiction: report it as unregistered, never as fabricate-ready.
        return unregistered_without_identity();
    }
    let mut present = 0usize;
    let mut gated: Option<CapabilityStatus> = None;
    let mut first_gap: Option<(CapabilityId, CapabilityStatus)> = None;
    for capability in capabilities {
        match sources.source(platform, capability) {
            PlatformSource::Present => present += 1,
            PlatformSource::Absent(status) => {
                if gated.is_none()
                    && matches!(
                        status,
                        CapabilityStatus::PermissionRequired | CapabilityStatus::RequiresEscalation
                    )
                {
                    gated = Some(status);
                }
                if first_gap.is_none() {
                    first_gap = Some((capability.clone(), status));
                }
            }
            PlatformSource::Undeclared => {
                if first_gap.is_none() {
                    first_gap = Some((capability.clone(), CapabilityStatus::Unsupported));
                }
            }
        }
    }
    if let Some(status) = gated {
        return FeaturePlatformStatus::Gated(status);
    }
    match first_gap {
        None => {
            // Every required source is declared present; only the frontend
            // axis can still narrow the surface.
            if frontend_partial.is_none() {
                FeaturePlatformStatus::Ready
            } else {
                FeaturePlatformStatus::Partial(PartialCause {
                    platform: None,
                    frontend: frontend_partial,
                })
            }
        }
        Some((capability, status)) => {
            if present > 0 || frontend_partial.is_some() {
                FeaturePlatformStatus::Partial(PartialCause {
                    platform: Some(status),
                    frontend: frontend_partial,
                })
            } else {
                FeaturePlatformStatus::Unsupported(PlatformUnavailability {
                    capability: Some(capability),
                    status,
                    retry: absence_retry(status),
                })
            }
        }
    }
}

/// The typed state of a feature with no capability identity.
fn unregistered_without_identity() -> FeaturePlatformStatus {
    FeaturePlatformStatus::Unregistered(PlatformUnavailability {
        capability: None,
        status: CapabilityStatus::Unsupported,
        retry: RetryDisposition::Never,
    })
}

/// Retry semantics for one absence projection.
///
/// This is the inverse of the single failure -> status authority
/// (`ProviderFailure::capability_status`) on the four absence projections;
/// `absence_retry_matches_the_provider_failure_authority` pins the round-trip.
/// A runtime status that cannot be an absence keeps
/// [`RetryDisposition::Never`]: it is a declaration contradiction the report
/// preserves, never a behaviour claim.
const fn absence_retry(status: CapabilityStatus) -> RetryDisposition {
    match status {
        CapabilityStatus::Unsupported => RetryDisposition::Never,
        CapabilityStatus::PermissionRequired
        | CapabilityStatus::RequiresEscalation
        | CapabilityStatus::MissingDependency => RetryDisposition::AfterCapabilityChange,
        CapabilityStatus::Available
        | CapabilityStatus::Degraded(_)
        | CapabilityStatus::TemporarilyUnavailable
        | CapabilityStatus::Stale => RetryDisposition::Never,
    }
}

/// One-call read-only report: the `FeatureId::ALL x PlatformAxis::ALL` fold for
/// one frontend declaration.
///
/// M3.1 returns the matrix without failing anything; hard gates land in M3.4.
#[must_use]
pub fn feature_platform_report(
    declaration: &FeatureCoverageDeclaration,
    sources: &PlatformCapabilitySurface,
) -> FeaturePlatformLedger {
    FeaturePlatformLedger::from_declaration(declaration, sources)
}

#[cfg(test)]
#[path = "../../tests/headless/ui_feature_platform_axis.rs"]
mod tests;
