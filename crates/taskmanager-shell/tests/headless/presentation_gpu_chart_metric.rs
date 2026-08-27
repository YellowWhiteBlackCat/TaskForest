//! Behavior tests for the ADR-034 stage-1 chart-metric selection contract:
//! default, availability gate, unavailable fallback, fixed cycle order,
//! generation reset, and per-device/per-field isolation.

use super::*;
use taskmanager_telemetry_store::GpuMetricPoint;

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

/// A point where every telemetry-store GPU series family carries a value:
/// 2 GiB/8 GiB, 1 GiB/4 GiB and 512 MiB/2 GiB are all 25%.
fn full_point() -> GpuMetricPoint {
    GpuMetricPoint {
        utilization_pct: Some(12.5),
        temperature_c: Some(61.0),
        memory_used_bytes: Some(2 * GIB),
        memory_total_bytes: Some(8 * GIB),
        dedicated_memory_used_bytes: Some(GIB),
        dedicated_memory_total_bytes: Some(4 * GIB),
        shared_memory_used_bytes: Some(512 * MIB),
        shared_memory_total_bytes: Some(2 * GIB),
        power_w: Some(88.5),
        frequency_mhz: Some(2_400),
        idle_residency_pct: Some(40.0),
    }
}

fn availability(point: &GpuMetricPoint) -> GpuChartMetricAvailability {
    GpuChartMetricAvailability::from_gpu_metric_point(point)
}

fn choice_of(
    projection: &GpuChartMetricProjection,
    metric: GpuChartMetric,
) -> &GpuChartMetricChoice {
    projection
        .choices
        .iter()
        .find(|choice| choice.metric == metric)
        .unwrap_or_else(|| panic!("vocabulary must keep {metric:?} present"))
}

#[test]
fn default_selection_is_utilization_before_any_reconcile() {
    let selection = GpuChartMetricSelection::new();
    assert_eq!(selection.selected(), GpuChartMetric::Utilization);
    assert_eq!(GpuChartMetric::DEFAULT, GpuChartMetric::Utilization);
    assert_eq!(selection.reconciled_generation(), None);
    assert_eq!(GpuChartMetricSelection::default(), selection);
}

#[test]
fn availability_gate_rejects_unavailable_selections_and_reselection_is_a_noop() {
    let mut point = full_point();
    point.temperature_c = None;
    let gate = availability(&point);
    let mut selection = GpuChartMetricSelection::new();
    selection.reconcile(&gate, 4);

    assert!(
        !selection.select(GpuChartMetric::Temperature, &gate),
        "an unavailable family must be rejected, not selected-then-degraded"
    );
    assert_eq!(selection.selected(), GpuChartMetric::Utilization);

    assert!(selection.select(GpuChartMetric::Power, &gate));
    assert!(!selection.select(GpuChartMetric::Power, &gate));
    assert_eq!(selection.selected(), GpuChartMetric::Power);
}

#[test]
fn selected_series_becoming_unavailable_falls_back_to_default_under_the_same_generation() {
    let healthy = availability(&full_point());
    let mut selection = GpuChartMetricSelection::new();
    selection.reconcile(&healthy, 7);
    assert!(selection.select(GpuChartMetric::Power, &healthy));

    let mut degraded_point = full_point();
    degraded_point.power_w = None;
    let degraded = availability(&degraded_point);

    assert!(
        selection.reconcile(&degraded, 7),
        "unavailability alone (same generation) must trigger the fallback"
    );
    assert_eq!(selection.selected(), GpuChartMetric::DEFAULT);

    // The failed family does not poison the others: temperature is still
    // selectable right after the fallback.
    assert!(selection.select(GpuChartMetric::Temperature, &degraded));
}

