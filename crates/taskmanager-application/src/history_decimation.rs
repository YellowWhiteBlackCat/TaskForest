//! Toolkit-neutral history-series decimation kernels — the single source every
//! frontend's replay/overview downsampling delegates to (ADR-020 single-source
//! rule; previously the GPUI scene cache carried the LTTB kernel and both the
//! GPUI and Iced history-replay panels carried byte-identical copies of the
//! stride-envelope kernel).
//!
//! # Kernel contracts (the adjudicated neutral semantics)
//!
//! Two kernels serve two honestly different jobs; each documents its own
//! endpoint and gap contract:
//!
//! - [`crate::history_decimation::lttb_indices`] — largest-triangle-three-buckets
//!   over ONE finite run of `(original_sample_index, value)` pairs. Shape-honest decimation for
//!   pixel-budgeted strokes: always keeps the run's first and last sample,
//!   emits strictly increasing positions, and preserves the original indices
//!   so time-axis spacing (including the gap-distorted spacing around a run)
//!   never lies. Gaps never cross a run boundary: the caller splits a gapped
//!   series into finite runs first (GPUI: `finite_sample_runs`) and decimates
//!   each run separately — a gap can never be smoothed away by this kernel.
//! - [`crate::history_decimation::stride_envelope`] — stride-bucket MAXIMUM
//!   envelope for whole-window overviews (a 7d window at live cadence far exceeds any graph's point
//!   budget). Spike honesty beats endpoint identity here: a spike anywhere
//!   inside a bucket — even at its start — stays visible, which is why this
//!   kernel intentionally does NOT keep each bucket's edge samples. A bucket
//!   with no finite value stays an explicit `NaN` gap.
//!
//! # Degenerate budgets (adjudicated: never erase)
//!
//! `budget < 3` (LTTB) and `target == 0` (stride) both fall back to the
//! identity selection/input. The safe contract is "no decimation" rather than
//! "no data": an empty or two-point result would render existing samples as a
//! blank/erased graph, which is dishonest, while a no-op merely costs the
//! caller its perf win for that frame. (A two-point LTTB line cannot represent
//! anything; callers that want endpoints-only must request `budget >= 3`.)
//!
//! The LTTB route follows the shared rendering contract in `docs/ARCH.md`; the
//! implementation below is the verbatim kernel that used to live in the GPUI
//! scene cache, promoted unchanged so both frontends (and the TUI later) share
//! it.

#![forbid(unsafe_code)]

