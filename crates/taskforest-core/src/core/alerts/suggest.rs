//! Rolling-statistic threshold *suggestions* for the alert engine (innovation #7).
//!
//! Most system monitors force a user to guess "what CPU% is abnormal for my
//! workload?". This module proposes a principled threshold from the rolling
//! statistics the app already keeps, so a user starts from a defensible number
//! instead of a guess. The result is always honest: too few samples, or a
//! metric the heuristic does not model, return a typed
//! [`SuggestedThreshold::Insufficient`] rather than a fabricated value.
//!
//! This module is pure: no I/O, no GPUI, no platform code. The caller reduces
//! a bounded window of observed samples into a [`RollingStatSnapshot`] (the
//! ring-buffer history itself lives in `taskmanager-telemetry-store`, which
//! depends on this crate; reducing its samples into a snapshot happens at the
//! application/UI composition edge).

use serde::{Deserialize, Serialize};

use super::{AlertEngine, AlertMetric};

/// Smallest rolling window for which a suggestion is principled. Below this,
/// [`AlertEngine::suggest_threshold`] returns
/// [`SuggestedThreshold::Insufficient`].
///
/// Rationale: with fewer than 20 finite samples the nearest-rank p95 collapses
/// to the sample maximum and the population standard deviation is too noisy to
/// support a three-sigma floor. Requiring 20 keeps p95 distinct from the peak
/// and the proposal defensible.
pub const SUGGESTION_MIN_SAMPLES: usize = 20;

/// Number of population standard deviations added to the mean when forming the
/// statistical floor of the suggestion. `3.0` is the classic three-sigma rule:
/// for an approximately normal workload, values above `mean + 3·σ` sit in the
/// rare upper tail and are reasonable "abnormal high" candidates.
pub const SUGGESTION_SIGMA_K: f32 = 3.0;

/// Sample count at/above which a suggestion is reported as
/// [`SuggestionConfidence::High`]. Below it, the proposal is still principled
/// (it cleared [`SUGGESTION_MIN_SAMPLES`]) but based on a shorter window.
pub const SUGGESTION_CONFIDENCE_HIGH_MIN_SAMPLES: usize = 40;

/// Hysteresis is proposed as this fraction of the suggested threshold, so the
/// clear-band scales with the metric instead of being a fixed offset.
const HYSTERESIS_FRACTION: f32 = 0.05;
/// Floor for the proposed hysteresis, in metric units (percentage points or
/// degrees Celsius). Keeps a low threshold from getting a noise-level band.
const HYSTERESIS_FLOOR: f32 = 2.0;

/// Typed reduction of a bounded window of observed samples to just the moments
/// and order statistics the suggestion heuristic consumes.
///
/// Construct it from real samples via [`RollingStatSnapshot::from_samples`];
/// never assemble it by hand from guessed fields (that would defeat the
/// "observed, not invented" contract).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RollingStatSnapshot {
    /// Number of finite samples in the window. Drives the insufficient-data
    /// policy in [`AlertEngine::suggest_threshold`].
    pub sample_count: usize,
    /// Arithmetic mean of the finite samples.
    pub mean: f32,
    /// Population standard deviation (the window is treated as the whole
    /// observed population, not a sample of a larger one).
    pub population_stddev: f32,
    /// 95th percentile via the nearest-rank method on the finite samples.
    pub p95: f32,
    /// Smallest finite sample.
    pub min: f32,
    /// Largest finite sample.
    pub max: f32,
}

