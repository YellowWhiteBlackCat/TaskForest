use super::*;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_escalation::polkit::{
    RaplHelperError, RaplHelperErrorKind, RaplPackageReading, RaplPowerSuccess,
};

#[test]
fn success_outcome_maps_to_real_package_rows() {
    let outcome = RaplHelperOutcome::Success(RaplPowerSuccess {
        schema: 1,
        sample_ms: 250,
        packages: vec![RaplPackageReading {
            name: "package-1".to_owned(),
            power_w: 12.5,
            energy_delta_uj: 3_125_000,
        }],
    });
    let snapshot = result_from_outcome(outcome).expect("success outcome");
    assert!(snapshot.is_success());
    assert_eq!(snapshot.sample_ms, 250);
    assert_eq!(snapshot.packages.len(), 1);
    assert_eq!(snapshot.packages[0].name, "package-1");
    assert_eq!(snapshot.packages[0].power_w, 12.5);
    assert_eq!(snapshot.packages[0].energy_delta_uj, 3_125_000);
}

#[test]
fn helper_error_maps_to_provider_health_not_an_ok_failure_snapshot() {
    let outcome = RaplHelperOutcome::HelperError(RaplHelperError {
        kind: RaplHelperErrorKind::NoRapl,
        detail: "no intel-rapl packages".to_owned(),
    });
    assert_eq!(
        result_from_outcome(outcome),
        Err(ProviderFailure::Unsupported),
        "a provider failure must update runtime health instead of hiding inside Ok(snapshot)",
    );
    assert_eq!(
        result_from_outcome(RaplHelperOutcome::HelperError(RaplHelperError {
            kind: RaplHelperErrorKind::PermissionDenied,
            detail: "energy_uj is root-only".to_owned(),
        })),
        Err(ProviderFailure::PermissionDenied),
    );
    assert_eq!(
        result_from_outcome(RaplHelperOutcome::HelperError(RaplHelperError {
            kind: RaplHelperErrorKind::OpenFailed,
            detail: "powercap open failed".to_owned(),
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
        let result = result_from_outcome(RaplHelperOutcome::Unavailable {
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

fn counted_helper() -> RaplHelperOutcome {
    HELPER_INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    panic!("a provider must not invoke pkexec when exact readiness failed")
}

#[test]
fn missing_crossing_fails_fast_without_launching_pkexec() {
    HELPER_INVOCATIONS.store(0, std::sync::atomic::Ordering::SeqCst);
    let mut provider = NativeRaplPowerProvider::with_crossing(missing_crossing, counted_helper);
    assert_eq!(
        provider.initial_status(),
        CapabilityStatus::MissingDependency
    );

    assert_eq!(
        provider.read_package_power(),
        Err(ProviderFailure::MissingDependency)
    );
    assert_eq!(
        HELPER_INVOCATIONS.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}
