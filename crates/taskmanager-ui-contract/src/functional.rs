//! Cross-frontend product-intent matrix (CORE-04).
//!
//! This is deliberately separate from the component capability registry and
//! from a renderer's interaction matrix. It describes the user intent that
//! must remain semantically common, the layer that owns that meaning, the
//! lifecycle/identity discipline it requires, and each frontend's explicit
//! surface decision. Adding a new intent requires updating the exhaustive
//! frontend mappings, so a new product feature cannot silently disappear from
//! one shape.

use crate::keybindings::FrontendShape;
use taskmanager_platform_contract::CapabilityId;

/// A stable user intent in the shared product contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProductIntent {
    AlertRuleToggle,
    AlertRuleAuthoring,
    AlertRuleTransfer,
    ActiveAlerts,
    AlertEventHistory,
    ServiceDetails,
    ServiceDependencies,
    ServiceLogs,
    ServiceLogExport,
    ProcessAffinityEditor,
    SmartSelfTest,
    DiagnosticBundle,
    FirstRunSetup,
    GpuMetricInspection,
    TransientFeedback,
}

impl ProductIntent {
    /// Canonical matrix order. Frontend declarations and reports fold against
    /// this list, never against a hand-maintained per-shape list.
    pub const ALL: [Self; 15] = [
        Self::AlertRuleToggle,
        Self::AlertRuleAuthoring,
        Self::AlertRuleTransfer,
        Self::ActiveAlerts,
        Self::AlertEventHistory,
        Self::ServiceDetails,
        Self::ServiceDependencies,
        Self::ServiceLogs,
        Self::ServiceLogExport,
        Self::ProcessAffinityEditor,
        Self::SmartSelfTest,
        Self::DiagnosticBundle,
        Self::FirstRunSetup,
        Self::GpuMetricInspection,
        Self::TransientFeedback,
    ];

    /// Stable machine name used by matrix reports and evidence manifests.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::AlertRuleToggle => "alerts.rule-toggle",
            Self::AlertRuleAuthoring => "alerts.rule-authoring",
            Self::AlertRuleTransfer => "alerts.rule-transfer",
            Self::ActiveAlerts => "alerts.active",
            Self::AlertEventHistory => "alerts.event-history",
            Self::ServiceDetails => "services.details",
            Self::ServiceDependencies => "services.dependencies",
            Self::ServiceLogs => "services.logs",
            Self::ServiceLogExport => "services.logs.export",
            Self::ProcessAffinityEditor => "process.affinity-editor",
            Self::SmartSelfTest => "storage.smart-self-test",
            Self::DiagnosticBundle => "diagnostics.bundle",
            Self::FirstRunSetup => "first-run.setup",
            Self::GpuMetricInspection => "gpu.metric-inspection",
            Self::TransientFeedback => "feedback.transient",
        }
    }

    /// The canonical owner and lifecycle contract for this intent. These are
    /// semantic facts, not frontend implementation hints.
    #[must_use]
    pub const fn spec(self) -> ProductIntentSpec {
        use ContractLayer::{Application, Shell};
        use IntentFamily::{Alerts, Diagnostics, Feedback, Gpu, Process, Services, Setup, Storage};
        use IntentLifecycle::{
            Projection, PureReducer, RequestResponse, Stream, TransitionHistory,
        };
        use TargetContract::{
            DeviceGeneration, FrozenProcessIdentity, None, RequestCorrelation, RuleSet, ServiceId,
            StableRuleId, StorageDeviceGeneration,
        };

        let (family, owner, lifecycle, target) = match self {
            Self::AlertRuleToggle => (Alerts, Application, PureReducer, StableRuleId),
            Self::AlertRuleAuthoring => (Alerts, Application, PureReducer, RuleSet),
            Self::AlertRuleTransfer => (Alerts, Application, PureReducer, RuleSet),
            Self::ActiveAlerts => (Alerts, Application, Projection, None),
            Self::AlertEventHistory => (Alerts, Application, TransitionHistory, None),
            Self::ServiceDetails => (Services, Application, RequestResponse, ServiceId),
            Self::ServiceDependencies => (Services, Application, RequestResponse, ServiceId),
            Self::ServiceLogs => (Services, Application, Stream, ServiceId),
            Self::ServiceLogExport => (Services, Application, RequestResponse, ServiceId),
            Self::ProcessAffinityEditor => {
                (Process, Application, RequestResponse, FrozenProcessIdentity)
            }
            Self::SmartSelfTest => (
                Storage,
                Application,
                RequestResponse,
                StorageDeviceGeneration,
            ),
            Self::DiagnosticBundle => (
                Diagnostics,
                Application,
                RequestResponse,
                RequestCorrelation,
            ),
            Self::FirstRunSetup => (Setup, Application, RequestResponse, RequestCorrelation),
            Self::GpuMetricInspection => (Gpu, Shell, Projection, DeviceGeneration),
            Self::TransientFeedback => (Feedback, Shell, Projection, None),
        };
        ProductIntentSpec {
            intent: self,
            family,
            owner,
            lifecycle,
            target,
        }
    }

    /// Platform capabilities required by this intent, when it crosses one or
    /// more native request ports. Pure rule/projection intents deliberately
    /// return an empty vector; they must not grow a fake provider dependency.
    #[must_use]
    pub fn required_platform_capabilities(self) -> Vec<CapabilityId> {
        match self {
            Self::AlertRuleToggle
            | Self::AlertRuleAuthoring
            | Self::AlertRuleTransfer
            | Self::ActiveAlerts
            | Self::AlertEventHistory
            | Self::ServiceLogExport
            | Self::GpuMetricInspection
            | Self::TransientFeedback
            | Self::DiagnosticBundle => Vec::new(),
            Self::ServiceDetails => vec![CapabilityId::SERVICES],
            Self::ServiceDependencies => {
                vec![CapabilityId::SERVICE_DEPENDENCIES]
            }
            Self::ServiceLogs => vec![CapabilityId::SERVICE_LOGS, CapabilityId::SERVICE_LOG_STREAM],
            Self::ProcessAffinityEditor => vec![
                CapabilityId::PROCESS_AFFINITY,
                CapabilityId::PROCESS_AFFINITY_CONTROL,
            ],
            Self::SmartSelfTest => vec![CapabilityId::SMART_CONTROL],
            Self::FirstRunSetup => vec![CapabilityId::FIRST_RUN_SETUP],
        }
    }
}