#[test]
fn generation_change_resets_to_default_while_a_stable_generation_keeps_selection() {
    let healthy = availability(&full_point());
    let mut selection = GpuChartMetricSelection::new();
    selection.reconcile(&healthy, 3);
    assert!(selection.select(GpuChartMetric::Power, &healthy));

    assert!(
        !selection.reconcile(&healthy, 3),
        "stable input keeps Power"
    );
    assert_eq!(selection.selected(), GpuChartMetric::Power);

    assert!(
        selection.reconcile(&healthy, 4),
        "generation change must reset even though Power stayed available"
    );
    assert_eq!(selection.selected(), GpuChartMetric::Utilization);
    assert_eq!(selection.reconciled_generation(), Some(4));
}

#[test]
fn generation_zero_is_a_bound_generation_not_an_unbound_state() {
    let healthy = availability(&full_point());
    let mut selection = GpuChartMetricSelection::new();
    assert!(!selection.reconcile(&healthy, 0));
    assert_eq!(selection.reconciled_generation(), Some(0));
    assert!(selection.select(GpuChartMetric::Frequency, &healthy));
    assert!(
        selection.reconcile(&healthy, 1),
        "0 → 1 is a generation change, not unbound → bound"
    );
    assert_eq!(selection.selected(), GpuChartMetric::DEFAULT);
}

/// The first reconcile BINDS the viewed device without resetting: a
/// selection made on the first rendered frames (before any tick folded)
/// survives the first fold (stage 2 — the UI render path can paint pills
/// before the first 100 ms tick).
#[test]
fn first_reconcile_binds_without_resetting_a_pre_render_selection() {
    let healthy = availability(&full_point());
    let mut selection = GpuChartMetricSelection::new();
    assert!(selection.select(GpuChartMetric::Temperature, &healthy));
    assert!(
        !selection.reconcile(&healthy, 0),
        "unbound → 0 is the initial binding, not a device change"
    );
    assert_eq!(selection.selected(), GpuChartMetric::Temperature);
    assert_eq!(selection.reconciled_generation(), Some(0));
}

#[test]
fn unavailable_default_projects_selected_unavailable_without_hiding_or_swapping() {
    let mut point = full_point();
    point.utilization_pct = None;
    let gate = availability(&point);
    let mut selection = GpuChartMetricSelection::new();
    assert!(
        !selection.reconcile(&gate, 2),
        "the default cannot fall back any further: it stays put"
    );
    assert_eq!(selection.selected(), GpuChartMetric::Utilization);
    assert!(!selection.select(GpuChartMetric::Utilization, &gate));

    let projection = selection.projection(&gate);
    assert_eq!(projection.selected, GpuChartMetric::Utilization);
    assert_eq!(
        choice_of(&projection, GpuChartMetric::Utilization).state,
        GpuChartMetricChoiceState::SelectedUnavailable,
        "the selected-but-unavailable default must be explicit, not a zero"
    );
    assert_eq!(
        GpuChartMetric::Utilization.value(&point),
        None,
        "no fabricated utilization value may back the projection"
    );
    for choice in &projection.choices {
        if choice.metric == GpuChartMetric::Utilization {
            continue;
        }
        assert_eq!(
            choice.state,
            GpuChartMetricChoiceState::Selectable,
            "one unavailable family must not hide or disable the others"
        );
    }
}

#[test]
fn cycle_follows_the_fixed_adr_vocabulary_order_and_wraps() {
    let healthy = availability(&full_point());
    let mut selection = GpuChartMetricSelection::new();
    let expected = [
        GpuChartMetric::Power,
        GpuChartMetric::Temperature,
        GpuChartMetric::Frequency,
        GpuChartMetric::Memory,
        GpuChartMetric::DedicatedMemory,
        GpuChartMetric::SharedMemory,
        GpuChartMetric::IdleResidency,
        GpuChartMetric::Utilization,
    ];
    for next in expected {
        assert!(selection.cycle(&healthy));
        assert_eq!(selection.selected(), next);
    }
    assert!(selection.cycle(&healthy));
    assert_eq!(
        selection.selected(),
        GpuChartMetric::Power,
        "the cycle repeats deterministically after wrapping"
    );
}

