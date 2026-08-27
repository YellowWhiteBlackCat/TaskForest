use super::*;
use taskmanager_escalation::polkit::{EngineReading, PerfHelperError, PerfHelperErrorKind};
use taskmanager_platform_contract::FailureKind;

fn device() -> DeviceId {
    DeviceId::new("gpu:0")
}

#[test]
fn success_outcome_maps_to_real_rows() {
    let outcome = PerfHelperOutcome::Success(taskmanager_escalation::polkit::PerfHelperSuccess {
        schema: 1,
        driver: "xe".to_owned(),
        sample_ms: 250,
        engines: vec![
            EngineReading {
                name: "Render Ring".to_owned(),
                class: "rcs".to_owned(),
                busy_pct: 41.5,
            },
            EngineReading {
                name: "Blitter".to_owned(),
                class: "bcs".to_owned(),
                busy_pct: 0.0,
            },
        ],
    });
    let snapshot = result_from_outcome(outcome, device()).expect("success outcome");
    assert!(snapshot.is_success());
    assert_eq!(snapshot.engines.len(), 2);
    assert_eq!(snapshot.engines[0].name, "Render Ring");
    assert_eq!(snapshot.engines[0].utilization_pct, 41.5);
    // Unmapped i915 engine classes stay Unknown — no guessed semantics.
    assert_eq!(snapshot.engines[0].kind, GpuEngineKind::Unknown);
    assert_eq!(snapshot.engines[1].kind, GpuEngineKind::Unknown);
}

#[test]
fn helper_error_maps_to_provider_health_not_an_ok_failure_snapshot() {
    let outcome = PerfHelperOutcome::HelperError(PerfHelperError {
        kind: PerfHelperErrorKind::NoPmu,
        detail: "no PMU on this host".to_owned(),
    });
    assert_eq!(
        result_from_outcome(outcome, device()),
        Err(ProviderFailure::Unsupported),
        "a provider failure must update runtime health instead of hiding inside Ok(snapshot)",
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
        let result = result_from_outcome(
            PerfHelperOutcome::Unavailable {
                reason,
                detail: "fixture".to_owned(),
            },
            device(),
        );
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

fn counted_helper() -> PerfHelperOutcome {
    HELPER_INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    panic!("a provider must not invoke pkexec when exact readiness failed")
}

#[test]
fn missing_crossing_fails_fast_without_launching_pkexec() {
    HELPER_INVOCATIONS.store(0, std::sync::atomic::Ordering::SeqCst);
    let mut provider = NativeGpuEngineRowsProvider::with_crossing(missing_crossing, counted_helper);
    assert_eq!(
        provider.initial_status(),
        CapabilityStatus::MissingDependency
    );

    assert_eq!(
        provider.read_engine_rows(&device()),
        Err(ProviderFailure::MissingDependency)
    );
    assert_eq!(
        HELPER_INVOCATIONS.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}
