//! The engine-rows lane commits only the active request terminal. A stale
//! terminal earlier in the same batch cannot overwrite that request's answer,
//! and a batch carrying no engine-rows events leaves the accepted session
//! untouched.

use super::*;
use taskmanager_application::{
    CorrelatedGpuEngineRowsEvent, GpuEngineRowsEvent, GpuEngineRowsRequestFailure,
    GpuEngineRowsState,
};
use taskmanager_core::core::metrics::GpuEngineRowsSnapshot;

fn gpu_engine_rows_event(
    sequence: u64,
    snapshot: GpuEngineRowsSnapshot,
) -> CorrelatedGpuEngineRowsEvent {
    CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(sequence).expect("non-zero fixture request id"),
            capability: CapabilityId::TELEMETRY_GPU_ENGINES,
            provider: None,
            sequence: EventSequence::new(sequence),
            observed_at_ms: 10,
        },
        GpuEngineRowsEvent::Update(snapshot),
    )
}

#[test]
fn gpu_engine_rows_snapshots_commit_only_the_active_request() {
    use taskmanager_core::core::metrics::{GpuEngineKind, GpuEngineMetric};
    let mut app = ShellApp::new();
    let attempt = app.begin_gpu_engine_rows_request(DeviceId::new("gpu:0"));
    assert!(app.accept_gpu_engine_rows_request(
        attempt,
        RequestId::new(5).expect("non-zero fixture request id")
    ));
    let mut batch = PlatformEventBatch::default();
    batch.gpu_engine_rows_events.push(gpu_engine_rows_event(
        4,
        GpuEngineRowsSnapshot::success(
            DeviceId::new("gpu:0"),
            vec![GpuEngineMetric {
                name: "Render Ring".to_owned(),
                kind: GpuEngineKind::Unknown,
                utilization_pct: 40.0,
            }],
        ),
    ));
    batch.gpu_engine_rows_events.push(gpu_engine_rows_event(
        5,
        GpuEngineRowsSnapshot::failed(
            DeviceId::new("gpu:0"),
            FailureKind::PermissionDenied,
            "user dismissed the prompt",
        ),
    ));

    app.apply_platform_batch(batch);

    assert!(matches!(
        app.gpu_engine_rows_state(),
        GpuEngineRowsState::Failed(failed)
            if failed.device_id == DeviceId::new("gpu:0")
                && matches!(
                    &failed.failure,
                    GpuEngineRowsRequestFailure::Provider(failure)
                        if failure.kind == FailureKind::PermissionDenied
                )
    ));

    app.apply_platform_batch(PlatformEventBatch::default());
    assert!(
        matches!(app.gpu_engine_rows_state(), GpuEngineRowsState::Failed(_)),
        "an empty-events batch must leave the request lifecycle untouched"
    );
}
