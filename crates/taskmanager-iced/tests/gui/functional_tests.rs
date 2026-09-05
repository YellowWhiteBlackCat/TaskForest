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
    assert_eq!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::AlertRuleTransfer)
            .expect("transfer intent is registered")
            .decision,
        SurfaceDecision::Local {
            route: "alerts.page.transfer",
        }
    );
    assert_eq!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::AlertRuleAuthoring)
            .expect("authoring intent is registered")
            .decision,
        SurfaceDecision::Local {
            route: "alerts.page.authoring",
        }
    );
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

#[test]
fn current_window_screenshot_intent_is_mapped_to_local_header_route() {
    let declaration = functional_declaration();
    let screenshot = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::CurrentWindowScreenshot)
        .expect("current window screenshot intent is registered");
    assert_eq!(
        screenshot.decision,
        SurfaceDecision::Local {
            route: "header.screenshot",
        }
    );
}

#[test]
fn smart_self_test_intent_is_mapped_to_local_storage_route() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::SmartSelfTest)
        .expect("SMART self-test intent is registered");
    assert_eq!(
        entry.decision,
        SurfaceDecision::Local {
            route: "storage.smart-self-test",
        }
    );
}

#[test]
fn service_intents_are_mapped_to_modal_and_shared_routes() {
    let declaration = functional_declaration();
    assert_eq!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::ServiceDetails)
            .expect("ServiceDetails registered")
            .decision,
        SurfaceDecision::Local {
            route: "service-details.modal",
        }
    );
    assert_eq!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::ServiceDependencies)
            .expect("ServiceDependencies registered")
            .decision,
        SurfaceDecision::Shared {
            route: "shell.service-dependencies",
        }
    );
    assert_eq!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::ServiceLogs)
            .expect("ServiceLogs registered")
            .decision,
        SurfaceDecision::Local {
            route: "service-details.log-lines",
        }
    );
    assert_eq!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::ServiceLogExport)
            .expect("ServiceLogExport registered")
            .decision,
        SurfaceDecision::Local {
            route: "service-log.export",
        }
    );
}

#[test]
fn process_affinity_editor_intent_is_mapped_to_local_modal_route() {
    let declaration = functional_declaration();
    assert_eq!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::ProcessAffinityEditor)
            .expect("ProcessAffinityEditor registered")
            .decision,
        SurfaceDecision::Local {
            route: "processes.affinity-modal",
        }
    );
}

#[test]
fn diagnostic_bundle_is_accepted_difference() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::DiagnosticBundle)
        .expect("DiagnosticBundle registered");
    assert!(matches!(
        entry.decision,
        SurfaceDecision::AcceptedDifference {
            route: "about.diagnostic-report",
            ..
        }
    ));
}

#[test]
fn first_run_setup_intent_is_mapped_to_local_dialog_route() {
    let declaration = functional_declaration();
    assert_eq!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::FirstRunSetup)
            .expect("FirstRunSetup registered")
            .decision,
        SurfaceDecision::Local {
            route: "first-run.dialog",
        }
    );
}

#[test]
fn active_alerts_and_event_history_intents_are_mapped() {
    let declaration = functional_declaration();
    assert_eq!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::ActiveAlerts)
            .expect("ActiveAlerts registered")
            .decision,
        SurfaceDecision::Shared {
            route: "shell.alert-active",
        }
    );
    assert_eq!(
        declaration
            .entries
            .iter()
            .find(|entry| entry.intent == ProductIntent::AlertEventHistory)
            .expect("AlertEventHistory registered")
            .decision,
        SurfaceDecision::Local {
            route: "alerts.overlay.event-history",
        }
    );
}

#[test]
fn all_sixteen_intents_have_explicit_and_valid_routes() {
    let declaration = functional_declaration();
    for intent in ProductIntent::ALL {
        let entry = declaration
            .entries
            .iter()
            .find(|e| e.intent == intent)
            .unwrap_or_else(|| panic!("intent {intent:?} must be registered"));
        let route = entry
            .decision
            .route()
            .unwrap_or_else(|| panic!("intent {intent:?} must have a route"));
        assert!(
            !route.is_empty(),
            "intent {intent:?} route must not be empty"
        );
    }
}
