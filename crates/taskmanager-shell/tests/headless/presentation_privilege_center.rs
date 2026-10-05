use super::*;
use taskmanager_platform_contract::RequestId;

#[test]
fn unobserved_capabilities_never_render_enabled_or_offer_authorization() {
    let shell = ShellApp::new();
    let rows = PrivilegeCenterInputs::from_shell(&shell).rows();
    assert_eq!(rows.len(), 4);
    assert!(
        rows.iter()
            .all(|row| row.state == PrivilegeRowState::Unavailable && row.action.is_none())
    );
}

#[test]
fn permission_gate_and_escalation_offer_have_different_actions() {
    let shell = ShellApp::new();
    let mut inputs = PrivilegeCenterInputs::from_shell(&shell);
    inputs.smbios_capability = Some(CapabilityStatus::PermissionRequired);
    inputs.rapl_capability = Some(CapabilityStatus::RequiresEscalation);
    let rows = inputs.rows();
    assert_eq!(rows[1].state, PrivilegeRowState::NeedsAuthorization);
    assert_eq!(rows[1].action, None);
    assert_eq!(rows[2].state, PrivilegeRowState::NeedsAuthorization);
    assert_eq!(rows[2].action, Some(PrivilegeAction::RaplPower));
    assert!(matches!(
        rows[2].action.as_ref().expect("offered").effect(),
        PlatformEffect::RaplPower(_)
    ));
}

#[test]
fn an_admitted_helper_read_renders_authorizing_and_rejection_stays_visible() {
    let mut shell = ShellApp::new();
    let attempt = shell.begin_smbios_memory_request();
    assert!(shell.accept_smbios_memory_request(attempt, RequestId::new(1).expect("request")));
    assert_eq!(
        PrivilegeCenterInputs::from_shell(&shell).rows()[1].state,
        PrivilegeRowState::Authorizing
    );
    let attempt = shell.begin_smbios_memory_request();
    assert!(shell.reject_smbios_memory_request(attempt, FailureKind::PermissionDenied));
    assert_eq!(
        PrivilegeCenterInputs::from_shell(&shell).rows()[1].state,
        PrivilegeRowState::Denied
    );
}

#[test]
fn gpu_authorization_freezes_the_observed_device_identity() {
    let shell = ShellApp::new();
    let mut inputs = PrivilegeCenterInputs::from_shell(&shell);
    let id = DeviceId::new("gpu:stable-fixture");
    inputs.gpu_engine_device_id = Some(id.clone());
    inputs.gpu_engine_capability = Some(CapabilityStatus::RequiresEscalation);
    let row = inputs.rows().remove(0);
    assert_eq!(row.action, Some(PrivilegeAction::GpuEngines(id.clone())));
    assert!(
        matches!(row.action.expect("offered").effect(), PlatformEffect::GpuEngineRows(request) if request.device_id == id)
    );
}

#[test]
fn failure_mapping_preserves_the_seven_permission_states() {
    assert_eq!(
        state_from_failure(FailureKind::RequiresEscalation),
        PrivilegeRowState::NeedsAuthorization
    );
    assert_eq!(
        state_from_failure(FailureKind::PermissionDenied),
        PrivilegeRowState::Denied
    );
    assert_eq!(
        state_from_failure(FailureKind::Unsupported),
        PrivilegeRowState::Unsupported
    );
    assert_eq!(
        state_from_failure(FailureKind::ProviderFault),
        PrivilegeRowState::Failed
    );
    assert_eq!(
        state_from_failure(FailureKind::TimedOut),
        PrivilegeRowState::Unavailable
    );
}

#[test]
fn capability_mapping_keeps_unsupported_separate_from_unavailable() {
    assert_eq!(
        capability_state(Some(CapabilityStatus::Unsupported)),
        Some(PrivilegeRowState::Unsupported)
    );
    assert_eq!(
        capability_state(Some(CapabilityStatus::TemporarilyUnavailable)),
        Some(PrivilegeRowState::Unavailable)
    );
    assert_eq!(
        capability_state(Some(CapabilityStatus::Degraded(FailureKind::ProviderFault))),
        Some(PrivilegeRowState::Failed)
    );
}
