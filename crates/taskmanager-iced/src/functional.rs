//! Iced's CORE-04 functional-intent declaration.
//!
//! The declaration is intentionally separate from the component registry:
//! it records product-intent execution, not widget parity. Every cell either
//! consumes shared semantic state, names an Iced-local surface, records an
//! accepted surface difference, or refuses the intent explicitly.

use taskmanager_ui_contract::{
    FrontendFunctionalDeclaration, FrontendShape, FunctionalEntry, ProductIntent, SurfaceDecision,
};

/// Declare Iced's complete CORE-04 surface decision set.
#[must_use]
pub fn functional_declaration() -> FrontendFunctionalDeclaration {
    FrontendFunctionalDeclaration {
        frontend: FrontendShape::Iced,
        entries: ProductIntent::ALL
            .into_iter()
            .map(|intent| FunctionalEntry {
                intent,
                decision: decision(intent),
            })
            .collect(),
    }
}

const fn decision(intent: ProductIntent) -> SurfaceDecision {
    match intent {
        ProductIntent::AlertRuleToggle => SurfaceDecision::Local {
            route: "alerts.page.rule-toggle",
        },
        ProductIntent::AlertRuleAuthoring => SurfaceDecision::Unsupported {
            reason: "the Iced product shape exposes rule toggles only; rule add/update/remove authoring remains a GPUI reference surface",
        },
        ProductIntent::AlertRuleTransfer => SurfaceDecision::Unsupported {
            reason: "the Iced product shape does not offer alert-rule import/export; the shared rule contract remains available to future surface work",
        },
        ProductIntent::ActiveAlerts => SurfaceDecision::Shared {
            route: "shell.alert-active",
        },
        ProductIntent::AlertEventHistory => SurfaceDecision::Local {
            route: "alerts.overlay.event-history",
        },
        ProductIntent::ServiceDetails => SurfaceDecision::Local {
            route: "service-details.modal",
        },
        ProductIntent::ServiceDependencies => SurfaceDecision::Shared {
            route: "shell.service-dependencies",
        },
        ProductIntent::ServiceLogs => SurfaceDecision::Local {
            route: "service-details.log-lines",
        },
        ProductIntent::ServiceLogExport => SurfaceDecision::Local {
            route: "service-log.export",
        },
        ProductIntent::ProcessAffinityEditor => SurfaceDecision::Local {
            route: "processes.affinity-modal",
        },
        ProductIntent::SmartSelfTest => SurfaceDecision::Unsupported {
            reason: "the Iced product shape exposes SMART observation only; SMART control remains a GPUI reference surface",
        },
        ProductIntent::DiagnosticBundle => SurfaceDecision::AcceptedDifference {
            route: "about.diagnostic-report",
            reason: "Iced provides a redacted clipboard report while GPUI provides the preview/write bundle workflow",
        },
        ProductIntent::FirstRunSetup => SurfaceDecision::Local {
            route: "first-run.dialog",
        },
        ProductIntent::GpuMetricInspection => SurfaceDecision::AcceptedDifference {
            route: "performance.gpu.all-families",
            reason: "Iced renders every available GPU metric family together instead of exposing a selector",
        },
        ProductIntent::TransientFeedback => SurfaceDecision::AcceptedDifference {
            route: "footer.activity-line",
            reason: "transient feedback uses the shared footer activity line instead of a floating toast",
        },
    }
}

#[cfg(test)]
#[path = "../tests/gui/functional_tests.rs"]
mod tests;
