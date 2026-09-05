use taskmanager_ui_contract::{
    FrontendShape, FunctionalStatus, ProductIntent, SurfaceDecision, functional_findings,
    functional_report,
};

#[test]
fn functional_declaration_is_complete_and_explicit() {
    let declaration = crate::functional::functional_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Bevy);
    assert!(functional_findings(&declaration).is_empty());

    let report = functional_report(&declaration);
    assert_eq!(report.len(), ProductIntent::ALL.len());
    assert!(
        report
            .iter()
            .all(|(_, status)| matches!(status, FunctionalStatus::Declared(_)))
    );
    assert!(matches!(
        report
            .iter()
            .find(|(intent, _)| *intent == ProductIntent::AlertRuleToggle)
            .map(|(_, status)| status),
        Some(FunctionalStatus::Declared(SurfaceDecision::Local { .. }))
    ));
    assert_eq!(
        report
            .iter()
            .find(|(intent, _)| *intent == ProductIntent::SmartSelfTest)
            .map(|(_, status)| status),
        Some(&FunctionalStatus::Declared(SurfaceDecision::Local {
            route: "performance.disk.smart-self-test",
        }))
    );
    assert_eq!(
        report
            .iter()
            .find(|(intent, _)| *intent == ProductIntent::ServiceLogExport)
            .map(|(_, status)| status),
        Some(&FunctionalStatus::Declared(SurfaceDecision::Local {
            route: "services.log-panel.export",
        }))
    );
}
