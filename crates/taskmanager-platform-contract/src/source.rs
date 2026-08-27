//! Source-snapshot contracts assembled from independently fallible providers.
//!
//! Successful values survive sibling failures; a single discovery authority
//! governs device presence independently of optional enrichment outcomes.

use taskmanager_core::{DeviceId, FailureKind, ProviderId, SourceOutcome, SourceStatus};

/// Constrained discovery input for physical-device snapshots. It binds the
/// outcome, discovered IDs, and reported item count in one constructor so a
/// caller cannot publish readings alongside an empty/unavailable inventory by
/// independently filling four public fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceDiscovery {
    Available(Vec<DeviceId>),
    Empty,
    Partial {
        discovered_devices: Vec<DeviceId>,
        failure: FailureKind,
    },
    Unavailable(FailureKind),
}

/// Values assembled from several independently fallible providers.
///
/// Successful values are retained when a sibling source fails. Consumers can
/// distinguish an authoritative empty observation from partial/unavailable
/// discovery by inspecting `sources`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialSourceSnapshot<T> {
    pub items: Vec<T>,
    pub sources: Vec<SourceStatus>,
}

/// One composite value assembled from independently fallible sources.
///
/// Unlike [`PartialSourceSnapshot`], this contract is for a single aggregate
/// such as hardware inventory. A missing firmware source does not discard
/// successfully observed operating-system or CPU-topology fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositeSourceSnapshot<T> {
    pub value: T,
    pub sources: Vec<SourceStatus>,
}

/// A physical-device observation with one explicit discovery authority.
///
/// `discovery` is the only source allowed to confirm that a previously known
/// device is absent. Optional enrichment failures (SMART, wireless metadata,
/// vendor libraries, and similar providers) remain visible without weakening
/// that lifecycle decision or turning a present device into an absent one.
/// ```compile_fail
/// use taskmanager_core::{DeviceId, ProviderId};
/// use taskmanager_platform_contract::{DeviceDiscovery, DeviceSourceSnapshot};
/// let mut snapshot = DeviceSourceSnapshot::from_discovery(
///     (),
///     ProviderId::borrowed("fixture"),
///     DeviceDiscovery::Available(vec![DeviceId::new("device")]),
///     Vec::new(),
/// );
/// snapshot.discovery.item_count = 0;
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSourceSnapshot<T> {
    pub value: T,
    /// Stable identities actually enumerated during this refresh.
    ///
    /// `value` may additionally retain cached rows during a partial or failed
    /// refresh. Lifecycle reconciliation must observe only this list.
    discovered_devices: Vec<DeviceId>,
    discovery: SourceStatus,
    pub enrichments: Vec<SourceStatus>,
}

impl<T> PartialSourceSnapshot<T> {
    #[must_use]
    pub fn new(items: Vec<T>, mut sources: Vec<SourceStatus>) -> Self {
        sources.sort_by(|left, right| left.provider.cmp(&right.provider));
        Self { items, sources }
    }

    #[must_use]
    pub fn is_authoritative(&self) -> bool {
        sources_are_authoritative(&self.sources)
    }
}

impl<T> CompositeSourceSnapshot<T> {
    #[must_use]
    pub fn new(value: T, mut sources: Vec<SourceStatus>) -> Self {
        sources.sort_by(|left, right| left.provider.cmp(&right.provider));
        Self { value, sources }
    }

    #[must_use]
    pub fn is_authoritative(&self) -> bool {
        sources_are_authoritative(&self.sources)
    }
}

impl<T> DeviceSourceSnapshot<T> {
    #[must_use]
    pub fn from_discovery(
        value: T,
        provider: ProviderId,
        discovery: DeviceDiscovery,
        enrichments: Vec<SourceStatus>,
    ) -> Self {
        let (mut discovered_devices, outcome) = match discovery {
            DeviceDiscovery::Available(devices) => (devices, SourceOutcome::Available),
            DeviceDiscovery::Empty => (Vec::new(), SourceOutcome::Empty),
            DeviceDiscovery::Partial {
                discovered_devices,
                failure,
            } => (discovered_devices, SourceOutcome::Partial(failure)),
            DeviceDiscovery::Unavailable(failure) => {
                (Vec::new(), SourceOutcome::Unavailable(failure))
            }
        };
        discovered_devices.sort();
        discovered_devices.dedup();
        let outcome = canonical_discovery_outcome(outcome, discovered_devices.is_empty());
        let item_count = discovered_devices.len();
        Self::assemble(
            value,
            discovered_devices,
            SourceStatus {
                provider,
                outcome,
                item_count,
            },
            enrichments,
        )
    }

