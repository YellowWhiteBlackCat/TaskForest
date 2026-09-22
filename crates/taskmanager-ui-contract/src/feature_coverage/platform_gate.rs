//! The hard three-axis gate (P5 M3.4): rules G1-G6 over the folded ledger.
//!
//! [`feature_platform_gate_findings`] is the one-call gate; an empty result is
//! the only passing state. Each finding names its rule
//! ([`PlatformGateFinding::rule`]) so a red gate points at the exact invariant
//! it refused, and [`PlatformGatePolicy`] carries the one authority for the
//! product-expected identity set, the explicit feature-independent
//! declarations, and the directional baselines.
//!
//! ## Rules
//!
//! | Rule | Invariant |
//! |---|---|
//! | G1 | the ledger covers `frontend x FeatureId::ALL x PlatformAxis::ALL` exactly once |
//! | G2 | every `Ready` cell carries a usable behaviour evidence anchor (never a placeholder) |
//! | G3 | every non-`Ready` cell carries its typed reason (absence projection + authoritative retry, or a non-empty note) |
//! | G4 | no false success: a `Ready` cell needs every required source `Present`, and a non-delivery cell carries no behaviour anchor |
//! | G5 | capability binding is bidirectional: every `Requires` identity is product-expected, and every expected identity is bound, declared feature-independent, or accepted baseline debt |
//! | G6 | the `Missing` and `Unregistered` counts stay at or below their directional baselines |
//!
//! ## Honest baseline, not a rubber stamp
//!
//! [`PLATFORM_GATE_BASELINE`] records the current legal gaps with their removal
//! conditions. The accepted-debt list for unbound expected capabilities is a
//! live census: binding one of them, or explicitly declaring it
//! feature-independent, makes the baseline entry stale and fails G5 until the
//! baseline shrinks in the same change - the ceiling only moves down.
//!
//! Evidence anchors arrive with the P4 manifest integration; until then no cell
//! may claim `Ready` with an empty anchor, so a new `Ready` claim is forced to
//! land together with its proof instead of borrowing the static-commitment
//! wording.

use std::collections::{BTreeMap, BTreeSet};

use taskmanager_platform_contract::{
    CapabilityId, CapabilityStatus, PlatformAxis, PlatformCapabilitySurface, PlatformSource,
};

use super::FeatureId;
use super::platform_axis::{
    FeaturePlatformCell, FeaturePlatformLedger, FeaturePlatformStatus, PlatformUnavailability,
    absence_retry,
};
use super::platform_binding::{FEATURE_INDEPENDENT_CAPABILITIES, PlatformBinding};
use crate::keybindings::FrontendShape;

/// The gate rule a finding belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlatformGateRule {
    /// G1 exhaustive grid.
    G1,
    /// G2 `Ready` carries evidence.
    G2,
    /// G3 non-`Ready` carries a typed reason.
    G3,
    /// G4 no false success.
    G4,
    /// G5 bidirectional capability binding.
    G5,
    /// G6 directional ceiling.
    G6,
}

impl PlatformGateRule {
    /// Stable machine id used in gate output.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::G1 => "G1",
            Self::G2 => "G2",
            Self::G3 => "G3",
            Self::G4 => "G4",
            Self::G5 => "G5",
            Self::G6 => "G6",
        }
    }
}

