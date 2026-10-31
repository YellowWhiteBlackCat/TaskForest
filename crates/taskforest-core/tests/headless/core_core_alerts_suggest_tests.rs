use super::*;

/// Repeat `value` `count` times, then append the spikes.
fn samples(base: f32, count: usize, spikes: &[f32]) -> Vec<f32> {
    let mut out = vec![base; count];
    out.extend_from_slice(spikes);
    out
}

#[test]
fn flat_workload_suggests_mean_with_low_confidence() {
    // 20 samples all at 50%: mean=50, stddev=0, p95=50.
    let snapshot =
        RollingStatSnapshot::from_samples(&samples(50.0, 20, &[])).expect("non-empty window");
    let suggestion = AlertEngine::suggest_threshold(AlertMetric::CpuUsagePercent, &snapshot);
    assert_eq!(
        suggestion,
        SuggestedThreshold::Suggested {
            metric: AlertMetric::CpuUsagePercent,
            threshold: 50.0,
            // 50 * 0.05 = 2.5 > HYSTERESIS_FLOOR
            hysteresis: 2.5,
            basis: SuggestionBasis::MeanPlusStddevFloorP95,
            sample_count: 20,
            confidence: SuggestionConfidence::Low,
        }
    );
}

#[test]
fn high_variance_workload_clamps_to_sane_ceiling() {
    // 10 samples at 10%, 10 at 90%: mean=50, stddev=40, p95=90.
    // mean + 3*sigma = 170 -> clamped to 99%.
    let window = {
        let mut v = vec![10.0; 10];
        v.extend(std::iter::repeat_n(90.0, 10));
        v
    };
    let snapshot = RollingStatSnapshot::from_samples(&window).expect("non-empty window");
    assert_eq!(snapshot.mean, 50.0);
    assert!((snapshot.population_stddev - 40.0).abs() < 1e-3);
    assert_eq!(snapshot.p95, 90.0);

    let suggestion = AlertEngine::suggest_threshold(AlertMetric::CpuUsagePercent, &snapshot);
    assert_eq!(
        suggestion,
        SuggestedThreshold::Suggested {
            metric: AlertMetric::CpuUsagePercent,
            threshold: 99.0,
            hysteresis: 99.0 * 0.05, // 4.95
            basis: SuggestionBasis::MeanPlusStddevFloorP95,
            sample_count: 20,
            confidence: SuggestionConfidence::Low,
        }
    );
}

#[test]
fn spiky_workload_floor_keeps_threshold_at_observed_tail() {
    // 18 samples at 20%, 2 at 80%: mean=26, stddev=18, p95=80.
    // mean + 3*sigma = 80; p95 = 80 -> raw 80.
    let window = {
        let mut v = vec![20.0; 18];
        v.extend(std::iter::repeat_n(80.0, 2));
        v
    };
    let snapshot = RollingStatSnapshot::from_samples(&window).expect("non-empty window");
    assert!((snapshot.mean - 26.0).abs() < 1e-3);
    assert!((snapshot.population_stddev - 18.0).abs() < 1e-3);
    assert_eq!(snapshot.p95, 80.0);

    let suggestion = AlertEngine::suggest_threshold(AlertMetric::CpuUsagePercent, &snapshot);
    assert_eq!(
        suggestion,
        SuggestedThreshold::Suggested {
            metric: AlertMetric::CpuUsagePercent,
            threshold: 80.0,
            hysteresis: 4.0,
            basis: SuggestionBasis::MeanPlusStddevFloorP95,
            sample_count: 20,
            confidence: SuggestionConfidence::Low,
        }
    );
}

#[test]
fn large_window_promotes_to_high_confidence() {
    // 40 samples at 60% memory.
    let snapshot =
        RollingStatSnapshot::from_samples(&samples(60.0, 40, &[])).expect("non-empty window");
    let suggestion = AlertEngine::suggest_threshold(AlertMetric::MemoryUsagePercent, &snapshot);
    assert_eq!(
        suggestion,
        SuggestedThreshold::Suggested {
            metric: AlertMetric::MemoryUsagePercent,
            threshold: 60.0,
            hysteresis: 3.0,
            basis: SuggestionBasis::MeanPlusStddevFloorP95,
            sample_count: 40,
            confidence: SuggestionConfidence::High,
        }
    );
}

