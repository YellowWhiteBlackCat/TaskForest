//! Toolkit-neutral history-series decimation kernels — the single source every
//! frontend's replay/overview downsampling delegates to (ADR-020 single-source
//! rule; previously the GPUI scene cache carried the LTTB kernel and both the
//! GPUI and Iced history-replay panels carried byte-identical copies of the
//! stride-envelope kernel).
//!
//! # Kernel contracts (the adjudicated neutral semantics)
//!
//! The kernels serve distinct jobs; each documents its own
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
//! - [`crate::history_decimation::gap_preserving_envelope`] — coarse review cells keep any unavailable
//!   interval visible; complete cells retain their maximum.
//! - [`crate::history_decimation::lttb_gap_preserving_positions`] — the
//!   endpoint-preserving LTTB route for gapped series. It splits the input into
//!   finite runs, allocates one shared point budget across those runs, and
//!   inserts explicit gap separators. Renderers use this when their budget is
//!   expressed in pixels rather than coarse review cells.
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

/// Decimate a gapped series with LTTB while retaining every finite run as a
/// separate run. The returned positions are original indices into `samples`.
///
/// `target` is a budget for finite points, not for gap separators. Every run
/// with at least three points keeps its endpoints; the remaining budget is
/// distributed in proportion to each run's decimatable interior. If `target`
/// cannot give every run its minimum, or no decimation is needed, every finite
/// point is returned unchanged. This is deliberately fail-open on fidelity:
/// a tight budget never erases a run.
#[must_use]
pub fn lttb_gap_preserving_runs(samples: &[f32], target: usize) -> Vec<Vec<(usize, f32)>> {
    let mut runs = Vec::<Vec<(usize, f32)>>::new();
    let mut current = Vec::<(usize, f32)>::new();
    for (index, sample) in samples.iter().copied().enumerate() {
        if sample.is_finite() {
            current.push((index, sample));
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    if target == 0 || runs.is_empty() {
        return runs;
    }
    let finite_total = runs.iter().map(Vec::len).sum::<usize>();
    if target >= finite_total {
        return runs;
    }
    let minimums = runs.iter().map(|run| run.len().min(3)).collect::<Vec<_>>();
    let minimum_total = minimums.iter().sum::<usize>();
    if target < minimum_total {
        return runs;
    }

    let mut budgets = minimums;
    let capacities = runs
        .iter()
        .zip(&budgets)
        .map(|(run, minimum)| run.len().saturating_sub(*minimum))
        .collect::<Vec<_>>();
    let capacity_total = capacities.iter().sum::<usize>();
    if capacity_total == 0 {
        return runs;
    }
    let distributable = target.saturating_sub(minimum_total).min(capacity_total);
    let mut allocated = 0usize;
    let mut remainders = Vec::with_capacity(runs.len());
    for (index, capacity) in capacities.iter().copied().enumerate() {
        let numerator = distributable.saturating_mul(capacity);
        let share = numerator / capacity_total;
        budgets[index] = budgets[index].saturating_add(share);
        allocated = allocated.saturating_add(share);
        remainders.push((index, numerator % capacity_total));
    }
    let mut leftover = distributable.saturating_sub(allocated);
    remainders.sort_by(
        |(left_index, left_remainder), (right_index, right_remainder)| {
            right_remainder
                .cmp(left_remainder)
                .then_with(|| left_index.cmp(right_index))
        },
    );
    for (index, _) in remainders {
        if leftover == 0 {
            break;
        }
        if budgets[index] < runs[index].len() {
            budgets[index] = budgets[index].saturating_add(1);
            leftover = leftover.saturating_sub(1);
        }
    }

    runs.iter()
        .zip(budgets)
        .map(|(run, budget)| {
            lttb_indices(run, budget)
                .into_iter()
                .filter_map(|position| run.get(position).copied())
                .collect()
        })
        .collect()
}

/// Positions selected by [`lttb_gap_preserving_runs`], with one `None`
/// separator between finite runs. `target` bounds the total projected length,
/// including those separators. No separator is emitted for leading or trailing
/// gaps because a line chart has no run on that side to disconnect.
///
/// If `target` cannot represent every run with its own endpoints and one
/// separator, the function returns every finite point rather than erasing a
/// run. That fail-open result can exceed `target`; callers with a hard display
/// slot should treat the input as too fragmented for LTTB instead of silently
/// dropping evidence.
#[must_use]
pub fn lttb_gap_preserving_positions(samples: &[f32], target: usize) -> Vec<Option<usize>> {
    let run_count = lttb_gap_preserving_runs(samples, 0).len();
    let separator_count = run_count.saturating_sub(1);
    let finite_target = target.saturating_sub(separator_count);
    let runs = lttb_gap_preserving_runs(samples, finite_target);
    let mut positions = Vec::new();
    for (run_index, run) in runs.iter().enumerate() {
        if run_index > 0 {
            positions.push(None);
        }
        positions.extend(run.iter().map(|(index, _)| Some(*index)));
    }
    positions
}

/// Values selected by [`lttb_gap_preserving_positions`], with `NaN`
/// separators between finite runs.
#[must_use]
pub fn lttb_gap_preserving(samples: &[f32], target: usize) -> Vec<f32> {
    lttb_gap_preserving_positions(samples, target)
        .into_iter()
        .map(|position| position.map_or(f32::NAN, |position| samples[position]))
        .collect()
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

/// Coarse history cells retain an unavailable marker whenever their bucket
/// contains a gap, instead of connecting measurements across missing data.
#[must_use]
pub fn gap_preserving_envelope(samples: &[f32], target: usize) -> Vec<f32> {
    gap_preserving_envelope_positions(samples, target)
        .into_iter()
        .filter_map(|position| samples.get(position).copied())
        .collect()
}

/// Whole-window positions whose coarse cells retain any missing interval.
/// A mixed cell selects its first gap; a complete cell selects its maximum.
/// Timestamps and values use the same position, and the true scalar peak
/// remains separately available in the replay publication.
#[must_use]
pub fn gap_preserving_envelope_positions(samples: &[f32], target: usize) -> Vec<usize> {
    if target == 0 || samples.len() <= target {
        return (0..samples.len()).collect();
    }
    (0..target)
        .map(|bucket_index| {
            let offset = bucket_index.saturating_mul(samples.len()) / target;
            let end = bucket_index.saturating_add(1).saturating_mul(samples.len()) / target;
            let values = &samples[offset..end];
            let position = values
                .iter()
                .position(|value| !value.is_finite())
                .or_else(|| {
                    values
                        .iter()
                        .enumerate()
                        .max_by(|(_, left), (_, right)| left.total_cmp(right))
                        .map(|(index, _)| index)
                })
                .unwrap_or(0);
            offset.saturating_add(position)
        })
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
