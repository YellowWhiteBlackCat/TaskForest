//! GPUI's CORE-04 functional-intent declaration.
//!
//! GPUI owns the reference execution for every product intent. The actual
//! behavior remains in the shared application/shell contracts; these routes
//! only make the reference surface explicit and keep new intents exhaustive.

use taskmanager_ui_contract::{
    FrontendFunctionalDeclaration, FunctionalEntry, ProductIntent, SurfaceDecision,
};

/// Declare the GPUI reference surface for every CORE-04 product intent.
#[must_use]
pub fn functional_declaration() -> FrontendFunctionalDeclaration {
    FrontendFunctionalDeclaration {
        frontend: taskmanager_ui_contract::FrontendShape::Gpui,
        entries: ProductIntent::ALL
            .into_iter()
            .map(|intent| FunctionalEntry {
                intent,
                decision: SurfaceDecision::Reference {
                    route: reference_route(intent),
                },
            })
            .collect(),
    }
}

const fn reference_route(intent: ProductIntent) -> &'static str {
    match intent {
        ProductIntent::AlertRuleToggle => "dashboard.alert-rules.toggle",
        ProductIntent::AlertRuleAuthoring => "dashboard.alert-rules.editor",
        ProductIntent::AlertRuleTransfer => "dashboard.alert-rules.transfer",
        ProductIntent::ActiveAlerts => "dashboard.active-alerts",
        ProductIntent::AlertEventHistory => "dashboard.events",
        ProductIntent::ServiceDetails => "services.details",
        ProductIntent::ServiceDependencies => "services.details.dependencies",
        ProductIntent::ServiceLogs => "services.details.logs",
        ProductIntent::ServiceLogExport => "services.details.log-export",
        ProductIntent::ProcessAffinityEditor => "processes.affinity",
        ProductIntent::SmartSelfTest => "system.smart-self-test",
        ProductIntent::DiagnosticBundle => "system.diagnostic-bundle",
        ProductIntent::CurrentWindowScreenshot => "root.current-window-screenshot",
        ProductIntent::FirstRunSetup => "first-run.setup",
        ProductIntent::GpuMetricInspection => "performance.gpu.metric-families",
        ProductIntent::TransientFeedback => "root.toast",
    }
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_functional_tests.rs"]
mod tests;