/// One gate finding.
///
/// `detail` names the exact sub-invariant, so a failure is diagnosable without
/// reading the gate source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformGateFinding {
    /// G1: a ledger with no frontend is not a grid.
    GridEmpty,
    /// G1: the cross-product cell is absent from the ledger.
    GridCellGap {
        feature: FeatureId,
        frontend: FrontendShape,
        platform: PlatformAxis,
    },
    /// G1: the cross-product cell is folded more than once.
    GridCellDuplicate {
        feature: FeatureId,
        frontend: FrontendShape,
        platform: PlatformAxis,
    },
    /// G2: a `Ready` cell cannot name the proof behind its claim.
    ReadyWithoutEvidence {
        feature: FeatureId,
        frontend: FrontendShape,
        platform: PlatformAxis,
        detail: &'static str,
    },
    /// G3: a non-`Ready` cell does not carry a typed reason.
    UntypedReason {
        feature: FeatureId,
        frontend: FrontendShape,
        platform: PlatformAxis,
        detail: &'static str,
    },
    /// G3: a reasoned cell carries an empty note.
    EmptyReason {
        feature: FeatureId,
        frontend: FrontendShape,
        platform: PlatformAxis,
    },
    /// G3: a `Partial` cell names neither half of its cause.
    PartialWithoutCause {
        feature: FeatureId,
        frontend: FrontendShape,
        platform: PlatformAxis,
    },
    /// G4: a cell claims a delivery shape its inputs contradict.
    FalseSuccess {
        feature: FeatureId,
        frontend: FrontendShape,
        platform: PlatformAxis,
        detail: &'static str,
    },
    /// G5: a `Requires` binding references an identity outside the product
    /// surface.
    CapabilityOutsideProductSurface {
        capability: CapabilityId,
        detail: &'static str,
    },
    /// G5: an expected identity is neither bound nor accepted.
    UnboundExpectedCapability { capability: CapabilityId },
    /// G5: an accepted-debt or feature-independent entry must be removed:
    /// the capability is now bound, or it left the product surface.
    StaleCapabilityBaseline {
        capability: CapabilityId,
        detail: &'static str,
    },
    /// G5: an explicit feature-independent declaration carries no reason.
    EmptyIndependentReason { capability: CapabilityId },
    /// G6: the `Missing` count rose above its per-frontend ceiling.
    MissingCeilingExceeded {
        frontend: FrontendShape,
        counted: usize,
        ceiling: usize,
    },
    /// G6: the `Unregistered` count rose above its per-frontend ceiling.
    UnregisteredCeilingExceeded {
        frontend: FrontendShape,
        counted: usize,
        ceiling: usize,
    },
}

impl PlatformGateFinding {
    /// The rule this finding enforces.
    #[must_use]
    pub const fn rule(&self) -> PlatformGateRule {
        match self {
            Self::GridEmpty | Self::GridCellGap { .. } | Self::GridCellDuplicate { .. } => {
                PlatformGateRule::G1
            }
            Self::ReadyWithoutEvidence { .. } => PlatformGateRule::G2,
            Self::UntypedReason { .. }
            | Self::EmptyReason { .. }
            | Self::PartialWithoutCause { .. } => PlatformGateRule::G3,
            Self::FalseSuccess { .. } => PlatformGateRule::G4,
            Self::CapabilityOutsideProductSurface { .. }
            | Self::UnboundExpectedCapability { .. }
            | Self::StaleCapabilityBaseline { .. }
            | Self::EmptyIndependentReason { .. } => PlatformGateRule::G5,
            Self::MissingCeilingExceeded { .. } | Self::UnregisteredCeilingExceeded { .. } => {
                PlatformGateRule::G6
            }
        }
    }
}

/// Directional baselines for the gaps the product accepts today, each with its
/// removal condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformGateBaseline<'a> {
    /// `Missing` cell ceiling per frontend (summed over the three platforms).
    ///
    /// Removal condition: a frontend lowers its own count by delivering the
    /// feature; the ceiling is a live census and must be lowered in the same
    /// change.
    pub missing_by_frontend: &'a [(FrontendShape, usize)],
    /// `Unregistered` cell ceiling per frontend.
    ///
    /// Removal condition: M3.3 registers a capability identity for every
    /// `PlatformBinding::NotInVocabulary` feature; the ceiling only moves down.
    pub unregistered_per_frontend: usize,
    /// Expected capabilities no feature binds yet.
    ///
    /// Removal condition: binding the identity in
    /// [`FeatureId::platform_binding`], or explicitly declaring it
    /// feature-independent in [`FEATURE_INDEPENDENT_CAPABILITIES`], makes the
    /// entry stale and fails G5 until the baseline shrinks.
    pub unbound_expected_capabilities: &'a [CapabilityId],
}

impl<'a> PlatformGateBaseline<'a> {
    /// A baseline that admits no gap: any count or orphan is a finding.
    pub const ZERO: Self = Self {
        missing_by_frontend: &[],
        unregistered_per_frontend: 0,
        unbound_expected_capabilities: &[],
    };