/// Broad intent family used to keep matrix reports readable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntentFamily {
    Alerts,
    Services,
    Process,
    Storage,
    Diagnostics,
    Setup,
    Gpu,
    Feedback,
}

/// The lowest shared layer that owns the intent's semantic truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContractLayer {
    Core,
    Application,
    Shell,
}

/// Lifecycle discipline required before a renderer can expose the intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntentLifecycle {
    PureReducer,
    Projection,
    TransitionHistory,
    RequestResponse,
    Stream,
}

/// Identity discipline for the intent's target, if it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetContract {
    None,
    RequestCorrelation,
    StableRuleId,
    RuleSet,
    ServiceId,
    FrozenProcessIdentity,
    StorageDeviceGeneration,
    DeviceGeneration,
}

/// The non-frontend part of one matrix row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProductIntentSpec {
    pub intent: ProductIntent,
    pub family: IntentFamily,
    pub owner: ContractLayer,
    pub lifecycle: IntentLifecycle,
    pub target: TargetContract,
}

/// One frontend's explicit execution decision for a product intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceDecision {
    /// The GPUI reference surface owns the reference execution.
    Reference { route: &'static str },
    /// The shape uses the shared route/projection without semantic reshaping.
    Shared { route: &'static str },
    /// The shape uses a local surface over the shared intent contract.
    Local { route: &'static str },
    /// The same intent is met through a deliberately different surface.
    AcceptedDifference {
        route: &'static str,
        reason: &'static str,
    },
    /// The product intent is not offered by this shape for a typed reason.
    Unsupported { reason: &'static str },
}

impl SurfaceDecision {
    #[must_use]
    pub const fn route(self) -> Option<&'static str> {
        match self {
            Self::Reference { route }
            | Self::Shared { route }
            | Self::Local { route }
            | Self::AcceptedDifference { route, .. } => Some(route),
            Self::Unsupported { .. } => None,
        }
    }

    #[must_use]
    pub const fn explanation(self) -> Option<&'static str> {
        match self {
            Self::AcceptedDifference { reason, .. } | Self::Unsupported { reason } => Some(reason),
            Self::Reference { .. } | Self::Shared { .. } | Self::Local { .. } => None,
        }
    }

    #[must_use]
    pub const fn is_reference(self) -> bool {
        matches!(self, Self::Reference { .. })
    }
}

/// One declared intent-to-surface pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionalEntry {
    pub intent: ProductIntent,
    pub decision: SurfaceDecision,
}

/// A frontend's total declaration against [`ProductIntent::ALL`].
#[derive(Clone, Debug)]
pub struct FrontendFunctionalDeclaration {
    pub frontend: FrontendShape,
    pub entries: Vec<FunctionalEntry>,
}

/// The result of folding a declaration against the canonical intent set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionalStatus {
    Declared(SurfaceDecision),
    Missing,
    Duplicated,
    Unknown,
}

