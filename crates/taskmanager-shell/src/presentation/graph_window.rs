//! The graph data-points window: how many samples a chart may draw.
//!
//! Two facts used to live in each frontend. The clamp that bounds a
//! configured window (two samples make a shape; the window never exceeds the
//! bounded history ring) was spelled by GPUI's `graph_capacity` and Iced's
//! `ProcessPerfHistory` constructor. The trailing slice ("the newest N
//! samples of a window") was spelled three times: GPUI's `latest_samples_rc`,
//! Iced's `newest_tail` and the TUI's `recent_window_with`. One preference
//! could therefore produce three different windows.
//!
//! The ring bound stays owned by the telemetry store; the render window
//! derives its maximum from it, so a larger ring widens the window instead of
//! growing a second literal.

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