    /// The `Missing` ceiling for one frontend; `0` when the baseline does not
    /// name it, so a new gap is never silently admitted.
    #[must_use]
    pub fn missing_ceiling(&self, frontend: FrontendShape) -> usize {
        self.missing_by_frontend
            .iter()
            .find(|(shape, _)| *shape == frontend)
            .map_or(0, |(_, ceiling)| *ceiling)
    }
}

/// The authority inputs of the hard gate.
#[derive(Clone, Copy, Debug)]
pub struct PlatformGatePolicy<'a> {
    /// The product-expected capability identity set.
    pub expected_surface: &'a [CapabilityId],
    /// Capabilities explicitly declared feature-independent, with the reason
    /// they have no owning feature.
    pub feature_independent: &'a [(CapabilityId, &'static str)],
    /// The directional baselines.
    pub baseline: &'a PlatformGateBaseline<'a>,
}

/// Evidence anchors that are present but can never prove a behaviour claim.
const PLACEHOLDER_EVIDENCE_ANCHORS: &[&str] = &[
    "-", "--", "0", "n/a", "none", "pending", "tbd", "todo", "unknown",
];

/// Whether `anchor` can back a `Ready` claim: non-empty and not a placeholder.
fn evidence_is_usable(anchor: &str) -> bool {
    let trimmed = anchor.trim();
    !trimmed.is_empty()
        && !PLACEHOLDER_EVIDENCE_ANCHORS
            .iter()
            .any(|placeholder| trimmed.eq_ignore_ascii_case(placeholder))
}

/// The rule for one typed absence record: the status must be one of the four
/// absence projections and the retry disposition must be its authoritative
/// inverse.
fn absence_finding(
    feature: FeatureId,
    frontend: FrontendShape,
    platform: PlatformAxis,
    unavailable: &PlatformUnavailability,
) -> Option<PlatformGateFinding> {
    let detail = if !PlatformSource::is_absence_projection(unavailable.status) {
        "the reason status is a runtime transient, not a static absence"
    } else if unavailable.retry != absence_retry(unavailable.status) {
        "the retry disposition is not the authoritative absence retry"
    } else {
        return None;
    };
    Some(PlatformGateFinding::UntypedReason {
        feature,
        frontend,
        platform,
        detail,
    })
}

