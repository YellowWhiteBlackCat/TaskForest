use super::*;

/// A `perf_event_open` `EACCES` (restrictive `perf_event_paranoid`) surfaces
/// as `ErrorKind::PermissionDenied`. Under the default unprivileged gate the
/// Intel PMU denial MUST type as `RequiresEscalation` — not a bare
/// `PermissionDenied` — so a consumer can offer the per-feature Intel PMU
/// prompt (ADR-023) rather than treating it as a hard wall.
#[test]
fn intel_pmu_permission_denial_is_escalation_aware() {
    let denial = io::Error::from(io::ErrorKind::PermissionDenied);
    assert_eq!(
        classify_intel_pmu_open_failure(&denial),
        FailureKind::RequiresEscalation,
        "an Intel PMU perf_event_open EACCES must type as RequiresEscalation \
             under the default unprivileged gate",
    );
}

/// A non-denial open failure keeps its honest non-escalation classification,
/// so the escalation signal is never fabricated for an unrelated cause.
#[test]
fn intel_pmu_non_denial_open_failures_keep_their_honest_kind() {
    // NotFound → the `missing` fallback passed to gpu_io_failure (Unsupported).
    let not_found = io::Error::from(io::ErrorKind::NotFound);
    assert_eq!(
        classify_intel_pmu_open_failure(&not_found),
        FailureKind::Unsupported,
    );
    // A plain OS error that is not a denial/missing/timeout falls through to
    // TemporarilyUnavailable — unchanged, not escalation-aware.
    let other = io::Error::from(io::ErrorKind::UnexpectedEof);
    assert_eq!(
        classify_intel_pmu_open_failure(&other),
        FailureKind::TemporarilyUnavailable,
    );
}

/// The gate consulted at the denial point is the honest default: probing the
/// Intel PMU feature yields `RequiresEscalation(IntelPmu)`, never `Available`
/// (would fabricate access) and never `Denied` (would hide the prompt).
#[test]
fn unprivileged_gate_reports_intel_pmu_as_requires_escalation() {
    match UnprivilegedGate.probe(EscalationFeature::IntelPmu) {
        EscalationAvailability::RequiresEscalation(EscalationFeature::IntelPmu) => {}
        other => panic!("UnprivilegedGate must require escalation for IntelPmu, got {other:?}"),
    }
}