impl FunctionalStatus {
    #[must_use]
    pub const fn is_explicit(self) -> bool {
        matches!(self, Self::Declared(_))
    }

    #[must_use]
    pub const fn is_drift(self) -> bool {
        !self.is_explicit()
    }
}

/// One reason a functional declaration cannot be accepted by the gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionalFindingKind {
    Missing,
    Duplicated,
    Unknown,
    ReferenceOutsideReferenceShape,
    ReferenceShapeCannotDefer,
    EmptyRoute,
    EmptyExplanation,
}

/// One machine-readable matrix finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionalFinding {
    pub frontend: FrontendShape,
    pub intent: ProductIntent,
    pub kind: FunctionalFindingKind,
}

/// Fold one frontend declaration against the complete CORE-04 intent set.
#[must_use]
pub fn functional_report(
    declaration: &FrontendFunctionalDeclaration,
) -> Vec<(ProductIntent, FunctionalStatus)> {
    let mut report: Vec<(ProductIntent, FunctionalStatus)> = ProductIntent::ALL
        .into_iter()
        .map(|intent| (intent, FunctionalStatus::Missing))
        .collect();
    for entry in &declaration.entries {
        match report
            .iter_mut()
            .find(|(intent, _)| *intent == entry.intent)
        {
            Some((_, status)) => {
                match status {
                    FunctionalStatus::Missing => {
                        *status = FunctionalStatus::Declared(entry.decision);
                    }
                    FunctionalStatus::Declared(_)
                    | FunctionalStatus::Duplicated
                    | FunctionalStatus::Unknown => {
                        // Keep a second declaration as drift. The Unknown
                        // arm matters when a future ProductIntent is added
                        // but omitted from ALL: repeating that declaration
                        // must not accidentally turn it into an accepted cell.
                        *status = FunctionalStatus::Duplicated;
                    }
                }
            }
            None => report.push((entry.intent, FunctionalStatus::Unknown)),
        }
    }
    report
}

/// Return only declaration omissions, duplicates, and unknown intents.
#[must_use]
pub fn functional_drift(
    report: &[(ProductIntent, FunctionalStatus)],
) -> Vec<(ProductIntent, FunctionalStatus)> {
    report
        .iter()
        .copied()
        .filter(|(_, status)| status.is_drift())
        .collect()
}

/// Apply the CORE-04 role, route, and explanation rules to one declaration.
#[must_use]
pub fn functional_findings(declaration: &FrontendFunctionalDeclaration) -> Vec<FunctionalFinding> {
    let mut findings = functional_report(declaration)
        .into_iter()
        .filter_map(|(intent, status)| {
            let kind = match status {
                FunctionalStatus::Missing => Some(FunctionalFindingKind::Missing),
                FunctionalStatus::Duplicated => Some(FunctionalFindingKind::Duplicated),
                FunctionalStatus::Unknown => Some(FunctionalFindingKind::Unknown),
                FunctionalStatus::Declared(_) => None,
            }?;
            Some(FunctionalFinding {
                frontend: declaration.frontend,
                intent,
                kind,
            })
        })
        .collect::<Vec<_>>();

    for (intent, status) in functional_report(declaration) {
        let FunctionalStatus::Declared(decision) = status else {
            continue;
        };
        let kind = if decision.is_reference()
            && !declaration.frontend.is_functional_reference_shape()
        {
            Some(FunctionalFindingKind::ReferenceOutsideReferenceShape)
        } else if declaration.frontend.is_functional_reference_shape() && !decision.is_reference() {
            Some(FunctionalFindingKind::ReferenceShapeCannotDefer)
        } else if decision.route().is_some_and(str::is_empty) {
            Some(FunctionalFindingKind::EmptyRoute)
        } else if decision.explanation().is_some_and(str::is_empty) {
            Some(FunctionalFindingKind::EmptyExplanation)
        } else {
            None
        };
        if let Some(kind) = kind {
            findings.push(FunctionalFinding {
                frontend: declaration.frontend,
                intent,
                kind,
            });
        }
    }
    findings.sort_by_key(|finding| finding.intent);
    findings.dedup();
    findings
}

impl FrontendShape {
    /// GPUI is the product reference shape for CORE-04, just as it is for the
    /// component registry. Other shapes declare ports or deliberate surface
    /// decisions; they cannot silently claim reference ownership.
    #[must_use]
    pub const fn is_functional_reference_shape(self) -> bool {
        matches!(self, Self::Gpui)
    }
}

#[cfg(test)]
#[path = "../tests/headless/ui_functional.rs"]
mod tests;
