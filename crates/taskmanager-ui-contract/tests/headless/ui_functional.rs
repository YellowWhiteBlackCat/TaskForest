//! CORE-04 product-intent matrix fold tests.

use super::*;

fn entry(intent: ProductIntent, decision: SurfaceDecision) -> FunctionalEntry {
    FunctionalEntry { intent, decision }
}

fn reference_declaration(frontend: FrontendShape) -> FrontendFunctionalDeclaration {
    FrontendFunctionalDeclaration {
        frontend,
        entries: ProductIntent::ALL
            .into_iter()
            .map(|intent| entry(intent, SurfaceDecision::Reference { route: "reference" }))
            .collect(),
    }
}

#[test]
fn product_intent_registry_is_unique_and_described() {
    let ids: Vec<_> = ProductIntent::ALL
        .iter()
        .map(|intent| intent.id())
        .collect();
    assert_eq!(ids.len(), 16);
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len());

    for intent in ProductIntent::ALL {
        let spec = intent.spec();
        assert_eq!(spec.intent, intent);
        assert!(!intent.id().is_empty());
    }
}

#[test]
fn platform_request_intents_name_their_native_capability() {
    let dependencies = ProductIntent::ServiceDependencies.required_platform_capabilities();
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].as_str(), "services.dependencies");
    let logs = ProductIntent::ServiceLogs.required_platform_capabilities();
    assert_eq!(
        logs.iter()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>(),
        ["services.logs", "services.logs.stream"]
    );
    let affinity = ProductIntent::ProcessAffinityEditor.required_platform_capabilities();
    assert_eq!(
        affinity
            .iter()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>(),
        ["process.affinity", "process.affinity.control"]
    );
    assert_eq!(
        ProductIntent::SmartSelfTest.required_platform_capabilities()[0].as_str(),
        "storage.smart.control"
    );
    assert_eq!(
        ProductIntent::DiagnosticBundle.required_platform_capabilities(),
        Vec::new()
    );
    assert_eq!(
        ProductIntent::DiagnosticBundle.spec().target,
        TargetContract::RequestCorrelation
    );
    assert_eq!(
        ProductIntent::CurrentWindowScreenshot.required_platform_capabilities(),
        Vec::new()
    );
    assert_eq!(
        ProductIntent::CurrentWindowScreenshot.spec().target,
        TargetContract::None
    );
    assert_eq!(
        ProductIntent::AlertRuleAuthoring.spec().target,
        TargetContract::RuleSet
    );
}

#[test]
fn total_reference_declaration_has_no_findings() {
    let declaration = reference_declaration(FrontendShape::Gpui);
    assert!(functional_drift(&functional_report(&declaration)).is_empty());
    assert!(functional_findings(&declaration).is_empty());
}

#[test]
fn missing_and_duplicate_intents_are_drift() {
    let mut declaration = reference_declaration(FrontendShape::Iced);
    declaration.entries.pop();
    declaration.entries.push(entry(
        ProductIntent::AlertRuleToggle,
        SurfaceDecision::Shared { route: "alerts" },
    ));
    declaration.entries.push(entry(
        ProductIntent::AlertRuleToggle,
        SurfaceDecision::Shared {
            route: "alerts-again",
        },
    ));
    let findings = functional_findings(&declaration);
    assert!(findings.iter().any(|finding| {
        finding.kind == FunctionalFindingKind::Missing
            || finding.kind == FunctionalFindingKind::Duplicated
    }));
}

#[test]
fn reference_role_and_explanations_are_enforced() {
    for frontend in [FrontendShape::Iced, FrontendShape::Tui, FrontendShape::Bevy] {
        let stolen = reference_declaration(frontend);
        assert!(
            functional_findings(&stolen).iter().all(
                |finding| finding.kind == FunctionalFindingKind::ReferenceOutsideReferenceShape
            ),
            "non-reference frontend {frontend:?} must not claim the GPUI reference role"
        );
    }

    let mut deferred = reference_declaration(FrontendShape::Gpui);
    deferred.entries[0] = entry(
        ProductIntent::AlertRuleToggle,
        SurfaceDecision::Unsupported {
            reason: "not ready",
        },
    );
    assert!(
        functional_findings(&deferred)
            .iter()
            .any(|finding| { finding.kind == FunctionalFindingKind::ReferenceShapeCannotDefer })
    );

    let mut empty = reference_declaration(FrontendShape::Iced);
    empty.entries[0] = entry(
        ProductIntent::AlertRuleToggle,
        SurfaceDecision::AcceptedDifference {
            route: "",
            reason: "surface differs",
        },
    );
    assert!(
        functional_findings(&empty)
            .iter()
            .any(|finding| finding.kind == FunctionalFindingKind::EmptyRoute)
    );

    empty.entries[0] = entry(
        ProductIntent::AlertRuleToggle,
        SurfaceDecision::Unsupported { reason: "" },
    );
    assert!(
        functional_findings(&empty)
            .iter()
            .any(|finding| finding.kind == FunctionalFindingKind::EmptyExplanation)
    );
}
