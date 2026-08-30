use super::*;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_escalation::polkit::{
    MsrHelperError, MsrHelperErrorKind, MsrPackageReading, MsrReadoutSuccess,
};

#[test]
fn success_outcome_maps_to_real_per_node_rows() {
    let outcome = MsrHelperOutcome::Success(MsrReadoutSuccess {
        schema: 1,
        packages: vec![
            MsrPackageReading {
                cpu: 0,
                bclk_mhz: None,
                temperature_c: Some(54.5),
                multiplier: Some(42.0),
                multiplier_min: Some(8.0),
                multiplier_max: Some(58.0),
                vcore_v: Some(1.219),
            },
            MsrPackageReading {
                cpu: 1,
                bclk_mhz: None,
                temperature_c: None,
                multiplier: None,
                multiplier_min: None,
                multiplier_max: None,
                vcore_v: None,
            },
        ],
    });
    let snapshot = result_from_outcome(outcome).expect("success outcome");
    assert!(snapshot.is_success());
    assert_eq!(snapshot.packages.len(), 2);
    assert_eq!(snapshot.packages[0].cpu, 0);
    assert_eq!(snapshot.packages[0].temperature_c, Some(54.5));
    assert_eq!(snapshot.packages[0].vcore_v, Some(1.219));
    // A register the CPU does not implement stays typed-absent, never zero.
    assert_eq!(snapshot.packages[1].temperature_c, None);
    assert_eq!(snapshot.packages[1].vcore_v, None);
}

#[test]
fn helper_error_maps_to_provider_health_not_an_ok_failure_snapshot() {
    let outcome = MsrHelperOutcome::HelperError(MsrHelperError {
        kind: MsrHelperErrorKind::NoMsr,
        detail: "no /dev/cpu tree".to_owned(),
    });
    assert_eq!(
        result_from_outcome(outcome),
        Err(ProviderFailure::Unsupported),
        "a provider failure must update runtime health instead of hiding inside Ok(snapshot)",
    );
    assert_eq!(
        result_from_outcome(MsrHelperOutcome::HelperError(MsrHelperError {
            kind: MsrHelperErrorKind::PermissionDenied,
            detail: "msr nodes are root-only".to_owned(),
        })),
        Err(ProviderFailure::PermissionDenied),
    );
    assert_eq!(
        result_from_outcome(MsrHelperOutcome::HelperError(MsrHelperError {
            kind: MsrHelperErrorKind::ReadFailed,
            detail: "msr node read failed".to_owned(),
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
        let result = result_from_outcome(MsrHelperOutcome::Unavailable {
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

fn counted_helper() -> MsrHelperOutcome {
    HELPER_INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    panic!("a provider must not invoke pkexec when exact readiness failed")
}

#[test]
fn missing_crossing_fails_fast_without_launching_pkexec() {
    HELPER_INVOCATIONS.store(0, std::sync::atomic::Ordering::SeqCst);
    let mut provider = NativeMsrReadoutProvider::with_crossing(missing_crossing, counted_helper);
    assert_eq!(
        provider.initial_status(),
        CapabilityStatus::MissingDependency
    );

    assert_eq!(
        provider.read_msr_readouts(),
        Err(ProviderFailure::MissingDependency)
    );
    assert_eq!(
        HELPER_INVOCATIONS.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}
