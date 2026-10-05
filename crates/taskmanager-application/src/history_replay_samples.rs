//! One bounded gap projection for persisted system and application series.

use crate::MAX_HISTORY_REPLAY_POINTS;
use crate::history_decimation::gap_preserving_envelope;
use std::sync::Arc;

pub(crate) fn gap_aware_samples(
    samples: &Arc<[f32]>,
    sample_times_ms: &[u64],
    observed: usize,
    gaps: usize,
) -> Arc<[f32]> {
    if samples.len() < 2 || samples.len() != sample_times_ms.len() {
        return Arc::clone(samples);
    }
    let mut intervals = sample_times_ms
        .windows(2)
        .filter_map(|times| times[1].checked_sub(times[0]))
        .filter(|interval| *interval > 0)
        .collect::<Vec<_>>();
    intervals.sort_unstable();
    // Lower median: with one normal interval and one downtime interval,
    // the larger outage must not become the inferred cadence.
    let discontinuity = intervals
        .get((intervals.len().saturating_sub(1)) / 2)
        .copied()
        .map_or(u64::MAX, |cadence| cadence.saturating_mul(3));
    let absolute_discontinuity = u64::try_from(crate::MAX_TELEMETRY_INTERVAL.as_millis())
        .unwrap_or(u64::MAX)
        .saturating_mul(3);
    let downsampled = observed.saturating_add(gaps) > samples.len();
    let mut projected = Vec::with_capacity(samples.len().saturating_mul(2));
    projected.push(samples[0]);
    for index in 1..samples.len() {
        let previous = sample_times_ms[index - 1];
        let current = sample_times_ms[index];
        let interval = current.saturating_sub(previous);
        if current <= previous
            || interval > discontinuity
            || (!downsampled && interval > absolute_discontinuity)
        {
            projected.push(f32::NAN);
        }
        projected.push(samples[index]);
    }
    gap_preserving_envelope(&projected, MAX_HISTORY_REPLAY_POINTS).into()
}