#[test]
fn cycle_skips_unavailable_series_which_stay_visible_in_the_projection() {
    let mut point = full_point();
    point.temperature_c = None;
    let gate = availability(&point);
    let mut selection = GpuChartMetricSelection::new();
    selection.reconcile(&gate, 5);

    assert!(selection.cycle(&gate));
    assert_eq!(selection.selected(), GpuChartMetric::Power);
    assert!(selection.cycle(&gate));
    assert_eq!(
        selection.selected(),
        GpuChartMetric::Frequency,
        "the cycle must apply the gate and skip Temperature, not land on it"
    );

    let projection = selection.projection(&gate);
    assert_eq!(
        choice_of(&projection, GpuChartMetric::Temperature).state,
        GpuChartMetricChoiceState::Unavailable,
        "a skipped family stays present and explicitly unavailable"
    );
}

#[test]
fn cycle_and_reconcile_are_noops_when_no_series_is_available() {
    let empty = GpuChartMetricAvailability::unavailable();
    let mut selection = GpuChartMetricSelection::new();
    assert!(!selection.cycle(&empty));
    assert!(!selection.reconcile(&empty, 9));
    assert!(!selection.cycle(&empty));
    assert_eq!(selection.selected(), GpuChartMetric::DEFAULT);

    let projection = selection.projection(&empty);
    assert!(projection.choices.iter().all(|choice| {
        matches!(
            choice.state,
            GpuChartMetricChoiceState::Unavailable | GpuChartMetricChoiceState::SelectedUnavailable
        )
    }));
}

#[test]
fn memory_families_require_used_and_total_capacity() {
    let mut used_only = full_point();
    used_only.memory_total_bytes = None;
    let gate = availability(&used_only);
    assert!(!gate.is_available(GpuChartMetric::Memory));
    let mut selection = GpuChartMetricSelection::new();
    assert!(!selection.select(GpuChartMetric::Memory, &gate));

    let healthy_point = full_point();
    let healthy = availability(&healthy_point);
    assert!(healthy.is_available(GpuChartMetric::Memory));
    let percent = GpuChartMetric::Memory.value(&healthy_point);
    assert!(
        (percent.unwrap_or(f32::NAN) - 25.0).abs() < 1e-4,
        "2 GiB / 8 GiB"
    );

    let mut zero_capacity = full_point();
    zero_capacity.memory_total_bytes = Some(0);
    assert!(
        !availability(&zero_capacity).is_available(GpuChartMetric::Memory),
        "a zero capacity is unavailable, never a divide-by-zero percent"
    );
}

#[test]
fn chart_value_maps_each_family_to_its_telemetry_store_series() {
    let point = full_point();
    for (metric, expected) in [
        (GpuChartMetric::Utilization, point.utilization_pct),
        (GpuChartMetric::Power, point.power_w),
        (GpuChartMetric::Temperature, point.temperature_c),
        (GpuChartMetric::IdleResidency, point.idle_residency_pct),
    ] {
        assert_eq!(metric.value(&point), expected);
    }
    assert_eq!(
        GpuChartMetric::Frequency.value(&point),
        point.frequency_mhz.map(|mhz| mhz as f32)
    );
    assert!(
        (GpuChartMetric::DedicatedMemory
            .value(&point)
            .unwrap_or(f32::NAN)
            - 25.0)
            .abs()
            < 1e-4,
        "1 GiB / 4 GiB"
    );
    assert!(
        (GpuChartMetric::SharedMemory
            .value(&point)
            .unwrap_or(f32::NAN)
            - 25.0)
            .abs()
            < 1e-4,
        "512 MiB / 2 GiB"
    );
}

