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

#[cfg(test)]
#[path = "../tests/headless/platform_surface.rs"]
mod tests;
