//! Single-source block-character sparkline for the TUI.
//!
//! Owned by the TUI: maps a bounded slice of finite samples onto the Unicode
//! half-block ramp. Both the App-history trend column and the Performance
//! trend strip project from this one implementation so the two trend views can
//! never drift apart. Per-row min/max normalization (Tufte sparkline
//! semantics) shows each series' recent SHAPE; the absolute value lives in the
//! adjacent text column when the caller renders one. The dual-direction rows
//! (disk read/write, NIC rx/tx) are the one deliberate exception: they share a
//! single min/max so the two directions stay comparable in amplitude.

use ratatui::style::Style;
use ratatui::text::Span;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::{bytes, graph_summary, missing_value};

/// How many of the most-recent samples a trend sparkline renders. Keeps every
/// trend cell a stable width regardless of the 64-sample window depth, and
/// keeps the per-frame allocation bounded.
pub(super) const SPARKLINE_MAX_SAMPLES: usize = 24;

/// The Unicode block ramp used for the sparkline, ordered low→high.
const SPARKLINE_BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Map a slice of samples onto a per-row-normalized block-character sparkline.
/// An empty slice renders empty; a constant or non-finite range renders as a
/// flat mid-ramp line — never a panic and never a fabricated trend.
pub(super) fn sparkline(samples: &[f32]) -> String {
    if samples.is_empty() {
        return String::new();
    }
    let max = samples.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min = samples.iter().copied().fold(f32::INFINITY, f32::min);
    let range = max - min;
    samples
        .iter()
        .map(|&value| {
            // A constant series (range 0, or non-finite guard) renders as the
            // mid-ramp block so a flat trend still reads as a flat trend.
            let normalized = if !range.is_finite() || range <= 0.0 {
                0.5_f32
            } else {
                ((value - min) / range).clamp(0.0, 1.0)
            };
            let idx = ((normalized * 7.0).round() as usize).min(7);
            SPARKLINE_BLOCKS[idx]
        })
        .collect()
}

/// The most-recent bounded window for `samples` (oldest→newest), so a caller
/// can render a stable-width trend from a deep ring without allocating more
/// than [`SPARKLINE_MAX_SAMPLES`].
pub(super) fn recent_window(samples: &[f32]) -> &[f32] {
    recent_window_with(samples, SPARKLINE_MAX_SAMPLES)
}

/// [`recent_window`] with an explicit window (the persisted
/// graph-data-points preference; the sparkline width adapts to it).
pub(super) fn recent_window_with(samples: &[f32], window: usize) -> &[f32] {
    let tail_start = samples.len().saturating_sub(window);
    &samples[tail_start..]
}

/// Minimum finite samples a per-device window needs before its trend renders a
/// sparkline. Below this the dotted placeholder is rendered: a single sample
/// cannot show a SHAPE, and drawing it as a one-block line would read as a
/// fabricated trend.
const MIN_DEVICE_TREND_SAMPLES: usize = 2;

/// The dotted "collecting" placeholder rendered when a per-device window has
/// fewer than [`MIN_DEVICE_TREND_SAMPLES`] finite samples — honest absence,
/// never a fabricated flat line. Uses the mid-dot (distinct from every ramp
/// block) so a reader can tell "no data yet" apart from a real flat trend.
const DEVICE_TREND_PLACEHOLDER: &str = "····";

/// Test-only sparkline helpers shared across the render test modules.
#[cfg(test)]
#[path = "../../tests/gui/ui/sparkline_test_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "../../tests/gui/ui/sparkline_tests.rs"]
mod tests;

/// [`device_trend`] with an explicit window (the persisted
/// graph-data-points preference).
pub(super) fn device_trend_with(samples: &[f32], window: usize) -> String {
    if samples.len() < MIN_DEVICE_TREND_SAMPLES {
        return DEVICE_TREND_PLACEHOLDER.to_string();
    }
    sparkline(recent_window_with(samples, window))
}

/// The glyph rendered for a missing sample (`NaN`) inside an otherwise live
/// dual-direction row — the same mid-dot as the cold-start placeholder,
/// distinct from every ramp block, so an explicit per-direction gap never
/// reads as a fabricated drop to the baseline block.
const DEVICE_TREND_GAP: char = '·';

/// The two rows of a per-device dual-direction trend (disk read/write, NIC
/// rx/tx). `primary` is the first-listed direction (disk read, NIC receive),
/// `secondary` its companion. Both rows normalize against the ONE min/max
/// shared by the pair, so a direction that dominates reads as dominant — the
/// terminal form of the iced two-series chart's shared-scale contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DeviceDualTrend {
    pub(super) primary: String,
    pub(super) secondary: String,
}

