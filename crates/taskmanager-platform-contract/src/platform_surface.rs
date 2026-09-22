//! Static per-platform capability registration surface (layer B of the
//! three-axis parity ledger).
//!
//! This module owns the vocabulary a platform uses to DECLARE which capability
//! source it registers and whether that source is present, honestly absent, or
//! undeclared. It lives next to the capability identity it describes
//! ([`CapabilityId`]) and the platform axis it addresses ([`PlatformAxis`]), so
//! a registration declaration never needs a second address: consumers import
//! the owner types from this crate.
//!
//! The surface is a declaration, never a runtime status: `Present` means "this
//! platform registers a real source for the capability identity", and a runtime
//! `Available`/`Degraded` observation belongs to a `CapabilityCatalog`
//! snapshot, which platform conformance checks separately. The typed absence
//! vocabulary is reused, never duplicated: [`PlatformSource::Absent`] carries
//! [`CapabilityStatus`], restricted to the four absence projections
//! `ProviderFailure::capability_status` can produce
//! ([`PlatformSource::is_absence_projection`]).
//!
//! The three-axis ledger in `taskmanager-ui-contract` folds its
//! feature-to-platform bindings against this surface; this crate never depends
//! on that fold, so the dependency direction stays one-way.

use std::collections::BTreeMap;
use std::fmt;

use crate::{CapabilityId, CapabilityStatus, PlatformAxis};

/// One capability's static registration fact on one platform.
///
/// This is a declaration, never a runtime status: `Present` means "this
/// platform registers a real source for the capability identity"; a runtime
/// `Available`/`Degraded` observation belongs to a capability catalog snapshot
/// and is checked by platform conformance, not declared here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformSource {
    /// A real provider is registered for the capability.
    Present,
    /// The capability answers with a typed absence, with the typed
    /// capability-level reason: either a registered provider that can only
    /// answer an absence (a registered-pending lane), or a product-expected
    /// identity this platform registers no source for at all
    /// ([`PlatformCapabilitySurface::declaring`] pads those). The status is
    /// restricted to the four absence projections of
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

    /// Declare one platform's complete layer-B surface from the lanes its
    /// adapter registers a provider for.
    ///
    /// `registered` names every identity the adapter registers a provider
    /// identity for, with its honest static source:
    ///
    /// - [`PlatformSource::Present`] - the registered provider really serves
    ///   the capability;
    /// - [`PlatformSource::Absent`] - a registered provider that can only answer
    ///   a typed absence (a registered-pending lane).
    ///
    /// Every other product-expected identity answers
    /// [`PlatformSource::Absent`] with [`CapabilityStatus::Unsupported`]: the
    /// adapter registers no source for it, so the honest static fact is the
    /// unsupported absence, never a silent omission. The identity set therefore
    /// comes from [`CapabilityId::EXPECTED_SURFACE`], the one authority for
    /// "which capabilities the product answers for", and the result never
    /// answers [`PlatformSource::Undeclared`] for an expected capability.
    ///
    /// A declaration outside the expected surface is kept verbatim: an adapter
    /// may register a vendor/diagnostic lane, and the surface stays a truthful
    /// mirror of the registrations instead of dropping it.
    #[must_use]
    pub fn declaring(
        platform: PlatformAxis,
        registered: &[(CapabilityId, PlatformSource)],
    ) -> Self {
        let mut surface = Self::new();
        for capability in CapabilityId::EXPECTED_SURFACE {
            let source = registered
                .iter()
                .find(|(candidate, _)| *candidate == capability)
                .map_or(
                    PlatformSource::Absent(CapabilityStatus::Unsupported),
                    |(_, source)| *source,
                );
            surface.declare(platform, capability, source);
        }
        for (capability, source) in registered {
            if !capability.is_expected() {
                surface.declare(platform, capability.clone(), *source);
            }
        }
        surface
    }

    /// Number of declared `(platform, capability)` pairs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no pair is declared at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many of this platform's declared pairs are `Present`.
    ///
    /// This is the count of capabilities the adapter registers a real source
    /// for; registered-pending and unregistered lanes are excluded because
    /// their honest static source is `Absent`.
    #[must_use]
    pub fn present_count(&self, platform: PlatformAxis) -> usize {
        self.entries
            .iter()
            .filter(|((declared_platform, _), source)| {
                *declared_platform == platform && **source == PlatformSource::Present
            })
            .count()
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

#[cfg(test)]
#[path = "../tests/headless/platform_surface.rs"]
mod tests;
