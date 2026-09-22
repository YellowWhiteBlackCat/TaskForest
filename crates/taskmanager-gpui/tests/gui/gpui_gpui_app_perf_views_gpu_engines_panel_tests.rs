use taskmanager_application::GpuEngineRowsSession;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::{
    GpuEngine, GpuEngineKind, GpuEngineMetric, GpuEngineRowsSnapshot, GpuMetrics,
};
use taskmanager_platform_contract::{CapabilityStatus, RequestId};
use taskmanager_shell::presentation::gpu_engine_rows::{
    GpuEngineRowsPresentation, present_gpu_engine_rows,
};
use taskmanager_telemetry_store::TelemetryStore;

use crate::gpui_app::perf_views::gpu_page::engine_grid::present_gpu_engine_mini_grid;

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

    // The GPU page's own mini-grid consumes that accepted payload: the cards
    // are the fold's cells (never a local reinterpretation), the per-engine
    // percentage resolves session-first with the snapshot's own observation as
    // the fallback, and another device's session never leaks in.
    let mut metrics = GpuMetrics::new("gpu:0", "Fixture GPU");
    metrics.engines = vec![
        GpuEngine {
            name: "Render Ring".to_owned(),
            kind: GpuEngineKind::Unknown,
            usage_pct: 9.0,
        },
        GpuEngine {
            name: "Video Decode".to_owned(),
            kind: GpuEngineKind::VideoDecode,
            usage_pct: 7.0,
        },
    ];
    let (store, _ingestor) = TelemetryStore::shared_with_correlated_ingestion(1);
    let grid = present_gpu_engine_mini_grid(
        &store.system_history,
        &metrics,
        session.state(),
        &device(),
        Some(CapabilityStatus::Available),
        Some(4),
    )
    .expect("a named engine inventory must admit the mini-grid");
    assert_eq!(
        grid.cells
            .iter()
            .map(|cell| (cell.name.as_str(), cell.utilization_pct))
            .collect::<Vec<_>>(),
        vec![
            ("Blitter", Some(0.0)),
            ("Render Ring", Some(42.5)),
            ("Video Decode", Some(7.0)),
        ],
        "the accepted session payload leads each card; engines it never reported keep the snapshot's own observation"
    );
    assert_eq!(
        (grid.visible_count(), grid.total_engines, grid.columns),
        (3, 3, 3),
        "an unbounded three-engine inventory paints every card"
    );
    let other_device = present_gpu_engine_mini_grid(
        &store.system_history,
        &metrics,
        session.state(),
        &DeviceId::new("gpu:1"),
        Some(CapabilityStatus::Available),
        Some(4),
    )
    .expect("the page's own metrics still name engines");
    assert_eq!(
        other_device
            .cells
            .iter()
            .map(|cell| cell.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Render Ring", "Video Decode"],
        "another device's accepted session must not contribute cards"
    );

    // A non-finite session reading stays a typed missing value: the card
    // paints the shared dash instead of a fabricated zero.
    let mut non_finite = GpuEngineRowsSession::default();
    accept_snapshot(
        &mut non_finite,
        2,
        GpuEngineRowsSnapshot::success(
            device(),
            vec![GpuEngineMetric {
                name: "Compute".to_owned(),
                kind: GpuEngineKind::Compute,
                utilization_pct: f32::NAN,
            }],
        ),
    );
    let non_finite_grid = present_gpu_engine_mini_grid(
        &store.system_history,
        &metrics,
        non_finite.state(),
        &device(),
        Some(CapabilityStatus::Available),
        Some(4),
    )
    .expect("the session names one engine");
    let compute = non_finite_grid
        .cells
        .iter()
        .find(|cell| cell.name == "Compute")
        .expect("the session-only engine must appear as a card");
    assert_eq!(
        compute.utilization_pct, None,
        "a non-finite observation must stay typed missing"
    );

    // The row budget admits complete rows only: five engines need two rows,
    // a one-row budget paints four cards and summarizes the remainder; a
    // zero-row budget paints nothing. The session stays closed here, so the
    // inventory names come from the page's own snapshot.
    let mut crowded = GpuMetrics::new("gpu:0", "Fixture GPU");
    crowded.engines = (0..5_u16)
        .map(|index| GpuEngine {
            name: format!("Engine {index}"),
            kind: GpuEngineKind::Unknown,
            usage_pct: f32::from(index),
        })
        .collect();
    let closed = GpuEngineRowsSession::default();
    let bounded = present_gpu_engine_mini_grid(
        &store.system_history,
        &crowded,
        closed.state(),
        &device(),
        Some(CapabilityStatus::Available),
        Some(1),
    )
    .expect("a one-row budget admits one complete four-column row");
    assert_eq!(
        (
            bounded.visible_count(),
            bounded.total_engines,
            bounded.columns
        ),
        (4, 5, 4),
        "only complete rows are admitted; the header summarizes the remainder"
    );
    assert!(
        bounded.cells.iter().all(|cell| cell.name != "Engine 4"),
        "the bounded-away engine must not paint a partial card"
    );
    assert!(
        present_gpu_engine_mini_grid(
            &store.system_history,
            &crowded,
            closed.state(),
            &device(),
            Some(CapabilityStatus::Available),
            Some(0),
        )
        .is_none(),
        "a zero-row budget must paint nothing"
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
