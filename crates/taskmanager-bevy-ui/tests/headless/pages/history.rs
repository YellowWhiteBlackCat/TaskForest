//! Headless behavior tests for the Bevy application-history page slice.
//!
//! These tests are mounted by `src/pages/history.rs`. The route registration
//! is deliberately a separate mainline integration step, so this file tests
//! the page's projection/runtime boundary without inventing a window or a
//! collector process.

use std::sync::Arc;

use super::*;
use taskmanager_application::{
    ApplicationHistoryRow, HistoryReplayCompletion, HistoryReplayCompletionDisposition,
    HistoryReplayCompletionOutcome, HistoryReplayError, HistoryReplayErrorKind, HistoryReplayRow,
};
use taskmanager_core::core::history::{HistoryMetric, HistorySeriesKey};

fn identity(value: &str, verified: bool) -> ApplicationHistoryIdentity {
    if verified {
        ApplicationHistoryIdentity::verified_launcher(value).expect("verified identity")
    } else {
        ApplicationHistoryIdentity::unverified_process_name(value).expect("fallback identity")
    }
}

fn replay_row(
    identity: ApplicationHistoryIdentity,
    metric: HistoryMetric,
    samples: &[f32],
    times: &[u64],
    peak: Option<f64>,
) -> HistoryReplayRow {
    assert_eq!(samples.len(), times.len());
    HistoryReplayRow {
        key: HistorySeriesKey::for_application(metric, identity),
        samples: Arc::from(samples),
        sample_times_ms: Arc::from(times),
        peak_value: peak,
        peak_measured_at_ms: Some(*times.last().unwrap_or(&0)),
        observed: samples.iter().filter(|sample| sample.is_finite()).count(),
        gaps: samples.iter().filter(|sample| !sample.is_finite()).count(),
        clock_jumps: 0,
    }
}

fn loaded_projection(rows: Vec<HistoryReplayRow>) -> ApplicationHistoryProjection {
    let mut controller = HistoryReplayController::default();
    let request = controller.open().expect("fresh controller opens");
    assert_eq!(
        controller.complete(HistoryReplayCompletion {
            request,
            loaded_at_ms: 10,
            outcome: HistoryReplayCompletionOutcome::Loaded(Arc::from(rows)),
        }),
        HistoryReplayCompletionDisposition::Applied
    );
    controller.application_history_projection(ApplicationHistoryCapability::Available)
}

fn application_row(identity: ApplicationHistoryIdentity, samples: &[f32]) -> ApplicationHistoryRow {
    let times = (0..samples.len())
        .map(|index| 1_000 + index as u64 * 1_000)
        .collect::<Vec<_>>();
    let cpu = replay_row(
        identity.clone(),
        HistoryMetric::ApplicationCpuUsagePct,
        samples,
        &times,
        samples
            .iter()
            .copied()
            .filter(|sample| sample.is_finite())
            .map(f64::from)
            .reduce(f64::max),
    );
    let memory = replay_row(
        identity.clone(),
        HistoryMetric::ApplicationMemoryBytes,
        samples,
        &times,
        Some(1024.0),
    );
    let process_count = replay_row(
        identity,
        HistoryMetric::ApplicationProcessCount,
        samples,
        &times,
        Some(3.0),
    );
    let rows = [cpu, memory, process_count];
    let projection = loaded_projection(rows.into_iter().collect());
    projection
        .rows
        .iter()
        .next()
        .cloned()
        .expect("the synthetic application row projects")
}

#[test]
fn stable_typed_identity_joins_three_metrics_without_collapsing_provenance() {
    let verified = identity("same-name", true);
    let fallback = identity("same-name", false);
    let rows = vec![
        replay_row(
            verified.clone(),
            HistoryMetric::ApplicationMemoryBytes,
            &[20.0, 40.0],
            &[1_000, 2_000],
            Some(40.0),
        ),
        replay_row(
            fallback.clone(),
            HistoryMetric::ApplicationCpuUsagePct,
            &[10.0, 30.0],
            &[1_000, 2_000],
            Some(30.0),
        ),
        replay_row(
            verified.clone(),
            HistoryMetric::ApplicationCpuUsagePct,
            &[3.0, 9.0],
            &[1_000, 2_000],
            Some(9.0),
        ),
        replay_row(
            verified.clone(),
            HistoryMetric::ApplicationProcessCount,
            &[1.0, 3.0],
            &[1_000, 2_000],
            Some(3.0),
        ),
    ];
    let model = HistoryPageModel::from_projection(&loaded_projection(rows));

    assert_eq!(model.rows.len(), 2);
    assert_eq!(model.rows[0].identity, fallback);
    assert!(!model.rows[0].verified);
    assert!(model.rows[0].memory.is_none());
    assert_eq!(model.rows[1].identity, verified);
    assert!(model.rows[1].verified);
    assert_eq!(model.rows[1].cpu_peak(), Some(9.0));
    assert_eq!(model.rows[1].memory_peak(), Some(40.0));
    assert_eq!(model.rows[1].process_count_peak(), Some(3.0));
}

