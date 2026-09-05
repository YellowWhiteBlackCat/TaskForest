use taskmanager_application::CommandId;
use taskmanager_ui_contract::{CoverageStatus, FrontendShape, coverage_report, drift_findings};

#[test]
fn binding_declaration_is_complete_for_bevy() {
    let declaration = crate::bindings::binding_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Bevy);

    let report = coverage_report(&declaration);
    assert!(drift_findings(&report).is_empty(), "{report:?}");
    assert_eq!(report.len(), CommandId::ALL.len());
    assert_eq!(
        report
            .iter()
            .find(|(command, _)| *command == CommandId::ToggleSidebar)
            .map(|(_, status)| *status),
        Some(CoverageStatus::Bound("F9"))
    );
}
