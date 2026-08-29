use taskmanager_ui_contract::{
    CapabilityStatus, ComponentCapability, FrontendShape, capability_findings, capability_report,
};

#[test]
fn capability_declaration_is_complete_and_has_no_reference_claim() {
    let declaration = crate::capabilities::capability_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Bevy);
    assert!(capability_findings(&declaration).is_empty());

    let report = capability_report(&declaration);
    assert_eq!(report.len(), ComponentCapability::ALL.len());
    assert!(report.iter().all(|(_, status)| matches!(
        status,
        CapabilityStatus::Declared(taskmanager_ui_contract::CapabilitySupport::Ported)
            | CapabilityStatus::Declared(taskmanager_ui_contract::CapabilitySupport::Native { .. })
            | CapabilityStatus::Declared(
                taskmanager_ui_contract::CapabilitySupport::Divergent { .. }
            )
            | CapabilityStatus::Declared(
                taskmanager_ui_contract::CapabilitySupport::Unsupported { .. }
            )
    )));
}
