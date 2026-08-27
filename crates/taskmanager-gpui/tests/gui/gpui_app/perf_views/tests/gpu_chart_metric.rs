//! GPU headline chart-metric selection regressions (ADR-034 stage 2):
//! the pill row, the shared projection, the availability gate, and the
//! generation reset — all through the same shell contract the Iced and TUI
//! frontends consume.

use super::*;

fn chart_metric_gpu(device_id: &str, utilization: f32, temperature_c: f32) -> GpuMetrics {
    let mut gpu = GpuMetrics::new(device_id, "Fixture GPU");
    gpu.device_generation = DeviceGeneration::new(1);
    gpu.device_state = crate::core::DeviceState::healthy(10);
    gpu.apply_scalar_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(utilization, 1),
        temperature_c: ScalarObservation::available(temperature_c, 1),
        // Power stays unobserved: the availability gate must keep its pill
        // visible-but-inert and reject its selection.
        ..GpuScalarObservations::default()
    });
    gpu
}

fn chart_metric_observation(
    gpu: GpuMetrics,
    observed_at_ms: u64,
) -> crate::core::GpuTelemetryObservation {
    let lifecycle = (
        crate::core::DeviceId::new(gpu.device_id.clone()),
        crate::core::DeviceLifecycle {
            presence: crate::core::DevicePresence::Present,
            state: crate::core::DeviceState::healthy(observed_at_ms),
            generation: 1,
            first_seen_ms: Some(observed_at_ms),
            last_seen_ms: Some(observed_at_ms),
            absent_since_ms: None,
        },
    );
    crate::core::GpuTelemetryObservation::current(
        vec![gpu],
        observed_at_ms,
        Vec::new(),
        Vec::new(),
        std::collections::BTreeMap::from([lifecycle]),
    )
}

fn seed_chart_metric_gpu(view: &mut RootView, device_id: &str, frames: [(f32, f32); 2]) {
    for (revision, (utilization, temperature)) in frames.into_iter().enumerate() {
        let revision = u64::try_from(revision + 1).expect("small revision");
        let gpu = chart_metric_gpu(device_id, utilization, temperature);
        let observed_at_ms = revision * 10;
        view.telemetry_ingestor
            .ingest_correlated_gpu(
                CorrelatedTelemetryStamp::from_accepted_event(revision, observed_at_ms + 1)
                    .expect("fixture revision is non-zero"),
                &chart_metric_observation(gpu, observed_at_ms),
            )
            .expect("gpu chart-metric fixture enters system history");
    }
}

/// The default projection renders the full vocabulary pill row — including
/// the unobserved power family — selects Utilization, and the headline graph
/// follows the selection through the shared `GpuChartMetric::value` fold.
/// The activation path (mouse click and the Pill's keyboard Enter/Space,
/// which route through the same `on_click`) accepts an observed family and
/// silently rejects an unobserved one.
#[gpui::test]
async fn gpu_chart_metric_pills_default_gate_and_selection(cx: &mut TestAppContext) {
    use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric;

    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        seed_chart_metric_gpu(v, "gpu:chart:metric", [(42.0, 61.0), (55.0, 63.0)]);
        v.system_snapshot_mut_for_test().gpu =
            vec![chart_metric_gpu("gpu:chart:metric", 55.0, 63.0)];
        v.selected = SelectedDevice::Gpu(0);
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-gpu-chart-metric-selector").is_some(),
        "the GPU page must render the shared selector row"
    );
    for metric in GpuChartMetric::ALL {
        let selector: &str =
            Box::leak(format!("tm-gpu-chart-metric-pill:{}", metric.id_stem()).into_boxed_str());
        assert!(
            vcx.debug_bounds(selector).is_some(),
            "every vocabulary family stays visible — including the unobserved ones ({selector})"
        );
    }
    drop(vcx);

    view.read_with(cx, |v, _| {
        assert_eq!(
            v.shell.gpu_chart_metric_selected(),
            GpuChartMetric::Utilization
        );
    });

    // An unobserved family is rejected with no state change.
    view.update(cx, |v, cx| {
        v.select_gpu_chart_metric(GpuChartMetric::Power, 0, cx);
    });
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.shell.gpu_chart_metric_selected(),
            GpuChartMetric::Utilization
        );
    });

    // An observed family is selected and the headline window follows it.
    view.update(cx, |v, cx| {
        v.select_gpu_chart_metric(GpuChartMetric::Temperature, 0, cx);
    });
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.shell.gpu_chart_metric_selected(),
            GpuChartMetric::Temperature
        );
        // The headline window reads the ONE shared dispatch (the same call
        // `render_gpu` makes over this direct track's live-graph view), not a
        // frontend-local fold.
        let window = taskmanager_shell::presentation::gpu_chart_metric::gpu_chart_metric_history(
            &v.live_graph_history,
            "gpu:chart:metric",
            1,
            GpuChartMetric::Temperature,
        );
        assert_eq!(window, [61.0, 63.0]);
        // The unobserved family's window stays explicit gaps, never a zero.
        let power = taskmanager_shell::presentation::gpu_chart_metric::gpu_chart_metric_history(
            &v.live_graph_history,
            "gpu:chart:metric",
            1,
            GpuChartMetric::Power,
        );
        assert!(power.iter().all(|value| value.is_nan()));
    });
    draw(cx, win);
}