/// Largest-triangle-three-buckets downsampling of one finite run.
///
/// Returns strictly increasing POSITIONS into `run` (not original sample
/// indices): always `0` first and `run.len() - 1` last when decimating
/// (`3 <= budget < run.len()`), exactly `budget` positions in total. Interior
/// buckets keep the point with the largest triangle area against the previous
/// selection and the next bucket's average, using the ORIGINAL sample index
/// as the x-coordinate, so gap-distorted spacing stays honest. Runs at or
/// below the budget — and degenerate budgets (`budget < 3`) — return the
/// identity positions `0..run.len()` (see the module docs: never erase).
///
/// # Examples
///
/// ```
/// use taskmanager_application::history_decimation::lttb_indices;
///
/// // A flat run decimates to deterministic bucket starts plus the endpoints.
/// let run: Vec<(usize, f32)> = (0..10).map(|index| (index, 0.0)).collect();
/// assert_eq!(lttb_indices(&run, 5), vec![0, 1, 3, 6, 9]);
/// // Within-budget and degenerate budgets never decimate.
/// assert_eq!(lttb_indices(&run, 10), (0..10).collect::<Vec<_>>());
/// assert_eq!(lttb_indices(&run, 0), (0..10).collect::<Vec<_>>());
/// ```
#[must_use]
pub fn lttb_indices(run: &[(usize, f32)], budget: usize) -> Vec<usize> {
    let len = run.len();
    if budget < 3 || budget >= len {
        return (0..len).collect();
    }
    let mut selected = Vec::with_capacity(budget);
    selected.push(0);
    let bucket = (len - 2) as f64 / (budget - 2) as f64;
    let mut previous = 0usize;
    for step in 0..budget - 2 {
        let next_start = ((step + 1) as f64 * bucket).floor() as usize + 1;
        let next_end = (((step + 2) as f64 * bucket).floor() as usize + 1).min(len - 1);
        let next_end = next_end.max(next_start + 1).min(len);
        let (mut sum_x, mut sum_y, mut count) = (0.0f64, 0.0f64, 0usize);
        for &(index, sample) in &run[next_start..next_end] {
            sum_x += index as f64;
            sum_y += sample as f64;
            count += 1;
        }
        let (avg_x, avg_y) = if count == 0 {
            // Unreachable with the clamped bucket bounds above (every next
            // bucket has at least one sample); kept as a typed fallback so
            // the kernel has no panic path on any input.
            (
                run[(next_start + 1).min(len - 1)].0 as f64,
                run[(next_start + 1).min(len - 1)].1 as f64,
            )
        } else {
            (sum_x / count as f64, sum_y / count as f64)
        };
        let current_start = (step as f64 * bucket).floor() as usize + 1;
        let current_end = (((step + 1) as f64 * bucket).floor() as usize + 1).min(len - 1);
        let (px, py) = (run[previous].0 as f64, run[previous].1 as f64);
        let mut best = current_start.min(len - 1);
        let mut best_area = f64::MIN;
        let candidates = run
            .iter()
            .enumerate()
            .take(current_end.max(current_start + 1))
            .skip(current_start);
        for (position, &(x, y)) in candidates {
            let (x, y) = (x as f64, y as f64);
            // Cross-product triangle area (constant factor dropped).
            let area = ((px - avg_x) * (y - py) - (px - x) * (avg_y - py)).abs();
            if area > best_area {
                best_area = area;
                best = position;
            }
        }
        selected.push(best);
        previous = best;
    }
    selected.push(len - 1);
    selected
}

/// Stride-bucket MAXIMUM envelope of a (possibly gapped) sample curve.
///
/// Buckets are equal-width strides of `samples.len().div_ceil(target)`
/// samples; each contributes its maximum finite value, and a bucket with no
/// finite value stays an explicit `NaN` gap — gaps are never smoothed away,
/// and a spike anywhere inside a bucket (even at its start) survives. The
/// output length is at most `target`. An empty input, `target == 0`, or a
/// series at or below `target` returns the input unchanged (see the module
/// docs: never erase).
///
/// # Examples
///
/// ```
/// use taskmanager_application::history_decimation::stride_envelope;
///
/// // 10 samples / target 4 → buckets of 3 → envelope 2, 5, 8, 9.
/// let samples: Vec<f32> = (0..10).map(|index| index as f32).collect();
/// assert_eq!(stride_envelope(&samples, 4), vec![2.0, 5.0, 8.0, 9.0]);
/// // An all-gap bucket stays a gap; a mixed bucket keeps the finite max.
/// let gapped = vec![f32::NAN, 7.0, f32::NAN, 2.0];
/// assert_eq!(stride_envelope(&gapped, 1), vec![7.0]);
/// ```
#[must_use]
pub fn stride_envelope(samples: &[f32], target: usize) -> Vec<f32> {
    stride_envelope_positions(samples, target)
        .into_iter()
        .filter_map(|position| samples.get(position).copied())
        .collect()
}

/// Positions selected by [`stride_envelope`]. Keeping this authority beside
/// the value kernel lets persistent replay decimate timestamps and values with
/// exactly the same buckets instead of reconstructing a lossy time axis.
#[must_use]
pub fn stride_envelope_positions(samples: &[f32], target: usize) -> Vec<usize> {
    if target == 0 || samples.len() <= target {
        return (0..samples.len()).collect();
    }
    let bucket = samples.len().div_ceil(target);
    samples
        .chunks(bucket)
        .enumerate()
        .map(|(bucket_index, chunk)| {
            let offset = bucket_index.saturating_mul(bucket);
            chunk
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, value)| value.is_finite())
                .max_by(|(_, left), (_, right)| {
                    left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map_or_else(
                    || offset.saturating_add(chunk.len().saturating_sub(1)),
                    |(index, _)| offset.saturating_add(index),
                )
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/headless/application_history_decimation_tests.rs"]
mod tests;
