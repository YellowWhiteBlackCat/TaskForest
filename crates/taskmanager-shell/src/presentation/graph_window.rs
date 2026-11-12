//! The graph data-points window: how many samples a chart may draw.
//!
//! The render window has two owner facts: two samples make a shape, and the
//! window never exceeds the bounded history ring. The telemetry store owns
//! the ring bound; this module derives the render maximum from it and owns
//! the newest-N trailing slice. A preference cannot produce a different
//! window per frontend.

use taskmanager_telemetry_store::live_graph::MAX_HISTORY_CAPACITY;

/// Two samples make a shape: a one-sample window cannot show a trend.
pub const MIN_GRAPH_WINDOW_SAMPLES: usize = 2;

/// The render window never exceeds the bounded history ring.
pub const MAX_GRAPH_WINDOW_SAMPLES: usize = MAX_HISTORY_CAPACITY;

/// Clamp a configured data-points preference into the renderable window.
#[must_use]
pub const fn graph_window_samples(data_points: usize) -> usize {
    if data_points < MIN_GRAPH_WINDOW_SAMPLES {
        MIN_GRAPH_WINDOW_SAMPLES
    } else if data_points > MAX_GRAPH_WINDOW_SAMPLES {
        MAX_GRAPH_WINDOW_SAMPLES
    } else {
        data_points
    }
}

/// The newest `capacity` samples of a sample window — the trailing slice, so
/// a window longer than the series keeps its most recent samples only.
#[must_use]
pub fn latest_samples(samples: &[f32], capacity: usize) -> &[f32] {
    let start = samples.len().saturating_sub(capacity);
    &samples[start..]
}

#[cfg(test)]
#[path = "../../tests/headless/presentation/graph_window_tests.rs"]
mod tests;
