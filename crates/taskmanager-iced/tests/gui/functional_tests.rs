//! CORE-04 Iced functional-declaration tests.

use super::functional_declaration;
use taskmanager_ui_contract::{
    FrontendShape, ProductIntent, SurfaceDecision, functional_drift, functional_findings,
    functional_report,
};

#[test]
fn declaration_is_total_and_explicit() {
    let declaration = functional_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Iced);
    assert_eq!(declaration.entries.len(), ProductIntent::ALL.len());
    assert!(functional_drift(&functional_report(&declaration)).is_empty());
    assert!(functional_findings(&declaration).is_empty());
}

#[test]
fn alert_rule_intents_are_split_by_user_operation() {
    let declaration = functional_declaration();
    assert!(matches!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::AlertRuleToggle)
            .expect("toggle intent is registered")
            .decision,
        SurfaceDecision::Local { .. }
    ));
    assert!(matches!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::AlertRuleAuthoring)
            .expect("authoring intent is registered")
            .decision,
        SurfaceDecision::Unsupported { .. }
    ));
}

#[test]
fn supplied_gpu_and_feedback_mappings_are_accepted_differences() {
    let declaration = functional_declaration();
    let gpu = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::GpuMetricInspection)
        .expect("GPU inspection intent is registered");
    assert!(matches!(
        gpu.decision,
        SurfaceDecision::AcceptedDifference {
            route: "performance.gpu.all-families",
            ..
        }
    ));
    let feedback = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::TransientFeedback)
        .expect("feedback intent is registered");
    assert!(matches!(
        feedback.decision,
        SurfaceDecision::AcceptedDifference {
            route: "footer.activity-line",
            ..
        }
    ));
}
