use super::*;
use taskforest_core::core::startup::{
    StartupControlPolicy, StartupEntryId, StartupEntryLocator, StartupImpact,
    StartupImpactUnknownReason, StartupScope, StartupSource,
};

#[test]
fn startup_source_and_impact_text_format_consistently() {
    let measured_entry = StartupEntry {
        id: StartupEntryId::new("test-app"),
        name: "Test App".to_string(),
        locator: StartupEntryLocator::new("test-app.desktop"),
        exec: "test-app --daemon".to_string(),
        enabled: true,
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        impact: StartupImpact::Medium,
        impact_evidence: StartupImpactEvidence::Measured { duration_ms: 120 },
        control_policy: StartupControlPolicy::Direct,
    };

    let source = startup_source_text(&measured_entry);
    assert!(source.contains("Desktop Entry"));

    let impact = startup_impact_text(&measured_entry);
    assert!(impact.contains("120 ms"));

    let unmeasured_entry = StartupEntry {
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
        ..measured_entry
    };

    let unmeasured_impact = startup_impact_text(&unmeasured_entry);
    assert!(!unmeasured_impact.contains("ms"));
}
