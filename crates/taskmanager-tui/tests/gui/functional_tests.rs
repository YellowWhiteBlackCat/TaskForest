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

#[test]
fn service_dependencies_decision_is_local_surface() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::ServiceDependencies)
        .expect("service dependencies intent is registered");
    assert_eq!(
        entry.decision,
        SurfaceDecision::Local {
            route: "services.details.dependencies",
        }
    );
}

#[test]
fn process_affinity_editor_decision_is_local_surface() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::ProcessAffinityEditor)
        .expect("process affinity editor intent is registered");
    assert_eq!(
        entry.decision,
        SurfaceDecision::Local {
            route: "processes.affinity-modal",
        }
    );
}

#[test]
fn alert_rule_toggle_decision_is_local_surface() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::AlertRuleToggle)
        .expect("alert rule toggle intent is registered");
    assert_eq!(
        entry.decision,
        SurfaceDecision::Local {
            route: "health.alert-rules.toggle",
        }
    );
}

#[test]
fn service_details_decision_is_local_surface() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::ServiceDetails)
        .expect("service details intent is registered");
    assert_eq!(
        entry.decision,
        SurfaceDecision::Local {
            route: "services.details-column",
        }
    );
}

#[test]
fn alert_event_history_decision_is_local_surface() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::AlertEventHistory)
        .expect("alert event history intent is registered");
    assert_eq!(
        entry.decision,
        SurfaceDecision::Local {
            route: "health.events",
        }
    );
}

#[test]
fn alert_rule_authoring_decision_is_local_surface() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::AlertRuleAuthoring)
        .expect("authoring intent is registered");
    assert_eq!(
        entry.decision,
        SurfaceDecision::Local {
            route: "health.alert-rules.authoring",
        }
    );
}

#[test]
fn alert_rule_transfer_decision_is_local_surface() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::AlertRuleTransfer)
        .expect("transfer intent is registered");
    assert_eq!(
        entry.decision,
        SurfaceDecision::Local {
            route: "health.alerts.transfer",
        }
    );
}

#[test]
fn active_alerts_decision_is_local_surface() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::ActiveAlerts)
        .expect("active alerts intent is registered");
    assert_eq!(
        entry.decision,
        SurfaceDecision::Local {
            route: "health.active-alerts",
        }
    );
}

#[test]
fn service_logs_decision_is_local_surface() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::ServiceLogs)
        .expect("service logs intent is registered");
    assert_eq!(
        entry.decision,
        SurfaceDecision::Local {
            route: "services.log-panel",
        }
    );
}

#[test]
fn smart_self_test_decision_is_local_surface() {
    let declaration = functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|entry| entry.intent == ProductIntent::SmartSelfTest)
        .expect("smart self test intent is registered");
    assert_eq!(
        entry.decision,
        SurfaceDecision::Local {
            route: "performance.disk.smart-self-test",
        }
    );
}

#[test]
fn unsupported_intents_are_explicit_and_have_honest_reasons() {
    let declaration = functional_declaration();
    for intent in [
        ProductIntent::DiagnosticBundle,
        ProductIntent::CurrentWindowScreenshot,
        ProductIntent::FirstRunSetup,
    ] {
        let entry = declaration
            .entries
            .iter()
            .find(|entry| entry.intent == intent)
            .unwrap_or_else(|| panic!("{intent:?} must be registered"));
        match entry.decision {
            SurfaceDecision::Unsupported { reason } => {
                assert!(
                    !reason.trim().is_empty(),
                    "reason for {intent:?} must not be empty"
                );
            }
            other => panic!("expected {intent:?} to be Unsupported, got {other:?}"),
        }
    }
}
