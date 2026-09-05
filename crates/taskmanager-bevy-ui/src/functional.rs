//! Bevy's explicit CORE-04 product-intent surface declaration.
//!
//! Bevy is a supported source-build shape but is still below GPUI maturity.
//! Every product intent is therefore declared here: implemented surfaces name
//! their route, while unfinished surfaces remain typed `Unsupported` instead
//! of disappearing from the four-frontend matrix.

use taskmanager_ui_contract::{
    FrontendFunctionalDeclaration, FrontendShape, FunctionalEntry, ProductIntent, SurfaceDecision,
};

/// Declare Bevy's complete product-intent decision set.
#[must_use]
pub fn functional_declaration() -> FrontendFunctionalDeclaration {
    FrontendFunctionalDeclaration {
        frontend: FrontendShape::Bevy,
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
        ProductIntent::AlertRuleAuthoring => SurfaceDecision::Local {
            route: "alerts.page.authoring",
        },
        ProductIntent::AlertRuleTransfer => SurfaceDecision::Local {
            route: "alerts.page.transfer",
        },
        ProductIntent::ActiveAlerts => SurfaceDecision::Local {
            route: "alerts.page.active",
        },
        ProductIntent::AlertEventHistory => SurfaceDecision::Local {
            route: "alerts.page.events",
        },
        ProductIntent::ServiceDetails => SurfaceDecision::Local {
            route: "services.details-modal",
        },
        ProductIntent::ServiceDependencies => SurfaceDecision::Local {
            route: "services.dependencies-panel",
        },
        ProductIntent::ServiceLogs => SurfaceDecision::Local {
            route: "services.log-panel",
        },
        ProductIntent::ServiceLogExport => SurfaceDecision::Local {
            route: "services.log-panel.export",
        },
        ProductIntent::ProcessAffinityEditor => SurfaceDecision::Local {
            route: "processes.affinity-modal",
        },
        ProductIntent::SmartSelfTest => SurfaceDecision::Local {
            route: "performance.disk.smart-self-test",
        },
        ProductIntent::DiagnosticBundle => SurfaceDecision::AcceptedDifference {
            route: "about.diagnostic-report",
            reason: "Bevy provides a redacted clipboard report while GPUI provides the preview/write bundle workflow",
        },
        ProductIntent::CurrentWindowScreenshot => SurfaceDecision::Local {
            route: "header.screenshot",
        },
        ProductIntent::FirstRunSetup => SurfaceDecision::Local {
            route: "first-run.dialog",
        },
        ProductIntent::GpuMetricInspection => SurfaceDecision::AcceptedDifference {
            route: "performance.gpu.metric-summary",
            reason: "Bevy renders the available GPU metric families together in its device cards instead of exposing a multi-level engine selector",
        },
        ProductIntent::TransientFeedback => SurfaceDecision::AcceptedDifference {
            route: "root.feedback-line",
            reason: "transient feedback renders through the Bevy shell feedback line instead of a floating toast",
        },
    }
}

#[cfg(test)]
#[path = "../tests/headless/functional.rs"]
mod tests;