/// The per-cell half of the gate: G2, G3, and G4 for one folded cell.
///
/// G1 and G5 are grid/axis invariants and live in
/// [`feature_platform_gate_findings`]; G6 is a ceiling over counted cells.
#[must_use]
pub fn feature_platform_cell_findings(
    cell: &FeaturePlatformCell,
    sources: &PlatformCapabilitySurface,
) -> Vec<PlatformGateFinding> {
    let mut findings = Vec::new();
    let (feature, frontend, platform) = (cell.feature, cell.frontend, cell.platform);
    let capabilities = feature.platform_binding().capabilities();
    let non_delivery_binding = matches!(
        feature.platform_binding(),
        PlatformBinding::NotApplicable(_) | PlatformBinding::NotInVocabulary(_)
    );

    match &cell.status {
        FeaturePlatformStatus::Ready => {
            // G4: a shared-derivation or vocabulary-gap binding never delivers.
            if non_delivery_binding {
                findings.push(PlatformGateFinding::FalseSuccess {
                    feature,
                    frontend,
                    platform,
                    detail: "a non-delivery binding cannot fold to a delivery state",
                });
            }
            // G2: a Ready claim needs a usable anchor; the static source
            // commitment alone is not a behaviour proof.
            if !evidence_is_usable(cell.evidence) {
                findings.push(PlatformGateFinding::ReadyWithoutEvidence {
                    feature,
                    frontend,
                    platform,
                    detail: if cell.evidence.trim().is_empty() {
                        "no behaviour evidence anchor is attached"
                    } else {
                        "the evidence anchor is a placeholder, not a behaviour proof"
                    },
                });
            }
            // G4: Ready requires every bound source to be Present.
            for capability in capabilities {
                if sources.source(platform, capability) != PlatformSource::Present {
                    findings.push(PlatformGateFinding::FalseSuccess {
                        feature,
                        frontend,
                        platform,
                        detail: "a Ready cell requires a Present platform source",
                    });
                    break;
                }
            }
        }
        FeaturePlatformStatus::Gated(status) => {
            if non_delivery_binding {
                findings.push(PlatformGateFinding::FalseSuccess {
                    feature,
                    frontend,
                    platform,
                    detail: "a non-delivery binding cannot fold to a delivery state",
                });
            }
            if !matches!(
                status,
                CapabilityStatus::PermissionRequired | CapabilityStatus::RequiresEscalation
            ) {
                findings.push(PlatformGateFinding::UntypedReason {
                    feature,
                    frontend,
                    platform,
                    detail: "a Gated cell must carry a permission/escalation status",
                });
            }
            if !cell.evidence.trim().is_empty() {
                findings.push(evidence_on_non_delivery(cell));
            }
        }
        FeaturePlatformStatus::Partial(cause) => {
            if non_delivery_binding {
                findings.push(PlatformGateFinding::FalseSuccess {
                    feature,
                    frontend,
                    platform,
                    detail: "a non-delivery binding cannot fold to a delivery state",
                });
            }
            if cause.platform.is_none() && cause.frontend.is_none() {
                findings.push(PlatformGateFinding::PartialWithoutCause {
                    feature,
                    frontend,
                    platform,
                });
            }
            if let Some(status) = cause.platform
                && !PlatformSource::is_absence_projection(status)
            {
                findings.push(PlatformGateFinding::UntypedReason {
                    feature,
                    frontend,
                    platform,
                    detail: "a Partial platform cause must be a static absence projection",
                });
            }
            if let Some(reason) = cause.frontend
                && reason.trim().is_empty()
            {
                findings.push(PlatformGateFinding::EmptyReason {
                    feature,
                    frontend,
                    platform,
                });
            }
        }
        FeaturePlatformStatus::Unsupported(unavailable) => {
            if unavailable.capability.is_none() {
                findings.push(PlatformGateFinding::UntypedReason {
                    feature,
                    frontend,
                    platform,
                    detail: "an Unsupported cell must name the required capability",
                });
            }
            findings.extend(absence_finding(feature, frontend, platform, unavailable));
            findings.extend(evidence_on_non_delivery_if_any(cell));
        }
        FeaturePlatformStatus::Unregistered(unavailable) => {
            findings.extend(absence_finding(feature, frontend, platform, unavailable));
            findings.extend(evidence_on_non_delivery_if_any(cell));
        }
        FeaturePlatformStatus::Missing(reason) => {
            if reason.trim().is_empty() {
                findings.push(PlatformGateFinding::EmptyReason {
                    feature,
                    frontend,
                    platform,
                });
            }
            findings.extend(evidence_on_non_delivery_if_any(cell));
        }
        FeaturePlatformStatus::NotApplicable(reason) => {
            if reason.trim().is_empty() {
                findings.push(PlatformGateFinding::EmptyReason {
                    feature,
                    frontend,
                    platform,
                });
            }
            findings.extend(evidence_on_non_delivery_if_any(cell));
        }
    }
    findings
}

/// G4: a cell that does not claim a delivered behaviour must not carry an
/// anchor that pretends it does.
fn evidence_on_non_delivery_if_any(cell: &FeaturePlatformCell) -> Option<PlatformGateFinding> {
    if cell.evidence.trim().is_empty() {
        None
    } else {
        Some(evidence_on_non_delivery(cell))
    }
}

fn evidence_on_non_delivery(cell: &FeaturePlatformCell) -> PlatformGateFinding {
    PlatformGateFinding::FalseSuccess {
        feature: cell.feature,
        frontend: cell.frontend,
        platform: cell.platform,
        detail: "a non-delivery cell must not carry a behaviour evidence anchor",
    }
}