#[test]
fn cpu_memory_and_process_history_are_bounded_at_the_render_boundary() {
    let samples = (0..(MAX_RENDERED_HISTORY_POINTS + 100))
        .map(|value| value as f32)
        .collect::<Vec<_>>();
    let row = application_row(identity("bounded", true), &samples);
    let model = HistoryPageModel::from_projection(&ApplicationHistoryProjection {
        status: ApplicationHistoryStatus::Ready,
        selected_window: HistoryWindow::OneHour,
        rows_window: Some(HistoryWindow::OneHour),
        rows: Arc::from([row]),
        source_request: None,
        refreshing: false,
        failure: None,
        unavailable_reason: None,
        loaded_at_ms: Some(10),
    });
    let row = &model.rows[0];
    for metric in [&row.cpu, &row.memory, &row.process_count] {
        let metric = metric.as_ref().expect("all three metrics are projected");
        assert_eq!(metric.samples.len(), MAX_RENDERED_HISTORY_POINTS);
        assert_eq!(metric.samples[0], 100.0);
        assert_eq!(metric.samples.last().copied(), Some(699.0));
    }
}

#[test]
fn gaps_are_visible_as_nan_slots_and_unknown_scalars_never_become_zero() {
    let row = replay_row(
        identity("gapped", true),
        HistoryMetric::ApplicationCpuUsagePct,
        &[1.0, 2.0, 4.0],
        &[1_000, 2_000, 20_000],
        Some(4.0),
    );
    let model = HistoryPageModel::from_projection(&loaded_projection(vec![row]));
    let samples = &model.rows[0].cpu.as_ref().expect("cpu series").samples;
    assert_eq!(&samples[..2], &[1.0, 2.0]);
    assert!(
        samples[2].is_nan(),
        "collector downtime remains a blank slot"
    );
    assert_eq!(samples[3], 4.0);
    assert_eq!(memory_text(None), missing_value());
    assert_eq!(process_count_text(Some(-1.0)), missing_value());
}

#[test]
fn stale_last_good_rows_keep_their_source_window_and_error_is_not_hidden() {
    let mut controller = HistoryReplayController::default();
    let request = controller.open().expect("open");
    let row = replay_row(
        identity("stale", true),
        HistoryMetric::ApplicationCpuUsagePct,
        &[1.0, 2.0],
        &[1_000, 2_000],
        Some(2.0),
    );
    assert_eq!(
        controller.complete(HistoryReplayCompletion {
            request,
            loaded_at_ms: 10,
            outcome: HistoryReplayCompletionOutcome::Loaded(Arc::from([row])),
        }),
        HistoryReplayCompletionDisposition::Applied
    );
    let wider = controller
        .select_window(HistoryWindow::TwentyFourHours)
        .expect("selecting a new window starts a bounded refresh");
    let stale = controller.application_history_projection(ApplicationHistoryCapability::Available);
    let stale_model = HistoryPageModel::from_projection(&stale);
    assert!(stale_model.has_visible_rows());
    assert!(stale_model.notice.stale);
    assert_eq!(stale_model.rows_window, Some(HistoryWindow::OneHour));
    assert_eq!(stale_model.selected_window, HistoryWindow::TwentyFourHours);

    let failure = HistoryReplayError::new(HistoryReplayErrorKind::Read, "reader failed");
    assert_eq!(
        controller.complete(HistoryReplayCompletion {
            request: wider,
            loaded_at_ms: 20,
            outcome: HistoryReplayCompletionOutcome::Failed(failure),
        }),
        HistoryReplayCompletionDisposition::Applied
    );
    let failed = controller.application_history_projection(ApplicationHistoryCapability::Available);
    let failed_model = HistoryPageModel::from_projection(&failed);
    assert!(failed_model.has_visible_rows());
    assert_eq!(failed_model.notice.error_code, Some("read"));
    assert!(failed_model.notice.stale);
}