impl RollingStatSnapshot {
    /// Reduce a raw window to the snapshot the heuristic consumes.
    ///
    /// Non-finite values (`NaN`, infinities) are dropped before any statistic
    /// is computed, so a missing sensor reading cannot poison the proposal.
    /// Returns `None` only when there is no finite sample at all; a single
    /// finite sample yields a valid (if policy-insufficient) snapshot with a
    /// zero standard deviation.
    #[must_use]
    pub fn from_samples(samples: &[f32]) -> Option<Self> {
        // f64 accumulator for numerical stability while forming the mean and
        // the squared deviations, narrowed back to f32 for storage.
        let mut finite: Vec<f32> = Vec::with_capacity(samples.len());
        for &value in samples {
            if value.is_finite() {
                finite.push(value);
            }
        }
        let count = finite.len();
        if count == 0 {
            return None;
        }
        let mut sum = 0_f64;
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for &value in &finite {
            let v = f64::from(value);
            sum += v;
            min = min.min(v);
            max = max.max(v);
        }
        let mean = sum / count as f64;
        let mut variance_sum = 0_f64;
        for &value in &finite {
            let deviation = f64::from(value) - mean;
            variance_sum += deviation * deviation;
        }
        let population_stddev = (variance_sum / count as f64).sqrt();
        let p95 = percentile_nearest_rank(&finite, 95.0);
        Some(Self {
            sample_count: count,
            mean: mean as f32,
            population_stddev: population_stddev as f32,
            p95,
            min: min as f32,
            max: max as f32,
        })
    }
}

/// Nearest-rank percentile on a non-empty slice of finite samples.
///
/// Sorts a copy ascending, takes the element at 1-based rank
/// `ceil(percentile / 100 · n)`, clamped to the available range. With
/// `n >= 20` and `percentile = 95` the rank lands below the maximum, so p95
/// stops collapsing to the peak. Defensive on empty input (returns 0.0) even
/// though [`RollingStatSnapshot::from_samples`] only calls this with at least
/// one sample.
fn percentile_nearest_rank(samples: &[f32], percentile: f32) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f32> = samples.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let n = sorted.len();
    let rank = ((percentile / 100.0) * n as f32).ceil() as usize;
    let index = rank.saturating_sub(1).min(n - 1);
    sorted[index]
}

/// How a suggested threshold was derived. Recorded on the suggestion so a UI
/// or audit log can answer "why this number?" instead of presenting a bare
/// value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionBasis {
    /// `max(mean + SUGGESTION_SIGMA_K · population_stddev, p95)`, clamped to
    /// the metric's sane bounds. The `max` keeps the proposal above a
    /// genuinely observed high tail even for low-variance workloads; the clamp
    /// prevents degenerate ceilings.
    MeanPlusStddevFloorP95,
}

/// Coarse confidence in a suggestion, derived from the rolling window size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionConfidence {
    /// Window met [`SUGGESTION_MIN_SAMPLES`] but is shorter than
    /// [`SUGGESTION_CONFIDENCE_HIGH_MIN_SAMPLES`].
    Low,
    /// Window is at/above [`SUGGESTION_CONFIDENCE_HIGH_MIN_SAMPLES`].
    High,
}

/// Why a suggestion could not be produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsufficientReason {
    /// The rolling window had fewer than [`SUGGESTION_MIN_SAMPLES`] finite
    /// samples. Proposing from a shorter window would be a guess.
    TooFewSamples,
    /// The metric is binary (e.g. SMART critical-warning) and has no
    /// meaningful numeric threshold to suggest; its only useful threshold is
    /// a fixed `1.0` (warning present).
    UnsupportedMetric,
}

/// Honest, principled threshold proposal.
///
/// Constructed only by [`AlertEngine::suggest_threshold`]. The insufficient
/// path is a typed variant rather than a sentinel value: callers cannot
/// accidentally use a fabricated threshold when there is not enough data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SuggestedThreshold {
    /// No principled proposal is possible from the supplied window.
    Insufficient {
        sample_count: usize,
        /// Minimum sample count the heuristic requires for this reason.
        required: usize,
        reason: InsufficientReason,
    },
    /// A principled threshold with its derivation recorded.
    Suggested {
        metric: AlertMetric,
        threshold: f32,
        /// Suggested clear-band width, ready to drop straight into an
        /// [`super::AlertRule`].
        hysteresis: f32,
        basis: SuggestionBasis,
        sample_count: usize,
        confidence: SuggestionConfidence,
    },
}

