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
        ProductIntent::AlertRuleAuthoring => SurfaceDecision::Unsupported {
            reason: "the Bevy shape exposes rule toggles only; rule authoring is not wired",
        },
        ProductIntent::AlertRuleTransfer => SurfaceDecision::Unsupported {
            reason: "the Bevy shape has no alert-rule import/export surface",
        },
        ProductIntent::ActiveAlerts => SurfaceDecision::Local {
            route: "alerts.page.active",
        },
        ProductIntent::AlertEventHistory => SurfaceDecision::Unsupported {
            reason: "the Bevy notification-history surface is not wired",
        },
        ProductIntent::ServiceDetails => SurfaceDecision::Unsupported {
            reason: "the Bevy shape currently exposes service inventory only",
        },
        ProductIntent::ServiceDependencies => SurfaceDecision::Unsupported {
            reason: "the Bevy service-dependency panel is not wired",
        },
        ProductIntent::ServiceLogs => SurfaceDecision::Local {
            route: "services.log-panel",
        },
        ProductIntent::ServiceLogExport => SurfaceDecision::Local {
            route: "services.log-panel.export",
        },
        ProductIntent::ProcessAffinityEditor => SurfaceDecision::Unsupported {
            reason: "the Bevy process-details surface has no affinity editor",
        },
        ProductIntent::SmartSelfTest => SurfaceDecision::Local {
            route: "performance.disk.smart-self-test",
        },
        ProductIntent::DiagnosticBundle => SurfaceDecision::Unsupported {
            reason: "the Bevy shape has no diagnostic-bundle surface",
        },
        ProductIntent::CurrentWindowScreenshot => SurfaceDecision::Unsupported {
            reason: "the Bevy shape has no current-window PNG capture surface",
        },
        ProductIntent::FirstRunSetup => SurfaceDecision::Unsupported {
            reason: "the Bevy first-run setup route is not wired",
        },
        ProductIntent::GpuMetricInspection => SurfaceDecision::Unsupported {
            reason: "the Bevy GPU metric-inspection detail surface is not wired",
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