#[test]
fn empty_collecting_and_error_without_last_good_are_distinct_states() {
    let controller = HistoryReplayController::default();
    let collecting =
        controller.application_history_projection(ApplicationHistoryCapability::Available);
    let collecting_model = HistoryPageModel::from_projection(&collecting);
    assert_eq!(
        collecting_model.status,
        ApplicationHistoryStatus::Collecting
    );
    assert!(!collecting_model.has_visible_rows());

    let error = HistoryReplayError::new(HistoryReplayErrorKind::Decode, "bad series");
    let mut failed_controller = HistoryReplayController::default();
    let request = failed_controller.open().expect("open");
    failed_controller.complete(HistoryReplayCompletion {
        request,
        loaded_at_ms: 10,
        outcome: HistoryReplayCompletionOutcome::Failed(error),
    });
    let failed =
        failed_controller.application_history_projection(ApplicationHistoryCapability::Available);
    let failed_model = HistoryPageModel::from_projection(&failed);
    assert_eq!(failed_model.status, ApplicationHistoryStatus::Unavailable);
    assert!(!failed_model.has_visible_rows());
    assert_eq!(failed_model.notice.error_code, Some("decode"));

    let disabled = HistoryPageModel::from_projection(
        &controller.application_history_projection(ApplicationHistoryCapability::Disabled),
    );
    assert_eq!(disabled.status, ApplicationHistoryStatus::Disabled);
    assert!(!disabled.has_visible_rows());
}

#[test]
fn runtime_is_inert_without_a_connector_and_only_exposes_typed_read_states() {
    let mut runtime = HistoryRuntime::default();
    assert!(!runtime.drain(), "an idle runtime does not repaint");
    assert_eq!(
        runtime.projection().status,
        ApplicationHistoryStatus::Disabled
    );
    runtime.request(true);
    assert_eq!(
        runtime.projection().status,
        ApplicationHistoryStatus::Unavailable
    );
    assert_eq!(
        runtime
            .projection()
            .unavailable_reason
            .map(|reason| reason.stable_code()),
        Some("connector_stopped")
    );
    assert!(runtime.projection().rows.is_empty());
    runtime.request(false);
    assert_eq!(
        runtime.projection().status,
        ApplicationHistoryStatus::Disabled
    );
}

// ---- wired page assembly: the disabled state speaks exactly once ----------

/// A disabled history page states its case once: the disabled heading and
/// detail are one surface, not a copy per series or per mount path. The
/// capture matrix renders this page; this test is its headless twin.
#[test]
fn the_disabled_page_states_itself_exactly_once() {
    use bevy::MinimalPlugins;
    use bevy::app::App;
    use bevy::asset::{AssetPlugin, Assets};
    use bevy::scene::{ScenePlugin, WorldSceneExt};
    use bevy::text::Font;
    use bevy::ui::widget::Text;
    use taskmanager_theme::Theme;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    // The paint path (bound by the page's on-insert hook) reads these two
    // resources; the window composition always has them.
    app.insert_resource(crate::pages::history::HistoryProjectionResource::default());
    app.insert_resource(crate::window::WindowPalette {
        inner: crate::palette::ui_palette(&Theme::dark()),
    });
    let projection = crate::pages::history::HistoryRuntime::default().projection();
    let palette = crate::palette::ui_palette(&Theme::dark());
    let world = app.world_mut();
    let root = world
        .spawn_scene(crate::pages::history::scene::content(&projection, &palette))
        .expect("the history scene resolves without assets")
        .id();
    // Flush the spawn AND a second frame so any late bind/paint pass runs.
    app.update();
    app.update();

    let heading = t("history.application.disabled");
    let detail = t("history.application.disabled_detail");
    let mut texts = app.world_mut().query::<&Text>();
    let count_hits = |state: &mut bevy::ecs::query::QueryState<&Text>,
                      world: &bevy::ecs::world::World,
                      needle: &str|
     -> usize { state.iter(world).filter(|text| text.0 == needle).count() };
    // Flush the spawn AND extra frames so every late bind/paint pass has
    // settled before counting.
    app.update();
    app.update();
    app.update();

    let said = count_hits(&mut texts, app.world(), heading);
    let said_detail = count_hits(&mut texts, app.world(), detail);
    assert_eq!(
        said, 1,
        "the disabled heading must appear exactly once, got {said}"
    );
    assert_eq!(said_detail, 1, "the disabled detail appears exactly once");
    let _ = root;
}