// ── Single-track sampling anchor (post ADR-034 stage-2 convergence) ─────────
//
// GPU chart-metric sampling used to run on two tracks: the shell/store
// dispatch the Iced and TUI shells read, and a frontend-local typed fold.
// They agreed on values and NaN gaps but silently diverged on window
// capacity and generation isolation, so the typed fold was sunk into the
// store as ONE generation-scoped read and every frontend now consumes the
// single `gpu_chart_metric_history` dispatch. This anchor keeps proving the
// two consumption SHAPES — this direct track's full-retention view and the
// composed track's capacity-narrowed view — sample the same window
// semantics, so the tracks can never fork again.

use std::collections::BTreeMap;

use taskmanager_shell::history::LiveGraphHistory;
use taskmanager_telemetry_store::live_graph::MAX_HISTORY_CAPACITY;

const TRACKS_DEVICE_ID: &str = "gpu:tracks:probe";
const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

/// A GPU observing every chartable family (all-percent memory pairs are 25%).
fn fully_observed_gpu(utilization: f32) -> GpuMetrics {
    let mut gpu = GpuMetrics::new(TRACKS_DEVICE_ID, "Probe GPU");
    gpu.device_generation = DeviceGeneration::new(1);
    gpu.device_state = crate::core::DeviceState::healthy(10);
    gpu.apply_scalar_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(utilization, 1),
        temperature_c: ScalarObservation::available(61.0, 1),
        power_w: ScalarObservation::available(88.5, 1),
        frequency_mhz: ScalarObservation::available(2_400, 1),
        idle_residency_pct: ScalarObservation::available(40.0, 1),
        memory_used_bytes: ScalarObservation::available(2 * GIB, 1),
        memory_total_bytes: ScalarObservation::available(8 * GIB, 1),
        dedicated_vram_used_bytes: ScalarObservation::available(GIB, 1),
        dedicated_vram_total_bytes: ScalarObservation::available(4 * GIB, 1),
        shared_vram_used_bytes: ScalarObservation::available(512 * MIB, 1),
        shared_vram_total_bytes: ScalarObservation::available(2 * GIB, 1),
        ..GpuScalarObservations::default()
    });
    gpu
}