/// The hard gate: fold-independent grid checks plus every per-cell rule and the
/// directional baselines. An empty result is the only passing state.
#[must_use]
pub fn feature_platform_gate_findings(
    ledger: &FeaturePlatformLedger,
    sources: &PlatformCapabilitySurface,
    policy: &PlatformGatePolicy<'_>,
) -> Vec<PlatformGateFinding> {
    let mut findings = Vec::new();
    let frontends = ledger.frontends();
    if frontends.is_empty() {
        findings.push(PlatformGateFinding::GridEmpty);
    }

    // G1: exactly one cell per (observed frontend, feature, platform).
    let mut counts: BTreeMap<(usize, FeatureId, PlatformAxis), usize> = BTreeMap::new();
    for cell in ledger.iter() {
        let frontend_index = frontend_index(cell.frontend);
        *counts
            .entry((frontend_index, cell.feature, cell.platform))
            .or_default() += 1;
    }
    for frontend in &frontends {
        for feature in FeatureId::ALL {
            for platform in PlatformAxis::ALL {
                let key = (frontend_index(*frontend), *feature, platform);
                match counts.get(&key).copied().unwrap_or(0) {
                    1 => {}
                    0 => findings.push(PlatformGateFinding::GridCellGap {
                        feature: *feature,
                        frontend: *frontend,
                        platform,
                    }),
                    _ => findings.push(PlatformGateFinding::GridCellDuplicate {
                        feature: *feature,
                        frontend: *frontend,
                        platform,
                    }),
                }
            }
        }
    }

    // G2/G3/G4 per cell.
    for cell in ledger.iter() {
        findings.extend(feature_platform_cell_findings(cell, sources));
    }

    // G5: both directions of the capability binding.
    findings.extend(capability_binding_findings(policy));

    // G6: directional ceilings, counted per frontend.
    let mut missing_by_frontend: BTreeMap<usize, usize> = BTreeMap::new();
    let mut unregistered_by_frontend: BTreeMap<usize, usize> = BTreeMap::new();
    for cell in ledger.iter() {
        let slot = frontend_index(cell.frontend);
        match cell.status {
            FeaturePlatformStatus::Missing(_) => {
                *missing_by_frontend.entry(slot).or_default() += 1;
            }
            FeaturePlatformStatus::Unregistered(_) => {
                *unregistered_by_frontend.entry(slot).or_default() += 1;
            }
            _ => {}
        }
    }
    for frontend in &frontends {
        let slot = frontend_index(*frontend);
        let counted_missing = missing_by_frontend.get(&slot).copied().unwrap_or(0);
        let missing_ceiling = policy.baseline.missing_ceiling(*frontend);
        if counted_missing > missing_ceiling {
            findings.push(PlatformGateFinding::MissingCeilingExceeded {
                frontend: *frontend,
                counted: counted_missing,
                ceiling: missing_ceiling,
            });
        }
        let counted_unregistered = unregistered_by_frontend.get(&slot).copied().unwrap_or(0);
        if counted_unregistered > policy.baseline.unregistered_per_frontend {
            findings.push(PlatformGateFinding::UnregisteredCeilingExceeded {
                frontend: *frontend,
                counted: counted_unregistered,
                ceiling: policy.baseline.unregistered_per_frontend,
            });
        }
    }

    findings
}