#[test]
fn one_devices_failed_series_never_affects_another_devices_availability() {
    let mut device_a_point = full_point();
    device_a_point.power_w = None;
    let device_a = availability(&device_a_point);
    let device_b = availability(&full_point());

    for metric in GpuChartMetric::ALL {
        if metric == GpuChartMetric::Power {
            assert!(!device_a.is_available(metric));
            assert!(device_b.is_available(metric));
        } else {
            assert_eq!(
                device_a.is_available(metric),
                device_b.is_available(metric),
                "one failed field must not disturb any other field"
            );
        }
    }

    let mut selection = GpuChartMetricSelection::new();
    assert!(!selection.select(GpuChartMetric::Power, &device_a));
    assert!(selection.select(GpuChartMetric::Power, &device_b));
}

// ── Sampling-window anchor (single-track convergence) ──────────────────────

/// The dispatch's Utilization window must stay value-identical to the
/// aggregate per-device usage ring the sidebar/rail still reads: both derive
/// from the same accepted observation, so a drift here would mean the typed
/// point fold and the usage ring fold stopped being one fact's two
/// projections. Seeded with one utilization gap to pin the NaN position too.
#[test]
fn chart_metric_utilization_window_matches_the_usage_ring() {
    use std::collections::BTreeMap;

    use taskmanager_application::{
        DeviceGeneration, DeviceId, DeviceLifecycle, DevicePresence, DeviceState, GpuMetrics,
        GpuScalarObservations, ScalarObservation,
    };
    use taskmanager_telemetry_store::CorrelatedTelemetryStamp;

    const DEVICE: &str = "gpu:anchor:utilization";
    let (history, ingestor) = crate::history::LiveGraphHistory::shared(64);

    let observation = |utilization: Option<f32>, observed_at_ms: u64| {
        let mut gpu = GpuMetrics::new(DEVICE, "Anchor GPU");
        gpu.device_generation = DeviceGeneration::new(1);
        gpu.device_state = DeviceState::healthy(observed_at_ms);
        let utilization = utilization.map_or_else(
            || ScalarObservation::unavailable(taskmanager_application::FailureKind::Unsupported),
            |value| ScalarObservation::available(value, observed_at_ms),
        );
        gpu.apply_scalar_observations(GpuScalarObservations {
            utilization_pct: utilization,
            ..GpuScalarObservations::default()
        });
        let lifecycle = (
            DeviceId::new(DEVICE.to_owned()),
            DeviceLifecycle {
                presence: DevicePresence::Present,
                state: DeviceState::healthy(observed_at_ms),
                generation: 1,
                first_seen_ms: Some(observed_at_ms),
                last_seen_ms: Some(observed_at_ms),
                absent_since_ms: None,
            },
        );
        taskmanager_application::GpuTelemetryObservation::current(
            vec![gpu],
            observed_at_ms,
            Vec::new(),
            Vec::new(),
            BTreeMap::from([lifecycle]),
        )
    };

    for (revision, (utilization, timestamp)) in [(Some(10.0), 10u64), (None, 20), (Some(30.0), 30)]
        .into_iter()
        .enumerate()
    {
        let revision = u64::try_from(revision + 1).expect("small revision");
        ingestor
            .ingest_correlated_gpu(
                CorrelatedTelemetryStamp::from_accepted_event(revision, timestamp + 1)
                    .expect("fixture revision is non-zero"),
                &observation(utilization, timestamp),
            )
            .expect("anchor fixture enters system history");
    }

    let chart = gpu_chart_metric_history(&history, DEVICE, 1, GpuChartMetric::Utilization);
    let ring = history.gpu_usage_pct_for(DEVICE, 1);
    assert_eq!(chart.len(), ring.len());
    assert!(
        chart
            .iter()
            .zip(&ring)
            .all(|(left, right)| left.to_bits() == right.to_bits()),
        "the typed-point fold and the usage ring disagree: {chart:?} vs {ring:?}"
    );
    assert_eq!(chart.len(), 3);
    assert_eq!(chart[0], 10.0);
    assert!(
        chart[1].is_nan(),
        "the unobserved frame is a gap, never zero"
    );
    assert_eq!(chart[2], 30.0);
}