impl AlertEngine {
    /// Propose a threshold for `metric` from observed rolling statistics.
    ///
    /// Formula:
    /// ```text
    /// raw        = max(mean + SUGGESTION_SIGMA_K · population_stddev, p95)
    /// threshold  = clamp(raw, sane_min(metric), sane_max(metric))
    /// hysteresis = clamp(threshold · HYSTERESIS_FRACTION, HYSTERESIS_FLOOR, ..)
    /// ```
    ///
    /// `mean + k·σ` is the three-sigma rule (rare upper tail of an
    /// approximately normal workload); the `p95` floor ensures the proposal
    /// stays above a genuinely observed high tail even for heavy-tailed or
    /// low-variance workloads. The clamp keeps the suggestion inside an
    /// observable, useful range for each metric.
    ///
    /// Honesty: with fewer than [`SUGGESTION_MIN_SAMPLES`] finite samples, or
    /// for a metric the heuristic does not model
    /// ([`AlertMetric::SmartCriticalWarning`]), this returns
    /// [`SuggestedThreshold::Insufficient`] — it never invents a value.
    #[must_use]
    pub fn suggest_threshold(
        metric: AlertMetric,
        snapshot: &RollingStatSnapshot,
    ) -> SuggestedThreshold {
        // Binary metric: there is no useful numeric threshold to propose.
        if matches!(metric, AlertMetric::SmartCriticalWarning) {
            return SuggestedThreshold::Insufficient {
                sample_count: snapshot.sample_count,
                required: SUGGESTION_MIN_SAMPLES,
                reason: InsufficientReason::UnsupportedMetric,
            };
        }

        if snapshot.sample_count < SUGGESTION_MIN_SAMPLES {
            return SuggestedThreshold::Insufficient {
                sample_count: snapshot.sample_count,
                required: SUGGESTION_MIN_SAMPLES,
                reason: InsufficientReason::TooFewSamples,
            };
        }

        let (sane_min, sane_max) = sane_bounds(metric);
        let statistical_floor = snapshot.mean + SUGGESTION_SIGMA_K * snapshot.population_stddev;
        let raw = statistical_floor.max(snapshot.p95);
        // `.max(min).min(max)` form clamps without panicking on an NaN; every
        // snapshot field is finite by construction, so this is a plain clamp.
        let threshold = raw.max(sane_min).min(sane_max);
        let hysteresis = (threshold * HYSTERESIS_FRACTION).max(HYSTERESIS_FLOOR);

        let confidence = if snapshot.sample_count >= SUGGESTION_CONFIDENCE_HIGH_MIN_SAMPLES {
            SuggestionConfidence::High
        } else {
            SuggestionConfidence::Low
        };

        SuggestedThreshold::Suggested {
            metric,
            threshold,
            hysteresis,
            basis: SuggestionBasis::MeanPlusStddevFloorP95,
            sample_count: snapshot.sample_count,
            confidence,
        }
    }
}

/// Per-metric sane clamp range. Percentages stay in an observable band
/// (`[1, 99]`) so the proposal is neither below noise nor at the unobservable
/// ceiling; disk temperature stays inside `[30, 80]` °C (below ambient is
/// noise, at/above 80 °C already sits in the SMART critical zone where a
/// threshold is moot). The binary SMART-warning case is unreachable here (the
/// engine returns early), bounds are kept for completeness.
fn sane_bounds(metric: AlertMetric) -> (f32, f32) {
    match metric {
        AlertMetric::CpuUsagePercent
        | AlertMetric::MemoryUsagePercent
        | AlertMetric::SmartPercentUsed => (1.0, 99.0),
        AlertMetric::DiskTemperatureC => (30.0, 80.0),
        AlertMetric::SmartCriticalWarning => (0.0, 1.0),
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_alerts_suggest_tests.rs"]
mod tests;