#[test]
fn too_few_samples_is_typed_insufficient_not_fabricated() {
    // 5 samples: below the 20-sample principled floor.
    let snapshot =
        RollingStatSnapshot::from_samples(&samples(80.0, 5, &[])).expect("non-empty window");
    let suggestion = AlertEngine::suggest_threshold(AlertMetric::CpuUsagePercent, &snapshot);
    assert_eq!(
        suggestion,
        SuggestedThreshold::Insufficient {
            sample_count: 5,
            required: SUGGESTION_MIN_SAMPLES,
            reason: InsufficientReason::TooFewSamples,
        }
    );
}

#[test]
fn binary_smart_warning_metric_is_unsupported() {
    // Even with a full window, a 0/1 metric has no useful numeric threshold.
    let snapshot =
        RollingStatSnapshot::from_samples(&samples(0.0, 30, &[])).expect("non-empty window");
    let suggestion = AlertEngine::suggest_threshold(AlertMetric::SmartCriticalWarning, &snapshot);
    assert_eq!(
        suggestion,
        SuggestedThreshold::Insufficient {
            sample_count: 30,
            required: SUGGESTION_MIN_SAMPLES,
            reason: InsufficientReason::UnsupportedMetric,
        }
    );
}

#[test]
fn disk_temperature_uses_temperature_sane_bounds() {
    // 20 samples at 45 °C -> threshold 45, hysteresis 45*0.05 = 2.25.
    let snapshot =
        RollingStatSnapshot::from_samples(&samples(45.0, 20, &[])).expect("non-empty window");
    let suggestion = AlertEngine::suggest_threshold(AlertMetric::DiskTemperatureC, &snapshot);
    assert_eq!(
        suggestion,
        SuggestedThreshold::Suggested {
            metric: AlertMetric::DiskTemperatureC,
            threshold: 45.0,
            hysteresis: 2.25,
            basis: SuggestionBasis::MeanPlusStddevFloorP95,
            sample_count: 20,
            confidence: SuggestionConfidence::Low,
        }
    );
}

#[test]
fn disk_temperature_clamps_below_ambient_up_to_sane_floor() {
    // Degenerate window (all 10 °C, below ambient): raw 10 -> clamped to 30.
    let snapshot =
        RollingStatSnapshot::from_samples(&samples(10.0, 20, &[])).expect("non-empty window");
    let suggestion = AlertEngine::suggest_threshold(AlertMetric::DiskTemperatureC, &snapshot);
    assert!(
        matches!(
            suggestion,
            SuggestedThreshold::Suggested {
                threshold: 30.0,
                ..
            }
        ),
        "degenerate below-ambient window should clamp to the temperature floor: {suggestion:?}"
    );
}

#[test]
fn from_samples_drops_non_finite_and_returns_none_on_empty() {
    // All non-finite -> None.
    assert!(
        RollingStatSnapshot::from_samples(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY]).is_none()
    );

    // One finite among non-finite -> single-sample snapshot.
    let snapshot = RollingStatSnapshot::from_samples(&[f32::NAN, 42.0, f32::INFINITY])
        .expect("one finite sample yields a snapshot");
    assert_eq!(snapshot.sample_count, 1);
    assert_eq!(snapshot.mean, 42.0);
    assert_eq!(snapshot.population_stddev, 0.0);
    assert_eq!(snapshot.p95, 42.0);
    assert_eq!(snapshot.min, 42.0);
    assert_eq!(snapshot.max, 42.0);
}

#[test]
fn suggested_threshold_round_trips_through_serde_json() {
    let suggested = SuggestedThreshold::Suggested {
        metric: AlertMetric::CpuUsagePercent,
        threshold: 87.5,
        hysteresis: 4.375,
        basis: SuggestionBasis::MeanPlusStddevFloorP95,
        sample_count: 42,
        confidence: SuggestionConfidence::High,
    };
    let json = serde_json::to_string(&suggested).expect("serialize suggested");
    assert!(
        json.contains("\"MeanPlusStddevFloorP95\""),
        "basis strategy must survive serialization: {json}"
    );
    let back: SuggestedThreshold = serde_json::from_str(&json).expect("deserialize suggested");
    assert_eq!(back, suggested);

    let insufficient = SuggestedThreshold::Insufficient {
        sample_count: 3,
        required: SUGGESTION_MIN_SAMPLES,
        reason: InsufficientReason::TooFewSamples,
    };
    let json = serde_json::to_string(&insufficient).expect("serialize insufficient");
    let back: SuggestedThreshold = serde_json::from_str(&json).expect("deserialize insufficient");
    assert_eq!(back, insufficient);
}
