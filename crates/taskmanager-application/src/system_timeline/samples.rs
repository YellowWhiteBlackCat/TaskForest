//! Timestamp-spaced, peak-preserving cells with explicit missing intervals.

use super::TimelineSample;
use super::{MAX_GRAPH_POINTS, MetricWindow, SystemHistoryWindow, raw_readouts};
use crate::TelemetryInterval;
use std::sync::Arc;

pub(super) fn metric_window(
    samples: Vec<TimelineSample>,
    window: SystemHistoryWindow,
    anchor: u64,
) -> MetricWindow {
    let span = window.minutes() * 60_000;
    let selected = samples
        .iter()
        .filter(|sample| {
            let time = sample.completed_at_ms;
            time <= anchor && anchor.saturating_sub(time) <= span
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return MetricWindow {
            values: Arc::from([]),
            covered_ms: 0,
            readouts: Default::default(),
        };
    }
    let cadence = cadence_ms(&selected);
    let points = usize::try_from(span / cadence)
        .unwrap_or(MAX_GRAPH_POINTS)
        .saturating_add(1)
        .clamp(2, MAX_GRAPH_POINTS);
    let mut cells = vec![f32::NAN; points];
    let mut missing = vec![false; points];
    let raw = selected
        .iter()
        .map(|sample| match (sample.measured_at_ms, sample.value) {
            (Some(_), Some(value)) if value.is_finite() => value,
            _ => f32::NAN,
        })
        .collect::<Vec<_>>();
    let mut previous_time = None;
    for (sample, observation) in selected.iter().zip(&raw) {
        let time = sample.completed_at_ms;
        let cell = cell_for(time, anchor, span, points);
        if let Some(previous) = previous_time {
            if time <= previous {
                missing[cell] = true;
            } else if time.saturating_sub(previous) > cadence.saturating_mul(3) {
                // Empty cells already expose downtime. When a narrow gap shares
                // one coarse cell with a valid sample, that cell must stay missing.
                let before = cell_for(previous, anchor, span, points);
                if before == cell {
                    missing[cell] = true;
                }
                for absent in missing.iter_mut().take(cell).skip(before.saturating_add(1)) {
                    *absent = true;
                }
            }
        }
        if observation.is_finite() {
            if let Some(value) = cells.get_mut(cell) {
                *value = value.max(*observation);
            }
        } else if let Some(absent) = missing.get_mut(cell) {
            *absent = true;
        }
        previous_time = Some(time);
    }
    for (cell, absent) in cells.iter_mut().zip(missing) {
        if absent {
            *cell = f32::NAN;
        }
    }
    let oldest = selected
        .iter()
        .zip(&raw)
        .filter(|(_, value)| value.is_finite())
        .map(|(sample, _)| sample.completed_at_ms)
        .min()
        .unwrap_or(anchor);
    let mut readouts = raw_readouts(&raw);
    if previous_time.is_none_or(|last| anchor.saturating_sub(last) > cadence.saturating_mul(3)) {
        readouts.latest = None;
    }
    MetricWindow {
        values: cells.into(),
        covered_ms: anchor.saturating_sub(oldest),
        readouts,
    }
}

fn cadence_ms(samples: &[&TimelineSample]) -> u64 {
    let mut intervals = samples
        .windows(2)
        .filter_map(|pair| pair[1].completed_at_ms.checked_sub(pair[0].completed_at_ms))
        .filter(|interval| *interval > 0)
        .collect::<Vec<_>>();
    intervals.sort_unstable();
    intervals
        .get(intervals.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(
            u64::try_from(TelemetryInterval::default().duration().as_millis()).unwrap_or(1_000),
        )
        .max(1)
}

fn cell_for(time: u64, anchor: u64, span: u64, points: usize) -> usize {
    let position = span.saturating_sub(anchor.saturating_sub(time));
    let steps = u64::try_from(points.saturating_sub(1)).unwrap_or(0);
    usize::try_from(position.saturating_mul(steps) / span.max(1)).unwrap_or(0)
}
