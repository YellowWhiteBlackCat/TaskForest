use super::*;
use taskmanager_application::{
    GpuEngineKind, GpuEngineMetric, GpuEngineRowsSession, GpuEngineRowsSnapshot, RequestId,
};

fn device() -> DeviceId {
    DeviceId::new("gpu:0")
}

#[test]
fn closed_session_projects_every_capability_state_without_string_parsing() {
    let state = GpuEngineRowsState::Closed;
    let cases = [
        (
            Some(CapabilityStatus::PermissionRequired),
            GpuEngineRowsPresentation::PermissionRequired,
            GpuEngineRowsAction::Enable,
        ),
        (
            Some(CapabilityStatus::MissingDependency),
            GpuEngineRowsPresentation::MissingDependency,
            GpuEngineRowsAction::Recheck,
        ),
        (
            Some(CapabilityStatus::TemporarilyUnavailable),
            GpuEngineRowsPresentation::AuthorizationUnavailable,
            GpuEngineRowsAction::Recheck,
        ),
        (
            Some(CapabilityStatus::Unsupported),
            GpuEngineRowsPresentation::Unsupported,
            GpuEngineRowsAction::None,
        ),
    ];
    for (status, expected, action) in cases {
        let actual = present_gpu_engine_rows(&state, &device(), status);
        assert_eq!(actual, expected);
        assert_eq!(actual.action(), action);
    }
}

#[test]
fn accepted_session_payload_is_the_only_rendered_row_authority() {
    let mut session = GpuEngineRowsSession::default();
    let attempt = session.begin_attempt(device());
    let request_id = RequestId::new(1).expect("fixture request id");
    assert!(session.accept_attempt(attempt, request_id));
    assert!(session.complete(
        request_id,
        GpuEngineRowsSnapshot::success(
            device(),
            vec![GpuEngineMetric {
                name: "Render".to_owned(),
                kind: GpuEngineKind::Render,
                utilization_pct: 42.0,
            }],
        ),
    ));

    let presentation = present_gpu_engine_rows(
        session.state(),
        &device(),
        Some(CapabilityStatus::Available),
    );
    assert!(matches!(
        presentation,
        GpuEngineRowsPresentation::Active(rows) if rows.len() == 1 && rows[0].name == "Render"
    ));
}
