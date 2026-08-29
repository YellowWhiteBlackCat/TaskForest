use super::*;
use taskmanager_core::core::alerts::{
    AlertMetric, InsufficientReason, SUGGESTION_MIN_SAMPLES, SuggestedThreshold,
};
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, ScalarObservation, SystemSnapshot,
};

#[test]
fn empty_window_is_typed_insufficient() {
    let window = AlertSuggestionWindow::new();
    assert_eq!(
        window.suggest(AlertMetric::CpuUsagePercent),
        SuggestedThreshold::Insufficient {
            sample_count: 0,
            required: SUGGESTION_MIN_SAMPLES,
            reason: InsufficientReason::TooFewSamples,
        }
    );
    assert_eq!(
        window.suggest(AlertMetric::SmartCriticalWarning),
        SuggestedThreshold::Insufficient {
            sample_count: 0,
            required: SUGGESTION_MIN_SAMPLES,
            reason: InsufficientReason::UnsupportedMetric,
        }
    );
}

#[test]
fn window_retains_only_finite_suggestion_evidence_and_is_bounded() {
    let mut window = AlertSuggestionWindow::new();
    window.set_capacity(10);
    for value in 0_u8..20 {
        window.record_snapshot(&SystemSnapshot {
            cpu: CpuMetrics::from_observations(CpuScalarObservations {
                global_usage_pct: ScalarObservation::available(f32::from(value), u64::from(value)),
                ..Default::default()
            }),
            ..SystemSnapshot::default()
        });
    }
    window.record_snapshot(&SystemSnapshot {
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(f32::NAN, 20),
            ..Default::default()
        }),
        ..SystemSnapshot::default()
    });
    assert_eq!(window.sample_count(AlertMetric::CpuUsagePercent), 10);
}

#[test]
fn smart_temperature_is_retained_only_as_suggestion_evidence() {
    let mut window = AlertSuggestionWindow::new();
    window.record_snapshot(&SystemSnapshot {
        disks: vec![
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .smart_temperature_c(Some(40.0))
                .build(),
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .smart_temperature_c(Some(45.0))
                .build(),
        ],
        ..SystemSnapshot::default()
    });
    assert_eq!(window.sample_count(AlertMetric::DiskTemperatureC), 2);
}
