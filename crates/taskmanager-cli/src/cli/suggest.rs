//! `--suggest-thresholds` rendering: pure helpers that shape a single
//! point-in-time [`SystemSnapshot`] into the per-metric threshold-suggestion
//! JSON document.
//!
//! Honesty contract (the project red line): a metric with no finite sample in
//! the snapshot serializes as JSON `null` (honest absence); the binary
//! `smart_critical_warning` metric is permanently `unsupported_metric`; a
//! metric that observed samples but fewer than [`SUGGESTION_MIN_SAMPLES`] is
//! rendered as the engine's `TooFewSamples` verdict — never a fabricated
//! threshold.

use taskmanager_core::SystemSnapshot;
use taskmanager_core::core::alerts::{
    AlertEngine, AlertMetric, InsufficientReason, RollingStatSnapshot, SUGGESTION_MIN_SAMPLES,
    SuggestedThreshold, SuggestionBasis, SuggestionConfidence,
};

/// Snake-case JSON key for an [`AlertMetric`], matching its serde rename so
/// the suggestion document's keys stay in sync with the enum spelling and with
/// the alert-rule transfer file (`cpu_usage_percent`, ...).
fn metric_key(metric: AlertMetric) -> &'static str {
    match metric {
        AlertMetric::CpuUsagePercent => "cpu_usage_percent",
        AlertMetric::MemoryUsagePercent => "memory_usage_percent",
        AlertMetric::DiskTemperatureC => "disk_temperature_c",
        AlertMetric::SmartPercentUsed => "smart_percent_used",
        AlertMetric::SmartCriticalWarning => "smart_critical_warning",
    }
}

/// Snake-case reason label for an [`InsufficientReason`], matching the rest of
/// the alert payload's spelling.
fn insufficient_reason_str(reason: InsufficientReason) -> &'static str {
    match reason {
        InsufficientReason::TooFewSamples => "too_few_samples",
        InsufficientReason::UnsupportedMetric => "unsupported_metric",
    }
}

/// Snake-case label for a [`SuggestionBasis`] strategy.
fn suggestion_basis_str(basis: SuggestionBasis) -> &'static str {
    match basis {
        SuggestionBasis::MeanPlusStddevFloorP95 => "mean_plus_stddev_floor_p95",
    }
}

/// Snake-case label for a [`SuggestionConfidence`] band.
fn suggestion_confidence_str(confidence: SuggestionConfidence) -> &'static str {
    match confidence {
        SuggestionConfidence::Low => "low",
        SuggestionConfidence::High => "high",
    }
}

/// Shape a [`SuggestedThreshold`] as a JSON value: a `Suggested` verdict keeps
/// its threshold, hysteresis, derivation basis, sample count, and confidence;
/// an `Insufficient` verdict carries its typed `reason`, `sample_count`, and
/// the `required` floor — and NEVER a fabricated `threshold` field.
pub(super) fn shape_suggestion(suggestion: &SuggestedThreshold) -> serde_json::Value {
    match suggestion {
        SuggestedThreshold::Suggested {
            metric,
            threshold,
            hysteresis,
            basis,
            sample_count,
            confidence,
        } => serde_json::json!({
            "status": "suggested",
            "metric": metric_key(*metric),
            "threshold": threshold,
            "hysteresis": hysteresis,
            "basis": suggestion_basis_str(*basis),
            "sample_count": sample_count,
            "confidence": suggestion_confidence_str(*confidence),
        }),
        SuggestedThreshold::Insufficient {
            sample_count,
            required,
            reason,
        } => serde_json::json!({
            "status": "insufficient",
            "reason": insufficient_reason_str(*reason),
            "sample_count": sample_count,
            "required": required,
        }),
    }
}

/// Collect the finite samples a single [`SystemSnapshot`] can contribute to a
/// metric's rolling window. A point-in-time snapshot yields at most one sample
/// per system-wide metric and one sample per disk for the disk/SMART metrics;
/// the binary SMART-warning metric has no useful numeric series and yields none.
fn collect_metric_samples(metric: AlertMetric, snapshot: &SystemSnapshot) -> Vec<f32> {
    match metric {
        AlertMetric::CpuUsagePercent => snapshot
            .cpu
            .current_global_usage_pct()
            .into_iter()
            .filter(|value| value.is_finite())
            .collect(),
        AlertMetric::MemoryUsagePercent => snapshot
            .memory
            .used_percentage_observed()
            .into_iter()
            .filter(|value| value.is_finite())
            .collect(),
        AlertMetric::DiskTemperatureC => snapshot
            .disks
            .iter()
            .filter_map(|disk| disk.smart_temperature_c)
            .filter(|value| value.is_finite())
            .collect(),
        AlertMetric::SmartPercentUsed => snapshot
            .disks
            .iter()
            .filter_map(|disk| disk.smart_percent_used)
            .filter(|value| value.is_finite())
            .collect(),
        AlertMetric::SmartCriticalWarning => Vec::new(),
    }
}

/// Build a pretty-printed JSON object keyed by metric, carrying each metric's
/// [`SuggestedThreshold`] derived from the single snapshot's samples.
///
/// Honesty contract (the project red line): a metric with NO finite sample in
/// this snapshot serializes as JSON `null` (honest absence) — EXCEPT the binary
/// `smart_critical_warning` metric, which is permanently unsupported and is
/// named as `{"status":"insufficient","reason":"unsupported_metric",...}` so a
/// consumer is not left guessing. A metric that DID observe samples, but fewer
/// than the [`SUGGESTION_MIN_SAMPLES`] floor, is rendered as the engine's
/// `Insufficient { TooFewSamples }` verdict — NEVER a fabricated threshold.
pub(super) fn suggest_thresholds_json(snapshot: &SystemSnapshot) -> String {
    let metrics = [
        AlertMetric::CpuUsagePercent,
        AlertMetric::MemoryUsagePercent,
        AlertMetric::DiskTemperatureC,
        AlertMetric::SmartPercentUsed,
        AlertMetric::SmartCriticalWarning,
    ];
    let mut object = serde_json::Map::new();
    for metric in metrics {
        let samples = collect_metric_samples(metric, snapshot);
        let value = match RollingStatSnapshot::from_samples(&samples) {
            None => {
                if matches!(metric, AlertMetric::SmartCriticalWarning) {
                    serde_json::json!({
                        "status": "insufficient",
                        "reason": insufficient_reason_str(InsufficientReason::UnsupportedMetric),
                        "sample_count": 0,
                        "required": SUGGESTION_MIN_SAMPLES,
                    })
                } else {
                    serde_json::Value::Null
                }
            }
            Some(rolling) => shape_suggestion(&AlertEngine::suggest_threshold(metric, &rolling)),
        };
        object.insert(metric_key(metric).to_string(), value);
    }
    // `to_string_pretty` is infallible here: keys are plain strings, the shaped
    // values are finite numbers/strings/null (the engine clamps thresholds to a
    // finite sane range and from_samples drops non-finite input).
    serde_json::to_string_pretty(&serde_json::Value::Object(object))
        .expect("threshold object serialization is infallible")
}
