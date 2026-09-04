//! CORE-04 TUI functional-declaration tests.

use super::functional_declaration;
use taskmanager_ui_contract::{
    FrontendShape, ProductIntent, SurfaceDecision, functional_drift, functional_findings,
    functional_report,
};

#[test]
fn declaration_is_total_and_every_terminal_limit_is_typed() {
    let declaration = functional_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Tui);
    assert_eq!(declaration.entries.len(), ProductIntent::ALL.len());
    assert!(functional_drift(&functional_report(&declaration)).is_empty());
    assert!(functional_findings(&declaration).is_empty());
    assert!(declaration.entries.iter().all(|entry| {
        matches!(
            entry.decision,
            SurfaceDecision::AcceptedDifference { .. }
                | SurfaceDecision::Local { .. }
                | SurfaceDecision::Shared { .. }
                | SurfaceDecision::Unsupported { .. }
        )
    }));
}

#[test]
fn supplied_gpu_and_feedback_mappings_are_accepted_differences() {
    let declaration = functional_declaration();
    for intent in [
        ProductIntent::GpuMetricInspection,
        ProductIntent::TransientFeedback,
    ] {
        let entry = declaration
            .entries
            .iter()
            .find(|entry| entry.intent == intent)
            .expect("supplied CORE-04 intent is registered");
        let expected_route = match intent {
            ProductIntent::GpuMetricInspection => "performance.gpu.metric-cycle",
            ProductIntent::TransientFeedback => "footer.activity-line",
            _ => unreachable!("only the supplied mapping intents reach this branch"),
        };
        assert!(matches!(
            entry.decision,
            SurfaceDecision::AcceptedDifference { route, .. } if route == expected_route
        ));
    }
}

#[test]
fn service_log_export_decision_is_local_surface() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::ServiceLogExport)
        .expect("service log export intent is registered");
    assert!(matches!(
        entry.decision,
        SurfaceDecision::Local { route } if route == "services.log-panel.export"
    ));
}