    /// Migrate a legacy source status through the constrained discovery
    /// lattice. Contradictory pairs are canonicalized: empty IDs cannot be
    /// `Available`/`Partial`, and `Empty`/`Unavailable` cannot retain IDs.
    #[must_use]
    pub fn from_source_status(
        value: T,
        discovered_devices: Vec<DeviceId>,
        discovery: SourceStatus,
        enrichments: Vec<SourceStatus>,
    ) -> Self {
        let provider = discovery.provider;
        let constrained = match discovery.outcome {
            SourceOutcome::Available if discovered_devices.is_empty() => DeviceDiscovery::Empty,
            SourceOutcome::Available => DeviceDiscovery::Available(discovered_devices),
            SourceOutcome::Empty => DeviceDiscovery::Empty,
            SourceOutcome::Partial(failure) if discovered_devices.is_empty() => {
                DeviceDiscovery::Unavailable(failure)
            }
            SourceOutcome::Partial(failure) => DeviceDiscovery::Partial {
                discovered_devices,
                failure,
            },
            SourceOutcome::Unavailable(failure) => DeviceDiscovery::Unavailable(failure),
        };
        Self::from_discovery(value, provider, constrained, enrichments)
    }

    fn assemble(
        value: T,
        discovered_devices: Vec<DeviceId>,
        discovery: SourceStatus,
        mut enrichments: Vec<SourceStatus>,
    ) -> Self {
        enrichments.sort_by(|left, right| left.provider.cmp(&right.provider));
        Self {
            value,
            discovered_devices,
            discovery,
            enrichments,
        }
    }

    #[must_use]
    pub fn discovered_devices(&self) -> &[DeviceId] {
        &self.discovered_devices
    }

    #[must_use]
    pub const fn discovery(&self) -> &SourceStatus {
        &self.discovery
    }

    /// Replace enumerated IDs while preserving the discovery lattice.
    pub fn replace_discovered_devices(&mut self, mut discovered_devices: Vec<DeviceId>) {
        discovered_devices.sort();
        discovered_devices.dedup();
        let outcome =
            canonical_discovery_outcome(self.discovery.outcome, discovered_devices.is_empty());
        if matches!(outcome, SourceOutcome::Unavailable(_)) {
            discovered_devices.clear();
        }
        self.discovery.outcome = outcome;
        self.discovery.item_count = discovered_devices.len();
        self.discovered_devices = discovered_devices;
    }

    /// Whether the authoritative inventory completed this observation.
    ///
    /// A partial discovery may retain valid devices, but it cannot confirm
    /// absence. Enrichment outcomes are deliberately irrelevant here.
    #[must_use]
    pub const fn discovery_is_authoritative(&self) -> bool {
        matches!(
            self.discovery.outcome,
            SourceOutcome::Available | SourceOutcome::Empty
        )
    }

    /// Flatten source diagnostics for read models that retain a single list.
    #[must_use]
    pub fn into_value_and_sources(self) -> (T, Vec<DeviceId>, Vec<SourceStatus>) {
        let mut sources = Vec::with_capacity(self.enrichments.len().saturating_add(1));
        sources.push(self.discovery);
        sources.extend(self.enrichments);
        sources.sort_by(|left, right| left.provider.cmp(&right.provider));
        (self.value, self.discovered_devices, sources)
    }
}

const fn canonical_discovery_outcome(outcome: SourceOutcome, ids_empty: bool) -> SourceOutcome {
    match (outcome, ids_empty) {
        (SourceOutcome::Available, true) => SourceOutcome::Empty,
        (SourceOutcome::Empty, false) => SourceOutcome::Available,
        (SourceOutcome::Partial(failure), true) => SourceOutcome::Unavailable(failure),
        (SourceOutcome::Unavailable(failure), _) => SourceOutcome::Unavailable(failure),
        (outcome, _) => outcome,
    }
}

fn sources_are_authoritative(sources: &[SourceStatus]) -> bool {
    sources.iter().all(|source| {
        matches!(
            source.outcome,
            SourceOutcome::Available | SourceOutcome::Empty
        )
    })
}

#[cfg(test)]
#[path = "../tests/headless/source_contract.rs"]
mod tests;