/// Project the two split-direction windows of one device (oldest→newest,
/// `NaN` = explicit gap) onto a pair of bounded, shared-scale sparkline rows.
/// Each direction keeps its own honest state: a row with fewer than
/// [`MIN_DEVICE_TREND_SAMPLES`] finite samples renders the dotted
/// "collecting" placeholder even while its companion plots, and a `NaN`
/// inside a live row renders the gap glyph instead of a baseline block.
pub(super) fn device_dual_trend_with(
    primary_samples: &[f32],
    secondary_samples: &[f32],
    window: usize,
) -> DeviceDualTrend {
    let primary_window = recent_window_with(primary_samples, window);
    let secondary_window = recent_window_with(secondary_samples, window);
    // One shared extent over BOTH windows' finite samples, so the pair's
    // amplitudes are directly comparable row-to-row.
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for value in primary_window
        .iter()
        .chain(secondary_window.iter())
        .copied()
        .filter(|value| value.is_finite())
    {
        min = min.min(value);
        max = max.max(value);
    }
    DeviceDualTrend {
        primary: dual_row(primary_window, min, max),
        secondary: dual_row(secondary_window, min, max),
    }
}

/// One dual-direction row: the dotted placeholder when this direction holds
/// fewer than [`MIN_DEVICE_TREND_SAMPLES`] finite samples, otherwise the
/// shared-normalization ramp where non-finite samples render [`DEVICE_TREND_GAP`].
fn dual_row(samples: &[f32], min: f32, max: f32) -> String {
    if samples.iter().filter(|value| value.is_finite()).count() < MIN_DEVICE_TREND_SAMPLES {
        return DEVICE_TREND_PLACEHOLDER.to_string();
    }
    let range = max - min;
    samples
        .iter()
        .map(|&value| {
            if !value.is_finite() {
                return DEVICE_TREND_GAP;
            }
            // A constant pair (shared range 0, or non-finite guard) renders as
            // the mid-ramp block so a flat trend still reads as a flat trend.
            let normalized = if !range.is_finite() || range <= 0.0 {
                0.5_f32
            } else {
                ((value - min) / range).clamp(0.0, 1.0)
            };
            SPARKLINE_BLOCKS[((normalized * 7.0).round() as usize).min(7)]
        })
        .collect()
}

/// One row of the two-row dual-direction trend: `label` padded to
/// `label_width` (the pair's common label width, so the two sparklines start
/// at the same column) followed by that direction's trend string. The label
/// stays unstyled; the trend carries the caller's direction color.
pub(super) fn dual_trend_line(
    label: &str,
    label_width: usize,
    trend: &str,
    style: Style,
) -> ratatui::text::Line<'static> {
    ratatui::text::Line::from(vec![
        Span::raw("  "),
        Span::raw(format!("{label:<label_width$} ")),
        Span::styled(trend.to_owned(), style),
    ])
}

/// Per-row CPU-history trend for one process: a bounded block-character
/// sparkline when at least two finite samples exist, otherwise the dotted
/// "collecting" placeholder. Mirrors the gpui per-row sparkline
/// (`processes_view/rows/cells.rs`) and the iced `process_sparkline` —
/// per-row min/max normalization is built into the shared [`sparkline`]
/// primitive so two rows aren't meant to be compared in amplitude.
///
/// Aggregate rows (group headers) and tree PARENT nodes carry no single CPU
/// history; the renderer filters those out and emits an honest `—` instead
/// of calling this helper. This helper only owns the leaf-row trend shape
/// (real sparkline vs the cold-start placeholder), reusing the SAME
/// finite-sample gate and bounded window as the per-device trends so the
/// two trend views can never drift apart.
pub(super) fn process_cpu_trend(samples: &[f32]) -> String {
    device_trend_with(samples, SPARKLINE_MAX_SAMPLES)
}

/// Units used by the compact Latest/Avg/Peak line beneath a per-device trend.
/// The reduction itself stays in the shared shell presentation layer; this
/// enum only selects the renderer-facing suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeviceSummaryUnit {
    Percent,
    BytesPerSecond,
    Rpm,
    /// Power draw in watts.
    Watts,
    /// Temperature in °C.
    Celsius,
}

fn summary_value(value: f32, unit: DeviceSummaryUnit) -> String {
    match unit {
        DeviceSummaryUnit::Percent => format!("{value:.0}%"),
        DeviceSummaryUnit::BytesPerSecond => {
            if value.is_finite() && value >= 0.0 {
                format!("{}/s", bytes(value.round() as u64))
            } else {
                missing_value()
            }
        }
        DeviceSummaryUnit::Rpm => format!("{value:.0} RPM"),
        DeviceSummaryUnit::Watts => format!("{value:.1} W"),
        DeviceSummaryUnit::Celsius => format!("{value:.0}°C"),
    }
}

/// Render one compact per-device summary using the same finite-sample rule as
/// Iced and GPUI. A single finite sample remains useful; an all-gap window
/// returns `None` so the caller does not paint fabricated statistics.
pub(super) fn device_summary_line(
    label: &str,
    samples: &[f32],
    unit: DeviceSummaryUnit,
) -> Option<String> {
    let summary = graph_summary(samples)?;
    Some(format!(
        "{label} · {} {} · {} {} · {} {}",
        t("common.latest"),
        summary_value(summary.latest, unit),
        t("common.avg"),
        summary_value(summary.average, unit),
        t("common.peak"),
        summary_value(summary.maximum, unit),
    ))
}