/// The G5 half: forward (every required identity is product-expected) and
/// reverse (every expected identity is bound, explicitly feature-independent,
/// or accepted baseline debt).
fn capability_binding_findings(policy: &PlatformGatePolicy<'_>) -> Vec<PlatformGateFinding> {
    let mut findings = Vec::new();
    let bound: BTreeSet<CapabilityId> = FeatureId::ALL
        .iter()
        .flat_map(|feature| feature.platform_binding().capabilities().iter().cloned())
        .collect();

    for capability in &bound {
        if !policy.expected_surface.contains(capability) {
            findings.push(PlatformGateFinding::CapabilityOutsideProductSurface {
                capability: capability.clone(),
                detail: "a Requires binding must reference a product-expected identity",
            });
        }
    }

    for (capability, reason) in policy.feature_independent {
        if reason.trim().is_empty() {
            findings.push(PlatformGateFinding::EmptyIndependentReason {
                capability: capability.clone(),
            });
        }
        if !policy.expected_surface.contains(capability) {
            findings.push(PlatformGateFinding::CapabilityOutsideProductSurface {
                capability: capability.clone(),
                detail: "a feature-independent declaration must reference a product-expected identity",
            });
        }
        if bound.contains(capability) {
            findings.push(PlatformGateFinding::StaleCapabilityBaseline {
                capability: capability.clone(),
                detail: "a feature binds the capability; it cannot be feature-independent",
            });
        }
    }

    for capability in policy.baseline.unbound_expected_capabilities {
        if !policy.expected_surface.contains(capability) {
            findings.push(PlatformGateFinding::StaleCapabilityBaseline {
                capability: capability.clone(),
                detail: "the accepted-debt entry left the product surface",
            });
            continue;
        }
        if bound.contains(capability) {
            findings.push(PlatformGateFinding::StaleCapabilityBaseline {
                capability: capability.clone(),
                detail: "the capability is now bound; shrink the baseline in the same change",
            });
            continue;
        }
        if policy
            .feature_independent
            .iter()
            .any(|(candidate, _)| candidate == capability)
        {
            findings.push(PlatformGateFinding::StaleCapabilityBaseline {
                capability: capability.clone(),
                detail: "the capability is explicitly feature-independent; it is no longer debt",
            });
        }
    }

    for capability in policy.expected_surface {
        if bound.contains(capability)
            || policy
                .feature_independent
                .iter()
                .any(|(candidate, _)| candidate == capability)
        {
            continue;
        }
        if policy
            .baseline
            .unbound_expected_capabilities
            .contains(capability)
        {
            continue;
        }
        findings.push(PlatformGateFinding::UnboundExpectedCapability {
            capability: capability.clone(),
        });
    }

    findings
}

/// Canonical slot of a frontend shape for the gate's counting keys.
///
/// The slot is a map key, never an array index, so an unknown shape would be
/// counted under a distinct key instead of panicking; every current variant is
/// registered in [`FrontendShape::ALL`].
fn frontend_index(frontend: FrontendShape) -> usize {
    FrontendShape::ALL
        .iter()
        .position(|shape| *shape == frontend)
        .unwrap_or(FrontendShape::ALL.len())
}

/// Storage for the product-expected identity set so the gate policy can borrow
/// one address instead of re-creating the constant.
///
/// The length is pinned on purpose: expanding `CapabilityId::EXPECTED_SURFACE`
/// fails to compile here until the gate policy is re-checked in the same
/// change.
static PRODUCT_EXPECTED_SURFACE: [CapabilityId; 81] = CapabilityId::EXPECTED_SURFACE;

