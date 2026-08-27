use super::super::evidence::{StartupImpactEvidence, StartupImpactUnknownReason};
use super::{
    StartupControlPolicy, StartupEntry, StartupEntryId, StartupEntryLocator, StartupImpact,
    StartupScope, StartupSource,
};

#[test]
fn neutral_startup_sources_decode_and_preserve_legacy_wire_values() {
    for (wire, source) in [
        ("XdgAutostart", StartupSource::DesktopEntry),
        ("SystemdUser", StartupSource::UserService),
        ("OpenRcRunlevel", StartupSource::RunLevel),
    ] {
        let decoded: StartupSource =
            serde_json::from_str(&format!("\"{wire}\"")).expect("legacy startup source");
        assert_eq!(decoded, source);
        assert_eq!(
            serde_json::to_string(&source).expect("startup source wire"),
            format!("\"{wire}\"")
        );
    }

    assert_eq!(
        serde_json::from_str::<StartupSource>("\"desktop_entry\"").expect("neutral startup source"),
        StartupSource::DesktopEntry
    );
    assert_eq!(
        serde_json::from_str::<StartupSource>("\"user_service\"").expect("neutral startup source"),
        StartupSource::UserService
    );
    assert_eq!(
        serde_json::from_str::<StartupSource>("\"run_level\"").expect("neutral startup source"),
        StartupSource::RunLevel
    );
}

#[test]
fn typed_locator_preserves_the_legacy_handle_wire_key() {
    let entry = StartupEntry {
        id: StartupEntryId::new("user-service:fixture.service"),
        name: "fixture".into(),
        exec: "fixture".into(),
        enabled: true,
        source: StartupSource::UserService,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: StartupEntryLocator::new("fixture.service"),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    };
    let json = serde_json::to_value(&entry).expect("startup entry wire");
    assert_eq!(json["handle"], "fixture.service");
    assert!(json.get("locator").is_none());

    let decoded: StartupEntry = serde_json::from_value(json).expect("legacy startup entry");
    assert_eq!(decoded.locator.as_str(), "fixture.service");
    assert_eq!(decoded.id.as_str(), "user-service:fixture.service");
    assert_eq!(decoded.scope, StartupScope::User);
    assert_eq!(decoded.control_policy, StartupControlPolicy::Direct);
}

#[test]
fn legacy_rows_without_authority_metadata_fail_closed() {
    let decoded: StartupEntry = serde_json::from_value(serde_json::json!({
        "name": "legacy",
        "exec": "legacy",
        "enabled": true,
        "source": "SystemdUser",
        "handle": "legacy.service",
        "impact": "None",
        "impact_evidence": {
            "Unknown": { "reason": "NotInstrumented" }
        }
    }))
    .expect("legacy startup row");

    assert_eq!(decoded.scope, StartupScope::Unknown);
    assert_eq!(decoded.control_policy, StartupControlPolicy::Unsupported);
    assert!(decoded.id.is_empty());
}
