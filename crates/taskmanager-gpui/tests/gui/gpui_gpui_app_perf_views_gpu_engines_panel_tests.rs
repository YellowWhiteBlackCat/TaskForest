use super::*;
use taskmanager_application::GpuEngineRowsSession;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{GpuEngineKind, GpuEngineRowsSnapshot};
use taskmanager_platform_contract::RequestId;

fn device() -> DeviceId {
    DeviceId::new("gpu:0")
}

fn sample_engines() -> Vec<GpuEngineMetric> {
    vec![
        GpuEngineMetric {
            name: "Render Ring".to_owned(),
            kind: GpuEngineKind::Unknown,
            utilization_pct: 42.5,
        },
        GpuEngineMetric {
            name: "Blitter".to_owned(),
            kind: GpuEngineKind::Copy,
            utilization_pct: 0.0,
        },
    ]
}

fn accept_snapshot(
    session: &mut GpuEngineRowsSession,
    request: u64,
    snapshot: GpuEngineRowsSnapshot,
) {
    let attempt = session.begin_attempt(snapshot.device_id.clone());
    let request_id = RequestId::new(request).expect("fixture request id");
    assert!(session.accept_attempt(attempt, request_id));
    assert!(session.complete(request_id, snapshot));
}

#[test]
fn mc04_gpu_panel_case_ready_session_renders_only_the_matching_session_payload() {
    let snapshot = GpuEngineRowsSnapshot::success(device(), sample_engines());
    let mut session = GpuEngineRowsSession::default();
    accept_snapshot(&mut session, 1, snapshot.clone());

    match present_gpu_engine_rows(
        session.state(),
        &device(),
        Some(CapabilityStatus::Available),
    ) {
        GpuEngineRowsPresentation::Active(engines) => {
            assert_eq!(engines[0].utilization_pct, 42.5);
            assert_eq!(engines[1].utilization_pct, 0.0);
        }
        other => panic!("expected accepted engine rows, got {other:?}"),
    }
    assert_eq!(
        present_gpu_engine_rows(
            session.state(),
            &DeviceId::new("gpu:1"),
            Some(CapabilityStatus::MissingDependency),
        ),
        GpuEngineRowsPresentation::MissingDependency,
        "a terminal for another device cannot supply visible rows"
    );
}

#[test]
fn refresh_loading_preserves_the_last_accepted_session_payload() {
    let snapshot = GpuEngineRowsSnapshot::success(device(), sample_engines());
    let mut session = GpuEngineRowsSession::default();
    accept_snapshot(&mut session, 1, snapshot.clone());
    let _ = session.begin_attempt(device());

    assert!(matches!(
        present_gpu_engine_rows(
            session.state(),
            &device(),
            Some(CapabilityStatus::Available),
        ),
        GpuEngineRowsPresentation::Active(engines) if engines.len() == 2
    ));
}

#[test]
fn closed_session_never_resurrects_a_late_payload() {
    let snapshot = GpuEngineRowsSnapshot::success(device(), sample_engines());
    let mut session = GpuEngineRowsSession::default();
    accept_snapshot(&mut session, 1, snapshot.clone());
    session.close();

    assert_eq!(
        present_gpu_engine_rows(
            session.state(),
            &device(),
            Some(CapabilityStatus::PermissionRequired),
        ),
        GpuEngineRowsPresentation::PermissionRequired,
    );
}

#[test]
fn mc04_gpu_unavailable_case_provider_failures_keep_their_named_honest_presentations() {
    for (request, kind, expected) in [
        (
            1,
            FailureKind::PermissionDenied,
            GpuEngineRowsPresentation::PermissionDenied,
        ),
        (
            2,
            FailureKind::MissingDependency,
            GpuEngineRowsPresentation::MissingDependency,
        ),
        (
            3,
            FailureKind::Unsupported,
            GpuEngineRowsPresentation::Unsupported,
        ),
        (
            4,
            FailureKind::ProviderFault,
            GpuEngineRowsPresentation::Failed,
        ),
    ] {
        let mut session = GpuEngineRowsSession::default();
        accept_snapshot(
            &mut session,
            request,
            GpuEngineRowsSnapshot::failed(device(), kind, "why"),
        );
        let view = present_gpu_engine_rows(
            session.state(),
            &device(),
            Some(CapabilityStatus::Available),
        );
        assert_eq!(view, expected, "{kind:?} mapped to the wrong typed view");
    }
}