/// Expected capabilities no feature binds yet: the M1 feature registry is a
/// 75-item representative subset, so this is a FEATURE-AXIS vocabulary gap,
/// not a platform gap and not a capability-level defect.
///
/// The set is a live census: binding one of these identities in
/// [`FeatureId::platform_binding`], or explicitly declaring it
/// feature-independent, fails G5 until the entry leaves this list in the same
/// change. Removal condition: the M1 feature expansion (or a deliberate
/// feature-independent declaration with its reason) covers each identity.
///
/// The M3.3 lowering pass recovered the identities whose owning feature was
/// still declared `NotInVocabulary` (`telemetry.pressure`, `ipc.dbus`,
/// `profiling.pmu`, `memory.vma-map`, `numa.topology`,
/// `threads.context-switch`, `telemetry.cpu.throttle`,
/// `profiling.syscalls`, `ipc.pipe-graph`, `desktop.responsiveness`). The
/// identities that stay here are owned by no current feature: their facts
/// either ride an already-bound lane (for example `process.scheduling` and
/// `filesystem.fd-limits` ride the process row/resource lanes,
/// `power.c-states` rides CPU telemetry) or belong to a surface the 75-item
/// registry has not admitted yet.
static UNBOUND_EXPECTED_CAPABILITIES: &[CapabilityId] = &[
    CapabilityId::TELEMETRY_NETWORK,
    CapabilityId::TELEMETRY_MEMORY_SMBIOS,
    CapabilityId::TELEMETRY_CPU_MSR,
    CapabilityId::CONTAINERS,
    CapabilityId::PROCESS_INSIGHTS_ENVIRONMENT,
    CapabilityId::PROCESS_RESOURCE_CONTROL,
    CapabilityId::PROCESS_NETWORK_ESCALATION,
    CapabilityId::STARTUP,
    CapabilityId::STARTUP_EVIDENCE,
    CapabilityId::STARTUP_CONTROL,
    CapabilityId::SESSIONS,
    CapabilityId::SESSION_CONTROL,
    CapabilityId::STORAGE_HEALTH,
    CapabilityId::DIRECTORY_USAGE,
    CapabilityId::SMART_CONTROL,
    CapabilityId::COMMAND_LAUNCH,
    CapabilityId::RESOURCE_REVEAL,
    CapabilityId::URL_OPEN,
    CapabilityId::DESKTOP_APPEARANCE,
    CapabilityId::DESKTOP_NOTIFY,
    CapabilityId::FIRST_RUN_SETUP,
    CapabilityId::TELEMETRY_GPU_HANG,
    CapabilityId::MEMORY_COMPRESSION,
    CapabilityId::MEMORY_HUGEPAGES,
    CapabilityId::FILESYSTEM_FILE_LOCKS,
    CapabilityId::FILESYSTEM_DELETED_HANDLES,
    CapabilityId::FILESYSTEM_FD_LIMITS,
    CapabilityId::PROCESS_SCHEDULING,
    CapabilityId::PROCESS_OOM_SCORE,
    CapabilityId::THREADS_WAIT_CHANNEL,
    CapabilityId::NETWORK_SOCKET_CONTROL,
    CapabilityId::NETWORK_TRAFFIC_CONTROL,
    CapabilityId::STORAGE_IO_ACCOUNTING,
    CapabilityId::STORAGE_IO_PRIORITY,
    CapabilityId::STORAGE_WRITEBACK,
    CapabilityId::POWER_C_STATES,
    CapabilityId::POWER_PROFILES,
    CapabilityId::SERVICES_TIMERS,
    CapabilityId::SERVICES_SOCKET_ACTIVATION,
    CapabilityId::PROFILING_OFF_CPU,
];

/// The accepted gaps at M3.4, each with its removal condition.
///
/// - `Missing` ceilings are the frontend-axis parity gaps the four shapes
///   already declare explicitly (their own P1 tests pin the same sets);
///   delivering the feature lowers the ceiling.
/// - `unregistered_per_frontend` is the live `NotInVocabulary` census
///   multiplied by the three platforms. The M3.3 lowering pass rebound every
///   feature whose capability identity already existed
///   (`memory.vma-map`, `threads.context-switch`, `network.socket-inventory`,
///   `telemetry.cpu.throttle`, `telemetry.pressure`,
///   `desktop.responsiveness`, `ipc.dbus`, `ipc.pipe-graph`,
///   `profiling.pmu`, `profiling.syscalls`, `numa.topology`), leaving only
///   `tracing.cpu-flame-graph`, whose stack-sampling facts still have no
///   identity; registering one drives the ceiling to zero.
/// - `unbound_expected_capabilities` is the feature-axis vocabulary gap
///   census; it only shrinks.
pub static PLATFORM_GATE_BASELINE: PlatformGateBaseline<'static> = PlatformGateBaseline {
    missing_by_frontend: &[
        (FrontendShape::Gpui, 78),
        (FrontendShape::Iced, 87),
        (FrontendShape::Tui, 78),
        (FrontendShape::Bevy, 96),
    ],
    unregistered_per_frontend: 3,
    unbound_expected_capabilities: UNBOUND_EXPECTED_CAPABILITIES,
};

/// The product gate policy: the single authority for the product-expected
/// surface, the explicit feature-independent declarations, and the baseline.
pub static PLATFORM_GATE_POLICY: PlatformGatePolicy<'static> = PlatformGatePolicy {
    expected_surface: &PRODUCT_EXPECTED_SURFACE,
    feature_independent: FEATURE_INDEPENDENT_CAPABILITIES,
    baseline: &PLATFORM_GATE_BASELINE,
};

#[cfg(test)]
#[path = "../../tests/headless/ui_feature_platform_gate.rs"]
mod tests;
