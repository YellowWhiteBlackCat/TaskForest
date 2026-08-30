use super::*;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_escalation::polkit::{
    DmiIdentityFacts, SmbiosHelperError, SmbiosHelperErrorKind, SmbiosMemorySuccess,
    SmbiosModuleReading,
};

fn module(slot: u32) -> SmbiosModuleReading {
    SmbiosModuleReading {
        slot,
        size_mb: Some(32_768),
        speed_mts: Some(5600),
        configured_speed_mts: Some(5200),
        manufacturer: Some("Samsung".to_owned()),
        serial_number: Some("SERIAL-1".to_owned()),
        part_number: Some("M471A4G43MB1".to_owned()),
        form_factor: Some("SODIMM".to_owned()),
        memory_type: Some("DDR5".to_owned()),
        locator: Some(format!("ChannelA-DIMM{slot}")),
    }
}

fn identity() -> DmiIdentityFacts {
    DmiIdentityFacts {
        bios_vendor: Some("AMI".to_owned()),
        bios_version: Some("P1.27".to_owned()),
        bios_date: Some("04/17/2024".to_owned()),
        board_manufacturer: Some("ASUSTeK".to_owned()),
        board_product: Some("X670E".to_owned()),
        board_serial: Some("MB-SN-1".to_owned()),
        board_asset_tag: Some("ASSET-42".to_owned()),
        system_manufacturer: Some("LENOVO".to_owned()),
        system_product: Some("21JX".to_owned()),
        system_serial: Some("PF3XYZ42".to_owned()),
        system_uuid: Some("4c4c4544-0042-3510-8054-b7c04f4d3532".to_owned()),
        system_sku: Some("SKU-AB".to_owned()),
        system_family: Some("ThinkPad".to_owned()),
    }
}

#[test]
fn success_outcome_maps_to_real_module_rows() {
    let outcome = SmbiosHelperOutcome::Success(Box::new(SmbiosMemorySuccess {
        schema: 1,
        slots_total: 4,
        slots_used: 2,
        modules: vec![module(0), module(1)],
        identity: None,
    }));
    let snapshot = result_from_outcome(outcome).expect("success outcome");
    assert!(snapshot.is_success());
    assert_eq!(snapshot.slots_total, 4);
    assert_eq!(snapshot.slots_used, 2);
    assert_eq!(snapshot.modules.len(), 2);
    assert_eq!(snapshot.modules[0].slot, 0);
    assert_eq!(snapshot.modules[0].size_mb, Some(32_768));
    assert_eq!(snapshot.modules[0].configured_speed_mts, Some(5200));
    assert_eq!(
        snapshot.modules[1].locator.as_deref(),
        Some("ChannelA-DIMM1")
    );
    assert_eq!(snapshot.identity, None, "a null identity stays absent");
}

#[test]
fn success_identity_maps_field_by_field_into_the_core_fact() {
    let outcome = SmbiosHelperOutcome::Success(Box::new(SmbiosMemorySuccess {
        schema: 1,
        slots_total: 2,
        slots_used: 0,
        modules: Vec::new(),
        identity: Some(identity()),
    }));
    let snapshot = result_from_outcome(outcome).expect("success outcome");
    let mapped = snapshot.identity.as_ref().expect("identity mapped");
    assert_eq!(mapped.bios_vendor.as_deref(), Some("AMI"));
    assert_eq!(mapped.board_asset_tag.as_deref(), Some("ASSET-42"));
    assert_eq!(mapped.system_serial.as_deref(), Some("PF3XYZ42"));
    assert_eq!(
        mapped.system_uuid.as_deref(),
        Some("4c4c4544-0042-3510-8054-b7c04f4d3532")
    );
    assert_eq!(mapped.system_sku.as_deref(), Some("SKU-AB"));
    assert_eq!(mapped.system_family.as_deref(), Some("ThinkPad"));
    assert_eq!(mapped.board_serial.as_deref(), Some("MB-SN-1"));
}

#[test]
fn helper_error_maps_to_provider_health_not_an_ok_failure_snapshot() {
    let outcome = SmbiosHelperOutcome::HelperError(SmbiosHelperError {
        kind: SmbiosHelperErrorKind::NoDmi,
        detail: "no DMI entries tree".to_owned(),
    });
    assert_eq!(
        result_from_outcome(outcome),
        Err(ProviderFailure::Unsupported),
        "a provider failure must update runtime health instead of hiding inside Ok(snapshot)",
    );
    assert_eq!(
        result_from_outcome(SmbiosHelperOutcome::HelperError(SmbiosHelperError {
            kind: SmbiosHelperErrorKind::PermissionDenied,
            detail: "still root-only".to_owned(),
        })),
        Err(ProviderFailure::PermissionDenied),
    );
    assert_eq!(
        result_from_outcome(SmbiosHelperOutcome::HelperError(SmbiosHelperError {
            kind: SmbiosHelperErrorKind::ReadFailed,
            detail: "raw record read failed".to_owned(),
        })),
        Err(ProviderFailure::ProviderFault),
    );
}

#[test]
fn unavailable_reasons_map_to_their_typed_kinds() {
    let cases = [
        (
            taskmanager_escalation::EscalationDenialReason::PermissionDenied,
            FailureKind::PermissionDenied,
        ),
        (
            taskmanager_escalation::EscalationDenialReason::HelperUnavailable,
            FailureKind::MissingDependency,
        ),
        (
            taskmanager_escalation::EscalationDenialReason::AuthorizationUnavailable,
            FailureKind::TemporarilyUnavailable,
        ),
        (
            taskmanager_escalation::EscalationDenialReason::HelperProtocolViolation,
            FailureKind::ProviderFault,
        ),
        (
            taskmanager_escalation::EscalationDenialReason::Unsupported,
            FailureKind::Unsupported,
        ),
    ];
    for (reason, expected_kind) in cases {
        let result = result_from_outcome(SmbiosHelperOutcome::Unavailable {
            reason,
            detail: "fixture".to_owned(),
        });
        assert_eq!(
            result.map_err(ProviderFailure::kind),
            Err(expected_kind),
            "no fabricated Ok failure snapshot",
        );
    }
}

static HELPER_INVOCATIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn missing_crossing() -> EscalationAvailability {
    EscalationAvailability::Denied {
        reason: EscalationDenialReason::HelperUnavailable,
    }
}

fn counted_helper() -> SmbiosHelperOutcome {
    HELPER_INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    panic!("a provider must not invoke pkexec when exact readiness failed")
}

#[test]
fn missing_crossing_fails_fast_without_launching_pkexec() {
    HELPER_INVOCATIONS.store(0, std::sync::atomic::Ordering::SeqCst);
    let mut provider = NativeSmbiosMemoryProvider::with_crossing(missing_crossing, counted_helper);
    assert_eq!(
        provider.initial_status(),
        CapabilityStatus::MissingDependency
    );

    assert_eq!(
        provider.read_memory_smbios(),
        Err(ProviderFailure::MissingDependency)
    );
    assert_eq!(
        HELPER_INVOCATIONS.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}
