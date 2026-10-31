//! Platform axis identity for the product parity ledger.
//!
//! [`PlatformAxis`] names the operating-system axis the three-axis parity
//! ledger folds against: the same three roles the composition boundary selects
//! with `cfg` (ADR-051). It carries identity only - no availability semantics
//! and no current-platform detection. Mapping "the host this binary runs on"
//! to one axis belongs to composition, exactly like every other platform
//! selection in this workspace; typed availability facts stay with
//! [`crate::CapabilityStatus`] and never leak into the axis identity.
//!
//! The axis is deliberately the three SHIPPED product targets. Android and
//! OpenHarmony have feature-gated placeholder seams
//! (`taskmanager-platform-android`, `taskmanager-platform-ohos`) that publish
//! the shared capability-absent handle and are not connected to any product
//! frontend (ADR-043/044/053); they are not product platforms, so they are not
//! variants here. Declaring one would fabricate product support. Admitting a
//! fourth role is a hard cutover of every exhaustive match and of
//! [`PlatformAxis::ALL`], moved together with the count pin in
//! `tests/headless/platform_axis.rs`.

use std::fmt;

/// One operating-system platform role in the product parity contract.
///
/// The variants mirror the three `cfg` roles of the composition boundary
/// one-to-one. Consumers (feature ledgers, evidence manifests, conformance
/// reports) fold against [`PlatformAxis::ALL`] and [`PlatformAxis::id`]; a new
/// platform role is a hard cutover of every exhaustive match, never a silent
/// string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlatformAxis {
    /// Linux.
    Linux,
    /// Windows.
    Windows,
    /// macOS.
    Macos,
}

impl PlatformAxis {
    /// Every platform axis in canonical order.
    pub const ALL: [Self; 3] = [Self::Linux, Self::Windows, Self::Macos];

    /// Stable machine id used by ledgers, manifests and diagnostics.
    ///
    /// The match is exhaustive without a wildcard, so a new variant fails to
    /// compile until every report site names it.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Windows => "windows",
            Self::Macos => "macos",
        }
    }

    /// Resolve a stable machine id back to its axis.
    ///
    /// This is the membership primitive a manifest validator or report parser
    /// uses: an id that does not resolve is an unknown platform, never a
    /// silently dropped row.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|axis| axis.id() == id)
    }
}

impl fmt::Display for PlatformAxis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.id())
    }
}

#[cfg(test)]
#[path = "../tests/headless/platform_axis.rs"]
mod tests;
