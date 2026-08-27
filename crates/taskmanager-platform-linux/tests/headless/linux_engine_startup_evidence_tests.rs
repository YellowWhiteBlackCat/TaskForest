use super::*;

#[test]
fn failed_unit_parser_preserves_typed_states_and_description() {
    let rows = parse_systemd_failed_units(
        "broken.service loaded failed failed Broken worker service\n\
             other.timer loaded failed failed Periodic task\n",
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].unit, "broken.service");
    assert_eq!(rows[0].active_state, "failed");
    assert_eq!(rows[0].description, "Broken worker service");
}

#[test]
fn critical_chain_parser_preserves_activation_and_duration() {
    let rows = parse_systemd_critical_chain(
        "graphical-session.target @1.500s\n\
             └─app.service @900ms +300ms\n\
               └─socket.socket @250ms\n",
    );
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].activated_at_ms, Some(1_500));
    assert_eq!(rows[1].unit, "app.service");
    assert_eq!(rows[1].activated_at_ms, Some(900));
    assert_eq!(rows[1].duration_ms, Some(300));
    assert_eq!(rows[2].duration_ms, None);
}

#[test]
fn openrc_and_unknown_init_never_borrow_systemd_evidence_semantics() {
    for init in [InitSystem::Openrc, InitSystem::Unsupported] {
        let snapshot = unavailable_for_init(Ok(init), 42)
            .expect("non-systemd init must stop before command construction");
        assert_eq!(snapshot.state.status, DeviceStatus::Unsupported);
        assert_eq!(
            snapshot.failed_units_failure,
            Some(StartupEvidenceFailure::Unsupported)
        );
        assert_eq!(
            snapshot.critical_chain_failure,
            Some(StartupEvidenceFailure::Unsupported)
        );
        assert!(snapshot.failed_units.is_empty());
        assert!(snapshot.critical_chain.is_empty());
    }
    assert!(unavailable_for_init(Ok(InitSystem::Systemd), 42).is_none());
}

#[test]
fn init_probe_failures_remain_typed_in_evidence_state() {
    for (failure, expected, status) in [
        (
            FailureKind::MissingDependency,
            StartupEvidenceFailure::MissingTool,
            DeviceStatus::MissingTool,
        ),
        (
            FailureKind::PermissionDenied,
            StartupEvidenceFailure::PermissionDenied,
            DeviceStatus::PermissionDenied,
        ),
        (
            FailureKind::TimedOut,
            StartupEvidenceFailure::TimedOut,
            DeviceStatus::Stale,
        ),
    ] {
        let snapshot =
            unavailable_for_init(Err(failure), 7).expect("failed probes must be unavailable");
        assert_eq!(snapshot.state.status, status);
        assert_eq!(snapshot.failed_units_failure, Some(expected));
        assert_eq!(snapshot.critical_chain_failure, Some(expected));
    }
}