fn seed_tracks_frames(
    ingestor: &taskmanager_telemetry_store::CorrelatedSystemTelemetryIngestor,
    frames: &[GpuMetrics],
) {
    for (index, gpu) in frames.iter().enumerate() {
        let revision = u64::try_from(index + 1).expect("small revision");
        let observed_at_ms = revision * 10;
        let lifecycle = (
            crate::core::DeviceId::new(TRACKS_DEVICE_ID.to_owned()),
            crate::core::DeviceLifecycle {
                presence: crate::core::DevicePresence::Present,
                state: crate::core::DeviceState::healthy(observed_at_ms),
                generation: 1,
                first_seen_ms: Some(observed_at_ms),
                last_seen_ms: Some(observed_at_ms),
                absent_since_ms: None,
            },
        );
        ingestor
            .ingest_correlated_gpu(
                CorrelatedTelemetryStamp::from_accepted_event(revision, observed_at_ms + 1)
                    .expect("fixture revision is non-zero"),
                &crate::core::GpuTelemetryObservation::current(
                    vec![gpu.clone()],
                    observed_at_ms,
                    Vec::new(),
                    Vec::new(),
                    BTreeMap::from([lifecycle]),
                ),
            )
            .expect("tracks fixture enters system history");
    }
}

/// Bitwise window comparison: `NaN != NaN`, so gap positions are compared by
/// bit pattern — every track shape must produce the literal `f32::NAN` gap.
fn same_window(direct_track: &[f32], composed_track: &[f32]) -> bool {
    direct_track.len() == composed_track.len()
        && direct_track
            .iter()
            .zip(composed_track)
            .all(|(left, right)| left.to_bits() == right.to_bits())
}

/// Within one window (content no longer than every view's capacity), the
/// direct track's full-retention view and the composed track's
/// capacity-narrowed view return the SAME window for every family — values,
/// lengths and NaN gap positions — because both call the one dispatch.
#[test]
fn gpu_chart_metric_sampling_is_one_track_across_view_capacities() {
    use taskmanager_shell::presentation::gpu_chart_metric::{
        GpuChartMetric, gpu_chart_metric_history,
    };

    let (live, ingestor) = LiveGraphHistory::shared(MAX_HISTORY_CAPACITY);
    let composed = LiveGraphHistory::from_store(live.store().clone(), 64);
    // One gap frame in the middle: utilization unobserved, everything else
    // measured — the Utilization family alone must gap there.
    let mut gap_frame = GpuMetrics::new(TRACKS_DEVICE_ID, "Probe GPU");
    gap_frame.device_generation = DeviceGeneration::new(1);
    gap_frame.device_state = crate::core::DeviceState::healthy(10);
    gap_frame.apply_scalar_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::unavailable(
            crate::core::FailureKind::TemporarilyUnavailable,
        ),
        temperature_c: ScalarObservation::available(61.0, 1),
        power_w: ScalarObservation::available(88.5, 1),
        frequency_mhz: ScalarObservation::available(2_400, 1),
        idle_residency_pct: ScalarObservation::available(40.0, 1),
        memory_used_bytes: ScalarObservation::available(2 * GIB, 1),
        memory_total_bytes: ScalarObservation::available(8 * GIB, 1),
        dedicated_vram_used_bytes: ScalarObservation::available(GIB, 1),
        dedicated_vram_total_bytes: ScalarObservation::available(4 * GIB, 1),
        shared_vram_used_bytes: ScalarObservation::available(512 * MIB, 1),
        shared_vram_total_bytes: ScalarObservation::available(2 * GIB, 1),
        ..GpuScalarObservations::default()
    });

    seed_tracks_frames(
        &ingestor,
        &[
            fully_observed_gpu(10.0),
            gap_frame,
            fully_observed_gpu(20.0),
        ],
    );

    for metric in GpuChartMetric::ALL {
        let direct_track = gpu_chart_metric_history(&live, TRACKS_DEVICE_ID, 1, metric);
        let composed_track = gpu_chart_metric_history(&composed, TRACKS_DEVICE_ID, 1, metric);
        assert!(
            same_window(&direct_track, &composed_track),
            "track shapes diverged for {metric:?}: direct {direct_track:?} vs composed {composed_track:?}"
        );
    }

    // The utilization gap is where the probe says it is, and it is a NaN —
    // not a fabricated zero — on both track shapes.
    let utilization =
        gpu_chart_metric_history(&live, TRACKS_DEVICE_ID, 1, GpuChartMetric::Utilization);
    assert_eq!(utilization.len(), 3);
    assert_eq!(utilization[0], 10.0);
    assert!(utilization[1].is_nan());
    assert_eq!(utilization[2], 20.0);
}

