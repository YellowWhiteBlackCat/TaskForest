//! TUI's CORE-04 functional-intent declaration.
//!
//! Terminal limits are explicit decisions, never silent omissions. The
//! shared application/shell intent still remains the authority for every
//! supported path; a terminal-only surface may reshape the interaction but
//! cannot redefine the result.

use taskmanager_ui_contract::{
    FrontendFunctionalDeclaration, FrontendShape, FunctionalEntry, ProductIntent, SurfaceDecision,
};

/// Declare TUI's complete CORE-04 surface decision set.
#[must_use]
pub fn functional_declaration() -> FrontendFunctionalDeclaration {
    FrontendFunctionalDeclaration {
        frontend: FrontendShape::Tui,
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
        ProductIntent::AlertRuleToggle => SurfaceDecision::Unsupported {
            reason: "the terminal product shape exposes alert status but keeps rule mutation out of its health surface",
        },
        ProductIntent::AlertRuleAuthoring => SurfaceDecision::Unsupported {
            reason: "the terminal product shape does not offer an alert-rule authoring form",
        },
        ProductIntent::AlertRuleTransfer => SurfaceDecision::Unsupported {
            reason: "the terminal product shape does not offer alert-rule import/export",
        },
        ProductIntent::ActiveAlerts => SurfaceDecision::Local {
            route: "health.active-alerts",
        },
        ProductIntent::AlertEventHistory => SurfaceDecision::Unsupported {
            reason: "the terminal product shape keeps the compact health surface to active alert status; event-center history is graphical",
        },
        ProductIntent::ServiceDetails => SurfaceDecision::Unsupported {
            reason: "the terminal product shape does not offer the graphical service-details panel",
        },
        ProductIntent::ServiceDependencies => SurfaceDecision::Local {
            route: "services.details.dependencies",
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
        ProductIntent::DiagnosticBundle => SurfaceDecision::Unsupported {
            reason: "the terminal product shape has no diagnostic-bundle preview or export surface",
        },
        ProductIntent::CurrentWindowScreenshot => SurfaceDecision::Unsupported {
            reason: "the terminal product shape does not expose a compositor current-window PNG capture control",
        },
        ProductIntent::FirstRunSetup => SurfaceDecision::Unsupported {
            reason: "first-run setup is owned by graphical composition and has no terminal surface",
        },
        ProductIntent::GpuMetricInspection => SurfaceDecision::AcceptedDifference {
            route: "performance.gpu.metric-cycle",
            reason: "TUI uses a keyboard cycle while GPUI and Iced render all available metric families",
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