/// The capacity difference is exactly a tail of one and the same window, and
/// the single track keeps the generation isolation the old local fold had:
/// a row/ring generation mismatch (row advanced, ring not re-ingested) and
/// an unbound `0` generation yield an honest empty window — never the
/// previous device instance's samples.
#[test]
fn gpu_chart_metric_sampling_tails_one_window_and_scopes_generation() {
    use taskmanager_shell::presentation::gpu_chart_metric::{
        GpuChartMetric, gpu_chart_metric_history,
    };

    let (full, ingestor) = LiveGraphHistory::shared(MAX_HISTORY_CAPACITY);
    let frames: Vec<GpuMetrics> = (0..80u16)
        .map(|index| fully_observed_gpu(f32::from(index) + 1.0))
        .collect();
    seed_tracks_frames(&ingestor, &frames);
    let narrow = LiveGraphHistory::from_store(full.store().clone(), 64);

    let metric = GpuChartMetric::Power;
    let full_window = gpu_chart_metric_history(&full, TRACKS_DEVICE_ID, 1, metric);
    let narrow_window = gpu_chart_metric_history(&narrow, TRACKS_DEVICE_ID, 1, metric);
    assert_eq!(
        full_window.len(),
        80,
        "the direct track keeps full retention"
    );
    assert_eq!(
        narrow_window.len(),
        64,
        "the composed track tails to capacity"
    );
    assert!(
        same_window(&narrow_window, &full_window[full_window.len() - 64..]),
        "the narrowed window is exactly the tail of the one retained window"
    );

    // A row/ring generation mismatch breaks the window on the single track;
    // the pre-convergence shell/store track leaked the previous generation's
    // samples here.
    assert!(
        gpu_chart_metric_history(&full, TRACKS_DEVICE_ID, 2, metric).is_empty(),
        "a generation mismatch must yield an honest empty window"
    );
    assert!(
        gpu_chart_metric_history(&full, TRACKS_DEVICE_ID, 0, metric).is_empty(),
        "an unbound generation must not inherit any ring's samples"
    );
}

/// The per-tick fold: a stable generation keeps the user's selection; a
/// generation advance (a confirmed hot-plug) resets it to the ADR default.
#[gpui::test]
async fn gpu_chart_metric_generation_change_resets_to_default(cx: &mut TestAppContext) {
    use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric;

    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        seed_chart_metric_gpu(v, "gpu:chart:gen", [(40.0, 60.0), (50.0, 62.0)]);
        v.system_snapshot_mut_for_test().gpu = vec![chart_metric_gpu("gpu:chart:gen", 50.0, 62.0)];
        v.selected = SelectedDevice::Gpu(0);
        v.reconcile_gpu_chart_metric();
        cx.notify();
    });
    view.update(cx, |v, cx| {
        v.select_gpu_chart_metric(GpuChartMetric::Temperature, 0, cx);
    });
    view.update(cx, |v, _cx| {
        v.reconcile_gpu_chart_metric();
    });
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.shell.gpu_chart_metric_selected(),
            GpuChartMetric::Temperature,
            "a stable generation keeps the selection"
        );
    });

    view.update(cx, |v, cx| {
        v.system_snapshot_mut_for_test().gpu[0].device_generation = DeviceGeneration::new(2);
        v.reconcile_gpu_chart_metric();
        cx.notify();
    });
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.shell.gpu_chart_metric_selected(),
            GpuChartMetric::Utilization,
            "a generation change resets the selection to the ADR default"
        );
    });
    draw(cx, win);
}
